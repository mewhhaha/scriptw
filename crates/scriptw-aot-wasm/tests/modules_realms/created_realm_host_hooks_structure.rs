use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{
    lower_with_host_surface_policy, HostBuiltinId, HostSurfacePolicy, StandardBuiltinId,
};
use std::collections::{BTreeMap, BTreeSet};
use wasmparser::{KnownCustom, Name, Parser, Payload, TypeRef, Validator, WasmFeatures};

#[test]
fn created_realm_global_carries_dollar262_detach() {
    std::thread::Builder::new()
        .name("created-realm-transitive-host-roots".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Neither the implicit detach hook nor hidden intrinsic bodies are
            // named by source. Creating the Realm must retain real code for them.
            let parsed = parse("__scriptwCreateRealm().global;", ParseOptions::script())
                .expect("created Realm source parses");
            let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
            let script = program
                .script
                .as_ref()
                .expect("created Realm source lowers");
            assert!(script.host_builtins.contains(&HostBuiltinId::CreateRealm));
            assert!(!script
                .host_builtins
                .contains(&HostBuiltinId::DetachArrayBuffer));
            let artifact = emit(&program).expect("created Realm and transitive bodies emit");
            let mut features = WasmFeatures::default();
            for feature in [
                WasmFeatures::THREADS,
                WasmFeatures::MULTI_MEMORY,
                WasmFeatures::REFERENCE_TYPES,
                WasmFeatures::FUNCTION_REFERENCES,
                WasmFeatures::GC,
                WasmFeatures::EXCEPTIONS,
                WasmFeatures::TAIL_CALL,
            ] {
                features.set(feature, true);
            }
            let runtime = artifact.runtime().expect("created Realm links R");
            let mut names = BTreeMap::new();
            let mut bodies = BTreeSet::new();
            for (module, bytes) in [runtime.bytes(), artifact.bytes.as_slice()]
                .into_iter()
                .enumerate()
            {
                Validator::new_with_features(features)
                    .validate_all(bytes)
                    .expect("created Realm callable entries preserve valid R and P Wasm");
                let mut index = 0u32;
                for payload in Parser::new(0).parse_all(bytes) {
                    match payload.expect("module decodes") {
                        Payload::ImportSection(section) => {
                            for import in section.into_imports() {
                                if matches!(
                                    import.expect("import decodes").ty,
                                    TypeRef::Func(_) | TypeRef::FuncExact(_)
                                ) {
                                    index += 1;
                                }
                            }
                        }
                        Payload::CodeSectionEntry(body) => {
                            assert!(!body.range().is_empty());
                            assert!(
                                bodies.insert((module, index)),
                                "each module owns unique bodies"
                            );
                            index += 1;
                        }
                        Payload::CustomSection(section) => {
                            if let KnownCustom::Name(subsections) = section.as_known() {
                                for subsection in subsections {
                                    if let Name::Function(map) =
                                        subsection.expect("name subsection decodes")
                                    {
                                        for naming in map {
                                            let naming = naming.expect("function name decodes");
                                            assert!(names
                                                .insert(
                                                    (module, naming.index),
                                                    naming.name.to_string()
                                                )
                                                .is_none());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            for name in [
                format!("host::{}", HostBuiltinId::DetachArrayBuffer.as_str()),
                format!(
                    "builtin::{}",
                    StandardBuiltinId::AbstractModuleSourceConstructor.debug_name()
                ),
                format!(
                    "builtin::{}",
                    StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter.debug_name()
                ),
            ] {
                assert!(
                    names
                        .iter()
                        .any(|(index, actual)| actual == &name && bodies.contains(index)),
                    "implicit Realm callable has no real body: {name}"
                );
            }
        })
        .expect("compiler source worker starts")
        .join()
        .expect("created Realm source worker completes");
}
