//! Physical module declarations and attribution from their actual owners.

use super::*;
use crate::emitted_function::ModuleFunctionTable;
use std::borrow::Cow;

pub(super) fn append_function_attribution(
    debug_dump: &mut Vec<String>,
    function_table: &ModuleFunctionTable,
) -> Vec<EmittedFunctionSummary> {
    let function_sizes = function_table.summaries();
    debug_dump.push(EmittedFunctionSummary::attribution_line(
        "largest emitted function",
        EmittedFunctionSummary::largest(&function_sizes),
    ));
    debug_dump.push(EmittedFunctionSummary::attribution_line(
        "most locals in an emitted function",
        EmittedFunctionSummary::most_locals(&function_sizes),
    ));
    debug_dump.push(format!(
        "emitted code bytes: {}",
        function_table.total_body_bytes()
    ));
    if emit_size_report_requested() {
        debug_dump.extend(EmittedFunctionSummary::report_lines(
            &function_sizes,
            usize::MAX,
        ));
    }
    // The optional file sink retains that same report through CLI wrappers.
    write_size_report_file_if_requested(&function_sizes);
    function_sizes
}

pub(super) fn append_function_name_and_check_budget(
    module: &mut Module,
    debug_dump: &mut Vec<String>,
    function_table: &ModuleFunctionTable,
) -> Result<(), EmitError> {
    // Native symbol names and literal attribution use the same completed table.
    module.section(&function_table.name_section());
    if let Some(section) = function_table.literal_operand_section() {
        module.section(&section);
    }
    debug_dump.push(format!(
        "name section: {} named functions",
        function_table.records().len()
    ));

    // Enforce an emitted-body limit only when explicitly requested.
    if let Some(budget) = FunctionBodyBudget::from_env() {
        function_table.check_budget(budget)?;
    }

    Ok(())
}

pub(super) fn append_memory_attribution(
    debug_dump: &mut Vec<String>,
    string_pool: &StringPool,
    uses_heap: bool,
    uses_shared_memory: bool,
) {
    if !string_pool.bytes.is_empty() || uses_heap {
        if uses_shared_memory {
            debug_dump.push("memory: exported private linear memory".to_string());
        } else {
            debug_dump.push("memory: exported linear memory".to_string());
        }

        if !string_pool.bytes.is_empty() {
            debug_dump.push("data segments: 1".to_string());
        } else {
            debug_dump.push("data segments: 0".to_string());
        }
        if uses_heap {
            debug_dump.push("heap: enabled".to_string());
        }
    } else {
        debug_dump.push("memory: none".to_string());
        debug_dump.push("data segments: 0".to_string());
    }
}

pub(super) fn append_module_declarations(
    module: &mut Module,
    kind: ModuleKind<'_>,
    foreign_modules: &scriptw_ir::foreign::ForeignModuleRegistry,
) {
    module.section(&wasm_encoder::CustomSection {
        name: Cow::Borrowed(crate::RUNTIME_SOURCE_COMPILATION_PROFILE_CUSTOM_SECTION),
        data: Cow::Owned(vec![kind.source_compilation_profile().wire()]),
    });
    if !foreign_modules.is_empty() {
        module.section(&wasm_encoder::CustomSection {
            name: Cow::Borrowed(crate::FOREIGN_MODULES_CUSTOM_SECTION),
            data: Cow::Owned(foreign_modules.to_normalized_json()),
        });
    }
}
