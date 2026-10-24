//! One canonical compiled graph, reusable against any Realm's module cache.

use super::*;
mod evaluation_capability;

/// Only a source callable with no caller captures can be materialized by
/// the independent global-environment factory. Construction stays private.
#[derive(Debug, Clone)]
pub(crate) struct IndependentScriptImportDispatcher {
    function: FunctionId,
}

impl IndependentScriptImportDispatcher {
    fn new(function: &scriptw_ir::FunctionIr) -> Result<Self, EmitError> {
        if !function.captured_bindings.is_empty()
            || function.captures_private_environment
            || function.captures_lexical_this
            || function.captures_lexical_arguments
            || function.is_named_expression
            || function.protocol.execution_kind() != FunctionExecutionKind::Async
            || function.protocol.flavor() != scriptw_ir::FunctionFlavor::Ordinary
        {
            return Err(EmitError::unsupported(
                "an independent Script import dispatcher requires capture-free async source",
            ));
        }
        Ok(Self {
            function: function.id.clone(),
        })
    }

    pub(crate) fn function(&self) -> &FunctionId {
        &self.function
    }
}

#[derive(Clone)]
pub(crate) struct ModuleGraphPlan {
    graph: scriptw_ir::ModuleExecutionGraphIr,
    script_import_dispatchers:
        BTreeMap<scriptw_ir::ImportPhaseIr, IndependentScriptImportDispatcher>,
}

impl ModuleGraphPlan {
    pub(crate) fn from_script(script: &ScriptIr) -> Result<Option<Self>, EmitError> {
        let mut plan: Option<Self> = None;
        for body in script.executable_script_bodies() {
            for statement in &body.statements {
                let StatementIr::Expression(TypedExpr {
                    expr: ExprIr::ModuleExecutionGraph(graph),
                    ..
                }) = statement
                else {
                    continue;
                };
                if graph.initializer().is_none() {
                    return Err(EmitError::unsupported(
                        "root module graph requires its reusable initializer",
                    ));
                }
                if graph.realm_import_dispatcher().is_none() {
                    return Err(EmitError::unsupported(
                        "reusable module graph requires its intrinsic import-job dispatcher",
                    ));
                }
                if let Some(previous) = &plan {
                    if previous.graph != **graph {
                        return Err(EmitError::unsupported(
                            "one Realm module registry requires one canonical compiled graph",
                        ));
                    }
                } else {
                    let script_import_dispatchers = graph
                        .script_import_dispatchers()
                        .iter()
                        .map(|(phase, id)| {
                            let function = script
                                .functions
                                .iter()
                                .find(|function| &function.id == id)
                                .ok_or_else(|| {
                                    EmitError::unsupported(
                                    "Script import dispatcher requires its compiled source owner",
                                )
                                })?;
                            IndependentScriptImportDispatcher::new(function)
                                .map(|dispatcher| (*phase, dispatcher))
                        })
                        .collect::<Result<_, _>>()?;
                    plan = Some(Self {
                        graph: (**graph).clone(),
                        script_import_dispatchers,
                    });
                }
            }
        }
        Ok(plan)
    }

    pub(crate) fn script_import_dispatcher(
        &self,
        referrer: u32,
        phase: scriptw_ir::ImportPhaseIr,
    ) -> Option<&IndependentScriptImportDispatcher> {
        self.graph.script_import_dispatcher(referrer, phase)?;
        self.script_import_dispatchers.get(&phase)
    }

    pub(crate) fn module_entry(&self) -> Option<scriptw_ir::ModuleUnitId> {
        self.graph.module_entry()
    }

    pub(crate) fn record_count(&self) -> u32 {
        self.graph.record_count()
    }
    pub(crate) fn source_record(&self, module: u32) -> Option<&scriptw_ir::ModuleSourceRecordIr> {
        self.graph
            .source_records()
            .iter()
            .find(|source| source.module() == module)
    }
    pub(crate) fn initializer(&self) -> &FunctionId {
        self.graph
            .initializer()
            .expect("plan validated a reusable initializer")
    }
    pub(crate) fn realm_requests(
        &self,
    ) -> &BTreeMap<scriptw_ir::ModuleRequestKeyIr, scriptw_ir::RealmModuleResolutionIr> {
        self.graph.realm_requests()
    }
    pub(crate) fn realm_import_dispatcher(&self) -> &FunctionId {
        self.graph
            .realm_import_dispatcher()
            .expect("canonical graph owns a Realm dispatcher")
    }
}

#[cfg(test)]
mod tests {
    use super::IndependentScriptImportDispatcher;

    #[test]
    fn independent_script_dispatcher_proof_rejects_a_real_outer_lexical_capture() {
        let lower = |source: &str| {
            let parsed =
                scriptw_front::parse(source, scriptw_front::ParseOptions::script()).unwrap();
            let program = scriptw_ir::lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            program
                .script
                .unwrap()
                .functions
                .into_iter()
                .find(|function| function.name == "dispatcher")
                .unwrap()
        };
        let independent = lower("async function dispatcher() { return await 1; }");
        let proof = IndependentScriptImportDispatcher::new(&independent).unwrap();
        assert_eq!(proof.function(), &independent.id);
        let captured = lower("let outer = 1; async function dispatcher() { return await outer; }");
        assert!(
            !captured.captured_bindings.is_empty(),
            "the counterexample owns an actual capture"
        );
        assert!(
            IndependentScriptImportDispatcher::new(&captured).is_err(),
            "a global-source factory cannot satisfy this lexical dependency"
        );
    }
}
