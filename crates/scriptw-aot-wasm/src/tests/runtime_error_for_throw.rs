//! Actual shared throwing-tail ABI, R/P domains and ordered error publication.
use super::*;
use crate::gc_types::{
    GcHeapType, GcNullability, GlobalLedger, NativeErrorObject, RuntimeModuleTypes, StringValue,
};
use scriptw_ir::NativeErrorKind;

fn named(bytes: &[u8], requested: &str) -> u32 {
    let matches = function_names_in_bytes(bytes)
        .into_iter()
        .filter_map(|(index, name)| (name == requested).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one physical function for {requested}");
    matches[0]
}

fn bodies(bytes: &[u8]) -> Vec<(u32, Vec<Operator<'_>>)> {
    let mut index = imported_function_count_in_bytes(bytes);
    let mut result = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            result.push((
                index,
                body.get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect(),
            ));
            index += 1;
        }
    }
    result
}

fn body(bytes: &[u8], index: u32) -> Vec<Operator<'_>> {
    let mut current = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if current == index {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            current += 1;
        }
    }
    panic!("named function has a physical body");
}

fn exported(bytes: &[u8], name: &str, kind: wasmparser::ExternalKind) -> u32 {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.name == name {
                    assert_eq!(export.kind, kind);
                    return export.index;
                }
            }
        }
    }
    panic!("missing export {name}");
}

fn signature(bytes: &[u8], function: u32) -> wasmparser::FuncType {
    let mut types = Vec::new();
    let mut declarations = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().into_types());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.unwrap().ty
                    {
                        declarations.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                declarations.extend(functions.into_iter().map(Result::unwrap))
            }
            _ => {}
        }
    }
    let wasmparser::CompositeInnerType::Func(function) = &types
        [declarations[function as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("actual function signature");
    };
    function.clone()
}

fn reference<T: GcHeapType>(schema: &crate::gc_types::RuntimeSchema) -> wasmparser::ValType {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("concrete GC signature");
    };
    wasmparser::ValType::Ref(
        wasmparser::RefType::new(
            false,
            wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
        )
        .unwrap(),
    )
}

fn calls(ops: &[Operator<'_>], callee: u32) -> Vec<usize> {
    ops.iter()
        .enumerate()
        .filter_map(|(index, op)| {
            matches!(op,Operator::Call {function_index} if *function_index==callee).then_some(index)
        })
        .collect()
}

fn caller_sets_throw(
    ops: &[Operator<'_>],
    callee: u32,
    expected: Option<NativeErrorKind>,
) -> Vec<i32> {
    let sites = calls(ops, callee);
    assert!(!sites.is_empty(), "actual caller reaches the tail");
    let mut kinds = Vec::new();
    for site in sites {
        let Operator::I32Const { value: kind } = ops[site - 6] else {
            panic!("closed kind immediate");
        };
        assert!(NativeErrorKind::ALL
            .iter()
            .any(|candidate| *candidate as i32 == kind));
        if let Some(expected) = expected {
            assert_eq!(kind, expected as i32);
        }
        kinds.push(kind);
        assert!(
            matches!(&ops[site-6..site], [Operator::I32Const {value},Operator::LocalGet {..},Operator::LocalGet {..},Operator::LocalGet {..},Operator::LocalGet {..},Operator::RefAsNonNull] if *value==kind),
            "closed kind immediate, full prototype Value and nonnull String message cross the ABI"
        );
        assert!(ops[site+1..].windows(4).any(|window|matches!(window,
            [Operator::I32Const {value},Operator::LocalSet {..},Operator::I32Const {value:0},Operator::LocalSet {..}]
            if *value==crate::emit::CompletionKind::Throw.code() as i32)),"the caller publishes its Throw completion after the returned error");
    }
    kinds
}

#[test]
fn throwing_tail_uses_actual_r_export_and_compacted_p_import_with_caller_completion() {
    let artifact = emit_script(
        "var key='program-only-error-key'; try { null[key]; } catch(error) { error.message; }",
    )
    .unwrap();
    expect_valid_module(&artifact, 0);
    let runtime = artifact.runtime().unwrap();
    let bytes = runtime.bytes();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(bytes)
        .unwrap();
    let helper = named(bytes, "helper::runtime_error_for_throw");
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let actual = signature(bytes, helper);
    assert_eq!(
        actual.params(),
        [
            wasmparser::ValType::I32,
            wasmparser::ValType::I32,
            wasmparser::ValType::I64,
            wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
            reference::<StringValue>(schema)
        ]
    );
    assert_eq!(
        actual.results(),
        [reference::<NativeErrorObject>(schema)],
        "the tail returns one object, not the caller's Completion words"
    );
    let mut export_name = None;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == helper {
                    assert!(export_name.replace(export.name.to_owned()).is_none());
                }
            }
        }
    }
    let export_name = export_name.expect("R exports the actual shared helper");
    let mut imported = 0;
    let mut program_helper = None;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && import.name == export_name
                    {
                        assert!(program_helper.replace(imported).is_none());
                    }
                    imported += 1;
                }
            }
        }
    }
    let program_helper =
        program_helper.expect("the actual P caller retains the R export as a compacted import");
    assert_eq!(signature(&artifact.bytes, program_helper), actual);
    let program_callers = bodies(&artifact.bytes)
        .into_iter()
        .filter(|(_, ops)| !calls(ops, program_helper).is_empty())
        .collect::<Vec<_>>();
    assert!(!program_callers.is_empty());
    let kinds = program_callers
        .into_iter()
        .flat_map(|(_, ops)| caller_sets_throw(&ops, program_helper, None))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&(NativeErrorKind::TypeError as i32)));
    let runtime_caller = body(bytes, named(bytes, "helper::lexical_binding_uninitialized"));
    caller_sets_throw(
        &runtime_caller,
        helper,
        Some(NativeErrorKind::ReferenceError),
    );
}

// Read the exact R descriptor address and passive UTF16 segment selected by its
// real resolver. Do not construct a partial pool or guess a P/R ordinal domain.
fn runtime_names(bytes: &[u8], ordinals: &[u32]) -> Vec<String> {
    let resolver = body(bytes, named(bytes, "helper::pooled_string_resolve"));
    let address=resolver.windows(4).find_map(|ops|match ops {
        [Operator::I32Mul,Operator::I32Const {value},Operator::I32Add,Operator::I32Load {memarg}] if memarg.offset==0=>Some(*value as u32),_=>None,
    }).expect("real descriptor row address");
    let count = resolver
        .windows(2)
        .find_map(|ops| match ops {
            [Operator::I32Const { value }, Operator::I32GeU] => Some(*value as u32),
            _ => None,
        })
        .unwrap();
    let utf16_segment = resolver
        .iter()
        .find_map(|op| match op {
            Operator::ArrayNewData {
                array_data_index, ..
            } => Some(*array_data_index as usize),
            _ => None,
        })
        .unwrap();
    let mut data = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(section) = payload.unwrap() {
            data.extend(section.into_iter().map(Result::unwrap));
        }
    }
    assert!(matches!(
        &data[utf16_segment].kind,
        wasmparser::DataKind::Passive
    ));
    let utf16 = data[utf16_segment].data;
    ordinals
        .iter()
        .map(|&ordinal| {
            assert!(
                ordinal < count,
                "R name stays inside R's sealed descriptor domain"
            );
            let row_address = address
                .checked_add(ordinal.checked_mul(12).unwrap())
                .unwrap();
            let row = data
                .iter()
                .find_map(|segment| {
                    let wasmparser::DataKind::Active {
                        memory_index: 0,
                        offset_expr,
                    } = &segment.kind
                    else {
                        return None;
                    };
                    let Operator::I32Const { value } =
                        offset_expr.get_operators_reader().read().unwrap()
                    else {
                        panic!("constant active data address");
                    };
                    let offset = row_address.checked_sub(value as u32)? as usize;
                    segment.data.get(offset..offset.checked_add(12)?)
                })
                .expect("selected descriptor row is physically present in R");
            let words = row
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
                .collect::<Vec<_>>();
            assert_eq!(words[0], ordinal);
            let start = words[1] as usize;
            let end = start.checked_add(words[2] as usize * 2).unwrap();
            let units = utf16
                .get(start..end)
                .unwrap()
                .chunks_exact(2)
                .map(|unit| u16::from_le_bytes(unit.try_into().unwrap()))
                .collect::<Vec<_>>();
            String::from_utf16(&units).unwrap()
        })
        .collect()
}

#[test]
fn nine_kind_tail_allocates_before_names_publishes_supplied_message_and_traps_invalid_wire() {
    let artifact = emit_script("({key:'program-only-pooled-string'});").unwrap();
    let bytes = artifact.runtime().unwrap().bytes();
    let helper = named(bytes, "helper::runtime_error_for_throw");
    let allocate = named(bytes, "helper::runtime_error_object");
    let resolve = named(bytes, "helper::pooled_string_resolve");
    let ops = body(bytes, helper);
    let allocation = calls(&ops, allocate);
    assert_eq!(allocation.len(), 1);
    assert_eq!(
        ops.iter()
            .find_map(|op| if let Operator::Call { function_index } = op {
                Some(*function_index)
            } else {
                None
            }),
        Some(allocate)
    );
    assert!(
        !ops.iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewDefault { .. }
        )),
        "sharing introduces no packaging allocation"
    );
    let name = exported(
        bytes,
        crate::module::THROW_ERROR_NAME_EXPORT,
        wasmparser::ExternalKind::Global,
    );
    let message = exported(
        bytes,
        crate::module::THROW_ERROR_MESSAGE_EXPORT,
        wasmparser::ExternalKind::Global,
    );
    let constructor = exported(
        bytes,
        crate::module::THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
        wasmparser::ExternalKind::Global,
    );
    let writes = |global| {
        ops.iter()
            .enumerate()
            .filter_map(|(index, op)| {
                matches!(op,Operator::GlobalSet {global_index} if *global_index==global)
                    .then_some(index)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(writes(name).len(), 10);
    assert_eq!(writes(message).len(), 2);
    assert!(writes(constructor).is_empty());
    assert!(ops.iter().all(|op|!matches!(op,Operator::GlobalSet {global_index} if *global_index!=name && *global_index!=message)));
    assert!(allocation[0] < writes(name)[0] && allocation[0] < writes(message)[0]);
    assert!(calls(&ops, resolve)
        .iter()
        .all(|position| allocation[0] < *position));
    let Operator::LocalSet { local_index: error } = ops[allocation[0] + 1] else {
        panic!("fresh error rooted immediately");
    };
    let supplied = ops[..allocation[0]]
        .windows(2)
        .find_map(|window| match window {
            [Operator::LocalGet { local_index: 4 }, Operator::LocalSet { local_index }] => {
                Some(*local_index)
            }
            _ => None,
        })
        .expect("nonnull message ABI parameter is rooted once");
    assert!(
        matches!(&ops[allocation[0]-2..allocation[0]],[Operator::LocalGet {local_index},Operator::RefAsNonNull] if *local_index==supplied)
    );
    let publish = *writes(message).last().unwrap();
    assert!(
        matches!(&ops[publish-2..publish],[Operator::LocalGet {local_index},Operator::RefAsNonNull] if *local_index==supplied),
        "publish original message String without resolving it again"
    );
    assert_eq!(
        ops.iter()
            .rev()
            .find_map(|op| if let Operator::LocalGet { local_index } = op {
                Some(*local_index)
            } else {
                None
            }),
        Some(error),
        "return the original rooted allocation"
    );
    let selectors=ops.windows(4).enumerate().filter_map(|(start,window)|match window {
        [Operator::LocalGet {local_index:0},Operator::I32Const {value},Operator::I32Eq,Operator::If {..}]=>Some((start,*value)),_=>None,
    }).collect::<Vec<_>>();
    assert_eq!(selectors.len(), NativeErrorKind::ALL.len());
    let mut ordinals = Vec::new();
    let mut previous_else = None;
    let mut terminal = None;
    for ((start, wire), kind) in selectors.iter().copied().zip(NativeErrorKind::ALL) {
        assert_eq!(wire, kind as i32);
        if let Some(previous) = previous_else {
            assert_eq!(
                start,
                previous + 1,
                "false branch reaches the next closed kind test"
            );
        }
        let mut depth = 0;
        let mut otherwise = None;
        for (position, op) in ops.iter().enumerate().skip(start + 4) {
            match op {
                Operator::If { .. } | Operator::Block { .. } | Operator::Loop { .. } => depth += 1,
                Operator::End => depth -= 1,
                Operator::Else if depth == 0 => {
                    otherwise = Some(position);
                    break;
                }
                _ => {}
            }
        }
        let otherwise = otherwise.expect("kind branch has an explicit fallback");
        let selected = &ops[start + 4..otherwise];
        let resolved = calls(selected, resolve);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            selected
                .iter()
                .filter(|op| matches!(op, Operator::Call { .. }))
                .count(),
            1
        );
        let Operator::LocalGet {
            local_index: ordinal,
        } = selected[resolved[0] - 1]
        else {
            panic!("R pooled name operand");
        };
        let assignment = selected[..resolved[0]]
            .iter()
            .rposition(|op| matches!(op,Operator::LocalSet {local_index} if *local_index==ordinal))
            .unwrap();
        let Operator::I32Const { value } = selected[assignment - 1] else {
            panic!("R owns the name ordinal constant");
        };
        ordinals.push(value as u32);
        let publication = selected
            .iter()
            .position(|op| matches!(op,Operator::GlobalSet {global_index} if *global_index==name))
            .unwrap();
        assert!(resolved[0] < publication);
        previous_else = Some(otherwise);
        terminal = Some(otherwise + 1);
    }
    assert!(
        matches!(ops[terminal.unwrap()], Operator::Unreachable),
        "every unmatched wire value follows all false arms to an actual trap"
    );
    for invalid in [-1, 9, i32::MIN, i32::MAX] {
        assert!(selectors.iter().all(|(_, wire)| *wire != invalid));
    }
    assert_eq!(
        calls(&ops, resolve).len(),
        9,
        "no second message resolution occurs in the R tail"
    );
    assert_eq!(
        runtime_names(bytes, &ordinals),
        NativeErrorKind::ALL.map(|kind| kind.as_str().to_owned())
    );
    assert!(publish > terminal.unwrap());
}

#[test]
fn normal_cancellation_keeps_allocation_only_error_path_and_normal_completion() {
    let artifact = emit_script("({value:1});").unwrap();
    let bytes = artifact.runtime().unwrap().bytes();
    let ops = body(bytes, named(bytes, "helper::embedding_cancel"));
    assert_eq!(
        calls(&ops, named(bytes, "helper::runtime_error_object")).len(),
        1
    );
    assert!(calls(&ops, named(bytes, "helper::runtime_error_for_throw")).is_empty());
    for role in [
        crate::module::THROW_ERROR_NAME_EXPORT,
        crate::module::THROW_ERROR_MESSAGE_EXPORT,
        crate::module::THROW_ERROR_CONSTRUCTOR_NAME_EXPORT,
    ] {
        let global = exported(bytes, role, wasmparser::ExternalKind::Global);
        assert!(
            !ops.iter()
                .any(|op| matches!(op,Operator::GlobalSet {global_index} if *global_index==global)),
            "normal cancellation does not publish throw diagnostics"
        );
    }
    let signature = signature(bytes, named(bytes, "helper::embedding_cancel"));
    assert_eq!(
        signature.results().len(),
        crate::module::COMPLETION_TYPES.len()
    );
    let (position, kind) = ops.windows(4).enumerate().filter_map(|(position,window)| match window {
        [Operator::I32Const {value},Operator::LocalSet {..},Operator::I32Const {value:0},Operator::LocalSet {..}] => Some((position,*value)),
        _ => None,
    }).last().expect("completion kind and target are published");
    assert!(position > calls(&ops, named(bytes, "helper::runtime_error_object"))[0]);
    assert_eq!(
        kind,
        crate::emit::CompletionKind::Normal.code() as i32,
        "the returned completion is Normal, not an earlier initialized state"
    );
}
