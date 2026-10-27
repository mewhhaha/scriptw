//! Direct real-runtime helper coverage through the private R root owner.
use crate::{CompileOptions, Engine, RealmBuilder};

#[test]
fn gc_shared_decimal_helper_executes_the_full_unsigned_magnitude_domain() {
    use wasmparser::{ExternalKind, KnownCustom, Name, Parser, Payload};
    use wasmtime::Val;

    crate::configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_script("String(0);", CompileOptions::default())
        .unwrap();
    let artifact = compiler.emit_wasm(&unit).unwrap();
    let raw = artifact.runtime.as_ref().unwrap().bytes();
    let mut helper_index = None;
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(raw) {
        match payload.unwrap() {
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(subsections) = section.as_known() {
                    for subsection in subsections {
                        if let Name::Function(names) = subsection.unwrap() {
                            for name in names {
                                let name = name.unwrap();
                                if name.name == "helper::integer_decimal_string" {
                                    assert!(helper_index.replace(name.index).is_none());
                                }
                            }
                        }
                    }
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export.unwrap();
                    if export.kind == ExternalKind::Func {
                        exports.push((export.index, export.name.to_owned()));
                    }
                }
            }
            _ => {}
        }
    }
    let helper_index = helper_index.expect("the real compiled helper has a name");
    let export = exports
        .iter()
        .find(|(index, _)| *index == helper_index)
        .expect("R retains its real helper export")
        .1
        .clone();
    let mut config = wasmtime::Config::new();
    crate::configure_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let runtime = crate::EmbeddedRuntime::compile(&engine, &artifact).unwrap();
    let state = runtime.new_state(RealmBuilder::new().build(), ()).unwrap();
    let mut store = wasmtime::Store::new(&engine, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker = wasmtime::Linker::new(&engine);
    runtime.register(&mut store, &mut linker).unwrap();
    let mut instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert!(matches!(
        instance.initialize(&mut store).unwrap(),
        crate::EmbeddedEvaluation::Settled(crate::JsCompletion::Normal(_))
    ));
    // This private test deliberately reaches the actual retained R owner.
    // Helpers unused by P must remain absent from the public linker.
    let formatter = instance
        .roots
        .get_func(&mut store, &export)
        .expect("R retains the compiled decimal helper independent of P imports");
    for magnitude in [0u64, 1u64 << 63, u64::MAX] {
        for negative in [0, 1] {
            let mut result = [Val::AnyRef(None)];
            formatter
                .call(
                    &mut store,
                    &[Val::I64(magnitude as i64), Val::I32(negative)],
                    &mut result,
                )
                .unwrap();
            let Val::AnyRef(Some(string)) = result[0] else {
                panic!("actual StringValue result");
            };
            let string = string.as_struct(&store).unwrap().unwrap();
            let Val::AnyRef(Some(units)) = string
                .field(
                    &mut store,
                    scriptw_aot_wasm::GcSnapshotField::StringUnits.index() as usize,
                )
                .unwrap()
            else {
                panic!("actual immutable UTF16 backing");
            };
            let units = units.as_array(&store).unwrap().unwrap();
            let length = units.len(&store).unwrap();
            let actual = (0..length)
                .map(|index| {
                    let Val::I32(unit) = units.get(&mut store, index).unwrap() else {
                        panic!("packed code unit");
                    };
                    u16::try_from(unit).unwrap()
                })
                .collect::<Vec<_>>();
            let expected = format!("{}{magnitude}", if negative == 1 { "-" } else { "" });
            assert_eq!(String::from_utf16(&actual).unwrap(), expected);
            assert_eq!(
                length as usize,
                expected.len(),
                "exact capacity includes only an optional sign and digits"
            );
            store.gc(None).unwrap();
        }
    }
}
