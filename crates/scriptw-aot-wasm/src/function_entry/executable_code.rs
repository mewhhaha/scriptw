//! Publication uses the declared entry's role and metadata together.

use super::*;
use crate::gc_types::{
    AsyncCallable, AsyncGeneratorCallable, BinaryNumericCallback, ExecutableCode,
    ExecutableCodeEntry, ExecutableCodeKind, ExecutableCodeMetadata, GcStackReference,
    GeneratorCallable, OrdinaryCallable, RuntimeSchema,
};

impl PlannedFunctionEntry {
    /// P-local immutable declaration identity; only its defining P may dispatch it.
    pub(crate) fn source_id(&self) -> i64 {
        i64::from(self.wasm_index)
    }

    /// The same planned kind validates required allocation ownership and code
    /// publication. Every actual source body uses defining program ownership.
    pub(crate) fn executable_kind(&self) -> Result<ExecutableCodeKind, EmitError> {
        match self.origin {
            EntryOrigin::User { .. } => Ok(ExecutableCodeKind::JavaScript),
            EntryOrigin::StandardBuiltin(_) => Ok(ExecutableCodeKind::StandardBuiltin),
            EntryOrigin::HostBuiltin(_) => Ok(ExecutableCodeKind::HostBuiltin),
            EntryOrigin::PreparedScript { .. } => Err(entry_error(
                "a prepared Script cannot publish a callable code record",
            )),
        }
    }

    pub(crate) fn emit_executable_code(
        &self,
        class_element_execution: ClassElementExecutionKind,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Result<GcStackReference<ExecutableCode>, EmitError> {
        let kind = self.executable_kind()?;
        let (standard_builtin, host_builtin) = match self.origin {
            EntryOrigin::StandardBuiltin(id) => (Some(id), None),
            EntryOrigin::HostBuiltin(id) => (None, Some(id)),
            EntryOrigin::User { .. } | EntryOrigin::PreparedScript { .. } => (None, None),
        };
        let metadata = ExecutableCodeMetadata {
            // This P-local identity also selects constructor plans inside that
            // P's retained instance-elements hook. Foreign Ps never look it up.
            source_id: self.source_id(),
            kind,
            standard_builtin,
            host_builtin,
            protocol: self.protocol(),
            class_element_execution,
        };
        let binary_numeric = self.binary_numeric().map(|entry| {
            schema
                .reserve_function_local::<BinaryNumericCallback, _>(function)
                .initialize(
                    schema.reference_binary_numeric_entry(entry, function),
                    function,
                )
        });
        macro_rules! publish {
            ($role:ident, $variant:ident) => {{
                let entry = schema
                    .reserve_function_local::<$role, _>(function)
                    .initialize(schema.reference_entry::<$role>(self, function)?, function);
                let published = schema.struct_type::<ExecutableCode>().publish(
                    ExecutableCodeEntry::$variant(&entry),
                    binary_numeric.as_ref(),
                    metadata,
                    function,
                )?;
                entry.clear(function);
                published
            }};
        }
        let published = match self.body() {
            PlannedBodyEntry::Ordinary(_) => publish!(OrdinaryCallable, Ordinary),
            PlannedBodyEntry::Generator(_) => publish!(GeneratorCallable, Generator),
            PlannedBodyEntry::Async(_) => publish!(AsyncCallable, Async),
            PlannedBodyEntry::AsyncGenerator(_) => publish!(AsyncGeneratorCallable, AsyncGenerator),
            PlannedBodyEntry::PreparedScript(_) => {
                unreachable!("Script rejected before publication")
            }
        };
        if let Some(entry) = binary_numeric {
            entry.clear(function);
        }
        Ok(published)
    }
}

#[cfg(test)]
mod program_ownership_tests {
    use super::*;

    #[test]
    fn native_and_source_entries_require_distinct_program_capture_policies() {
        let native = PlannedFunctionEntry::standard(StandardBuiltinId::FunctionPrototype, 17);
        let source = PlannedFunctionEntry {
            origin: EntryOrigin::User {
                id: "source-owner".into(),
                name: "sourceOwner".into(),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                binary_numeric: None,
            },
            wasm_index: 19,
        };
        let host = PlannedFunctionEntry::host(HostBuiltinId::HTMLDDA, 23);
        assert_eq!(
            native.executable_kind().unwrap(),
            ExecutableCodeKind::StandardBuiltin
        );
        assert_eq!(
            host.executable_kind().unwrap(),
            ExecutableCodeKind::HostBuiltin
        );
        assert_eq!(
            source.executable_kind().unwrap(),
            ExecutableCodeKind::JavaScript
        );
        assert!(source.has_source_activation());
        assert!(!native.has_source_activation());
        let script = PlannedFunctionEntry {
            origin: EntryOrigin::PreparedScript {
                id: "script-owner".into(),
                kind: PreparedScriptKind::IndirectEval,
            },
            wasm_index: 29,
        };
        assert!(script.executable_kind().is_err());
    }
}
