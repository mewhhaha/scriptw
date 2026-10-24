use super::*;
use crate::{
    ExprIr, FunctionProtocolIr, ModuleGraphSources, ModuleKey, ModuleRequestKeyIr, ModuleSourceIr,
    StatementIr,
};

fn function(
    export: &str,
    module: &str,
    name: &str,
    result: ForeignResultType,
) -> ForeignFunctionDeclaration {
    ForeignFunctionDeclaration::new(
        export,
        module,
        name,
        vec![ForeignValueType::Number; 2],
        result,
    )
    .unwrap()
}
fn module(specifier: &str, export: &str) -> ForeignModuleDeclaration {
    ForeignModuleDeclaration::new(
        specifier,
        vec![function(
            export,
            "native",
            export,
            ForeignResultType::Number,
        )],
    )
    .unwrap()
}

#[test]
fn normalized_order_and_optional_error_authority_round_trip() {
    let first =
        ForeignModuleRegistry::new(vec![module("z:host", "z"), module("a:host", "a")]).unwrap();
    let second =
        ForeignModuleRegistry::new(vec![module("a:host", "a"), module("z:host", "z")]).unwrap();
    assert_eq!(first.to_normalized_json(), second.to_normalized_json());
    assert_eq!(
        first
            .functions()
            .map(|(id, f)| (id.index(), f.js_export()))
            .collect::<Vec<_>>(),
        vec![(0, "a"), (1, "z")]
    );
    assert_eq!(
        ForeignModuleRegistry::from_normalized_json(&first.to_normalized_json()).unwrap(),
        first
    );
    let throwing = first.clone().with_error_tag(ForeignErrorTag::code_v1());
    assert_ne!(first.to_normalized_json(), throwing.to_normalized_json());
    let decoded =
        ForeignModuleRegistry::from_normalized_json(&throwing.to_normalized_json()).unwrap();
    assert_eq!(decoded.error_tag().unwrap().module(), "scriptw_foreign");
    assert_eq!(decoded.error_tag().unwrap().name(), "error_v1");
    assert!(first.error_tag().is_none());
}

#[test]
fn duplicate_exports_modules_imports_and_reserved_namespaces_reject() {
    let f = function("add", "native", "add", ForeignResultType::Number);
    assert!(ForeignModuleDeclaration::new("dd:host", vec![f.clone(), f.clone()]).is_err());
    let m = ForeignModuleDeclaration::new("dd:host", vec![f]).unwrap();
    assert!(ForeignModuleRegistry::new(vec![m.clone(), m]).is_err());
    let a = ForeignModuleDeclaration::new(
        "a",
        vec![function("a", "native", "same", ForeignResultType::Number)],
    )
    .unwrap();
    let b = ForeignModuleDeclaration::new(
        "b",
        vec![function("b", "native", "same", ForeignResultType::Void)],
    )
    .unwrap();
    assert!(ForeignModuleRegistry::new(vec![a, b]).is_err());
    for namespace in ["scriptw_runtime", "scriptw_host", "scriptw_foreign"] {
        assert!(ForeignFunctionDeclaration::new(
            "x",
            namespace,
            "x",
            vec![],
            ForeignResultType::Void
        )
        .is_err());
    }
}

#[test]
fn persisted_schema_rejects_unknown_modes_types_versions_and_fields() {
    let registry = ForeignModuleRegistry::new(vec![module("dd:host", "add")]).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&registry.to_normalized_json()).unwrap();
    for (field, replacement) in [
        ("mode", serde_json::json!("future")),
        ("parameters", serde_json::json!(["string"])),
        ("result", serde_json::json!("symbol")),
    ] {
        let mut corrupt = value.clone();
        corrupt["modules"][0]["functions"][0][field] = replacement;
        assert!(ForeignModuleRegistry::from_normalized_json(
            &serde_json::to_vec(&corrupt).unwrap()
        )
        .is_err());
    }
    let mut corrupt = value.clone();
    corrupt["abi_version"] = serde_json::json!(1);
    assert!(
        ForeignModuleRegistry::from_normalized_json(&serde_json::to_vec(&corrupt).unwrap())
            .is_err()
    );
    let mut corrupt = value.clone();
    corrupt["ambient"] = serde_json::json!(true);
    assert!(
        ForeignModuleRegistry::from_normalized_json(&serde_json::to_vec(&corrupt).unwrap())
            .is_err()
    );
    let mut pretty = registry.to_normalized_json();
    pretty.push(b' ');
    assert!(ForeignModuleRegistry::from_normalized_json(&pretty).is_err());
    assert!(ForeignModuleRegistry::from_normalized_json(
        b"{\"abi_version\":2,\"abi_version\":2,\"error_tag\":null,\"modules\":[]}"
    )
    .is_err());
}

#[test]
fn async_mode_and_lossless_transfer_types_have_distinct_canonical_authority() {
    let parameters = vec![
        ForeignValueType::Number,
        ForeignValueType::StringUtf16,
        ForeignValueType::Bytes,
    ];
    for result in [
        ForeignResultType::Number,
        ForeignResultType::StringUtf16,
        ForeignResultType::Bytes,
        ForeignResultType::Void,
    ] {
        let synchronous = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
            "dd:host",
            vec![ForeignFunctionDeclaration::new(
                "exchange",
                "dd",
                "exchange",
                parameters.clone(),
                result,
            )
            .unwrap()],
        )
        .unwrap()])
        .unwrap();
        let asynchronous = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
            "dd:host",
            vec![ForeignFunctionDeclaration::new_async(
                "exchange",
                "dd",
                "exchange",
                parameters.clone(),
                result,
            )
            .unwrap()],
        )
        .unwrap()])
        .unwrap();
        assert_eq!(asynchronous.abi_version(), 2);
        assert_eq!(
            asynchronous.functions().next().unwrap().1.mode(),
            ForeignFunctionMode::Asynchronous
        );
        assert_ne!(
            synchronous.to_normalized_json(),
            asynchronous.to_normalized_json()
        );
        assert_eq!(
            ForeignModuleRegistry::from_normalized_json(&asynchronous.to_normalized_json())
                .unwrap(),
            asynchronous
        );
        assert_eq!(
            ForeignModuleRegistry::from_normalized_json(&synchronous.to_normalized_json()).unwrap(),
            synchronous
        );
    }
}

#[test]
fn asynchronous_number_declaration_lowers_as_a_promise_instead_of_a_number() {
    let module = ForeignModuleDeclaration::new(
        "dd:host",
        vec![ForeignFunctionDeclaration::new_async(
            "later",
            "dd",
            "later",
            vec![ForeignValueType::Number],
            ForeignResultType::Number,
        )
        .unwrap()],
    )
    .unwrap();
    let program = crate::lower_module_graph(&graph(
        module,
        "import {later} from 'dd:host'; export const promise=later(42);",
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    let declaration = script
        .functions
        .iter()
        .find(|function| function.name == "later")
        .unwrap();
    let returned = declaration
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        })
        .unwrap();
    assert!(matches!(returned.expr, ExprIr::ForeignCall(_)));
    assert_eq!(returned.kind, crate::ValueKind::Object);
    assert_eq!(
        declaration.params.len(),
        1,
        "private resolver parameters do not change JS function.length"
    );
    assert_eq!(declaration.protocol, FunctionProtocolIr::OrdinaryCallOnly);
}

fn graph(module: ForeignModuleDeclaration, source: &str) -> ModuleGraphSources {
    ModuleGraphSources {
        realm_requests: Default::default(),
        modules: vec![
            ModuleSourceIr::new(
                ModuleKey::from_host("entry"),
                source.to_owned(),
                "entry".to_owned(),
            ),
            ModuleSourceIr::foreign(ModuleKey::from_host("foreign"), module, None),
        ],
        entry: 0,
        resolutions: vec![(0, ModuleRequestKeyIr::plain("dd:host"), 1)],
    }
}

#[test]
fn imported_wrappers_have_real_foreign_ir_native_names_and_call_only_protocols() {
    let module = ForeignModuleDeclaration::new(
        "dd:host",
        vec![
            function("add", "dd", "add", ForeignResultType::Number),
            function("trace", "dd", "trace", ForeignResultType::Void),
        ],
    )
    .unwrap();
    let program = crate::lower_module_graph(&graph(
        module,
        "import {add as plus,trace} from 'dd:host'; export const values=[plus,trace];",
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    assert_eq!(script.foreign_modules.functions().count(), 2);
    for name in ["add", "trace"] {
        let function = script
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("declaration owns ordinary function metadata");
        assert_eq!(function.protocol, FunctionProtocolIr::OrdinaryCallOnly);
        assert!(!function.protocol.is_constructable());
        assert!(!function.captures_lexical_this);
        assert!(!function.captures_lexical_arguments);
        assert!(function.strict);
        assert_eq!(function.params.len(), 2);
        assert_eq!(
            function.to_string_representation.materialize(),
            format!("function {name}() {{ [native code] }}")
        );
        let returned = function
            .body
            .statements
            .iter()
            .find_map(|statement| match statement {
                StatementIr::Return(value) => Some(value),
                _ => None,
            })
            .expect("wrapper returns its actual operation");
        let ExprIr::ForeignCall(call) = &returned.expr else {
            panic!("wrapper must contain the real foreign operation: {returned:?}");
        };
        assert_eq!(call.operands().len(), 2);
        assert_eq!(
            script
                .foreign_modules
                .function(call.function())
                .unwrap()
                .js_export(),
            name
        );
    }
}

#[test]
fn ordinary_source_cannot_forge_foreign_provenance_and_payload_retains_native_target() {
    let parsed = scriptw_front::parse(
        "const __scriptw_foreign_0=function(p0,p1){return[p0,p1];};__scriptw_foreign_0;",
        scriptw_front::ParseOptions::script(),
    )
    .unwrap();
    let program = crate::lower(&parsed);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.unwrap();
    assert!(script.foreign_modules.is_empty());
    assert_eq!(
        script.functions[0].protocol,
        FunctionProtocolIr::OrdinaryCallAndConstruct,
        "untrusted lookalike keeps its ordinary source constructability"
    );
    let returned = script
        .functions
        .iter()
        .flat_map(|function| &function.body.statements)
        .find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        })
        .unwrap();
    assert!(matches!(returned.expr, ExprIr::ArrayLiteral(_)));
    let first = ModuleSourceIr::foreign(
        ModuleKey::from_host("foreign"),
        module("dd:host", "add"),
        None,
    );
    let changed = ForeignModuleDeclaration::new(
        "dd:host",
        vec![function(
            "add",
            "different-native",
            "add",
            ForeignResultType::Number,
        )],
    )
    .unwrap();
    let second = ModuleSourceIr::foreign(ModuleKey::from_host("foreign"), changed, None);
    assert_eq!(
        first.source_text(),
        second.source_text(),
        "scaffolding alone cannot identify native capability"
    );
    assert_ne!(first.payload_bytes(), second.payload_bytes());
}
