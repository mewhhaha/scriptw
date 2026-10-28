//! Index/declaration controls use synthetic bodies, without claiming Wasm execution.
use super::*;
use crate::{
    code_sink::LocalDeclarations,
    function_layout::{FunctionIndexLayout, ModuleEntryRole},
    runtime_artifact::ModuleKind,
};
use wasm_encoder::Instruction;

fn body() -> Function {
    let mut body = Function::new(LocalDeclarations::from_types([]));
    body.instruction(&Instruction::End);
    body
}

fn identity(index: u32) -> FunctionIdentity {
    FunctionIdentity::Script {
        id: format!("index-{index}"),
        name: String::new(),
    }
}

#[test]
fn runtime_program_and_standalone_layouts_match_actual_emission_order() {
    let runtime = crate::runtime_artifact(&scriptw_intl::IntlDataSelection::new(
        scriptw_intl::IntlCompilationProfile::Minimal,
    ))
    .unwrap();
    for kind in [
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::Portable),
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::NativeHost),
        ModuleKind::Program(&runtime),
        ModuleKind::Standalone,
    ] {
        for imported in [0, 3] {
            for source_count in [0, 2] {
                for helper_count in [0, 2] {
                    let r_count = if kind.uses_heap() { source_count } else { 0 };
                    let r_helpers = if kind.uses_heap() { helper_count } else { 0 };
                    let p_helpers = if kind.uses_heap() { helper_count } else { 0 };
                    let layout = FunctionIndexLayout::new(
                        imported,
                        r_count,
                        0,
                        0,
                        r_helpers,
                        source_count,
                        0,
                        p_helpers,
                        kind,
                        ModuleEntryRole::for_module(kind),
                    );
                    let boundary = imported + r_count as u32 + r_helpers as u32;
                    assert_eq!(layout.first_program_index(), boundary);
                    let linked = matches!(kind, ModuleKind::Program(_));
                    assert_eq!(layout.program_initialize_index().is_some(), linked);
                    let main_count = usize::from(kind.compiles_program());
                    assert_eq!(
                        layout.program_prefix_count(),
                        main_count + usize::from(linked)
                    );
                    if let Some(initialize) = layout.program_initialize_index() {
                        assert_eq!(initialize.raw(), boundary + 1);
                    }
                    if source_count > 0 {
                        assert_eq!(
                            layout.program_callable_index(0),
                            boundary + main_count as u32 + u32::from(linked)
                        );
                    }
                    assert_eq!(
                        layout.defined_functions().end,
                        boundary
                            + main_count as u32
                            + u32::from(linked)
                            + source_count as u32
                            + p_helpers as u32
                    );
                    let first = if kind.compiles_runtime() {
                        imported
                    } else {
                        boundary
                    };
                    let mut code = ModuleCode::new(first);
                    let mut expected = Vec::new();
                    if kind.compiles_runtime() {
                        for index in imported..boundary {
                            let id = identity(index);
                            expected.push(id.clone());
                            code.push(EmittedFunction::new(id, body()));
                        }
                    }
                    if kind.compiles_program() {
                        expected.push(FunctionIdentity::Main);
                        code.push(EmittedFunction::main(body()));
                        if let Some(index) = layout.program_initialize_index() {
                            let mut initialize =
                                EmittedFunction::new(FunctionIdentity::ProgramInitialize, body());
                            initialize.declaration = FunctionDeclaration::ProgramInitialize(index);
                            code.push(initialize);
                            expected.push(FunctionIdentity::ProgramInitialize);
                        }
                        for index in 0..source_count {
                            let id = identity(layout.program_callable_index(index));
                            expected.push(id.clone());
                            code.push(EmittedFunction::new(id, body()));
                        }
                        for helper in RuntimeHelperId::ALL.iter().take(p_helpers) {
                            expected.push(FunctionIdentity::RuntimeHelper(*helper));
                            code.push(EmittedFunction::runtime_helper(*helper, body()));
                        }
                    }
                    let (_, _, table) = code.finish();
                    assert_eq!(
                        table
                            .records()
                            .iter()
                            .map(|record| record.identity.clone())
                            .collect::<Vec<_>>(),
                        expected
                    );
                    assert_eq!(
                        table
                            .records()
                            .iter()
                            .map(|record| record.wasm_index)
                            .collect::<Vec<_>>(),
                        (first..first + expected.len() as u32).collect::<Vec<_>>()
                    );
                    if linked {
                        let initialize = layout.program_initialize_index().unwrap();
                        assert_eq!(
                            table.type_indices()[(initialize.raw() - first) as usize],
                            StaticSignature::ProgramInitialize.type_index()
                        );
                    }
                    if matches!(kind, ModuleKind::Runtime(_)) {
                        assert_eq!(table.records().len(), r_count + r_helpers);
                    }
                    if matches!(kind, ModuleKind::Standalone) {
                        assert_eq!(table.records().len(), 1 + source_count);
                    }
                }
            }
        }
    }
}

#[test]
fn native_factory_layout_starts_with_its_typed_initializer_without_main() {
    let runtime = crate::runtime_artifact(&scriptw_intl::IntlDataSelection::new(
        scriptw_intl::IntlCompilationProfile::Minimal,
    ))
    .unwrap();
    let layout = FunctionIndexLayout::new(
        3,
        2,
        4,
        6,
        8,
        2,
        0,
        3,
        ModuleKind::Program(&runtime),
        ModuleEntryRole::NativeFactory,
    );
    let boundary = 3 + 2 + 4 + 6 + 8;
    assert_eq!(layout.main_index(), None);
    assert_eq!(layout.program_prefix_count(), 1);
    let initialize = layout.program_initialize_index().unwrap();
    assert_eq!(initialize.raw(), boundary);
    assert_eq!(layout.program_callable_index(0), boundary + 1);
    assert_eq!(layout.defined_functions().end, boundary + 1 + 2 + 3);
    let mut code = ModuleCode::new(boundary);
    let mut body = EmittedFunction::new(FunctionIdentity::ProgramInitialize, body());
    body.declaration = FunctionDeclaration::ProgramInitialize(initialize);
    code.push(body);
    let (_, _, table) = code.finish();
    assert_eq!(
        table.records()[0].identity,
        FunctionIdentity::ProgramInitialize
    );
    assert_eq!(table.records()[0].wasm_index, boundary);
    assert_eq!(
        table.type_indices(),
        [StaticSignature::ProgramInitialize.type_index()]
    );
}

#[test]
fn only_a_linked_program_can_have_a_native_factory_entry() {
    for kind in [
        ModuleKind::Standalone,
        ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::NativeHost),
    ] {
        assert!(std::panic::catch_unwind(|| FunctionIndexLayout::new(
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            kind,
            ModuleEntryRole::NativeFactory,
        ))
        .is_err());
    }
}

#[test]
#[should_panic(expected = "P initializer emitted outside its planned index")]
fn a_bootstrap_body_cannot_be_published_at_main_position() {
    let runtime = crate::runtime_artifact(&scriptw_intl::IntlDataSelection::new(
        scriptw_intl::IntlCompilationProfile::Minimal,
    ))
    .unwrap();
    let layout = FunctionIndexLayout::new(
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        ModuleKind::Program(&runtime),
        ModuleEntryRole::Main,
    );
    let index = layout.program_initialize_index().unwrap();
    let mut initialize = EmittedFunction::new(FunctionIdentity::ProgramInitialize, body());
    initialize.declaration = FunctionDeclaration::ProgramInitialize(index);
    ModuleCode::new(layout.main_index().unwrap()).push(initialize);
}
