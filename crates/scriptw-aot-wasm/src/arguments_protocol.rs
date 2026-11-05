use scriptw_ir::{
    FunctionExecutionKind, FunctionFlavor, FunctionIr, FunctionProtocolIr, StatementIr,
};

use crate::EmitError;

mod strict_projection;
pub(crate) use strict_projection::{ArgumentsProjectionRead, StrictArgumentsProjection};

mod unobservable_leaf;
mod unobserved_strict;
pub(crate) use unobservable_leaf::UserFunctionActivationPlan;
pub(crate) use unobserved_strict::UnobservedStrictArguments;

/// Whether this invocation needs a materialized arguments object, and the only legal
/// construction protocol when it does.
///
/// See `docs/rust-rewrite/contracts/arguments-object-construction-protocol.md`.
pub(crate) struct FunctionArgumentsProtocol(FunctionArgumentsState);

enum FunctionArgumentsState {
    Pending(FunctionArgumentsKind),
    BoundAbsent,
    BoundPresent,
}

enum FunctionArgumentsKind {
    Absent,
    EmptyStrictBody(EmptyStrictBody),
    UnobservableLeaf,
    UnobservedStrict,
    Present(PresentArgumentsObjectProtocol),
}

/// A source body with no parameter, binding, closure or statement observer.
/// Only the IR admission below can authorize eliding its binding setup.
#[must_use = "the admitted empty body must retain its callable entry and epilogue"]
pub(crate) struct EmptyStrictBody {
    _private: (),
}

pub(crate) enum PresentArgumentsObjectProtocol {
    Unmapped(UnmappedArgumentsPlan),
    Mapped(MappedArgumentsPlan),
}

#[must_use = "the function arguments binding protocol must be consumed"]
pub(crate) struct ArgumentsBindingProtocol(Option<PresentArgumentsObjectProtocol>);

pub(crate) struct UnmappedArgumentsPlan {
    _private: (),
}

pub(crate) struct MappedArgumentsPlan {
    entries: Box<[MappedArgumentEntry]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MappedArgumentEntry {
    argument: ArgumentIndex,
    environment: ParameterEnvironmentSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArgumentIndex(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParameterEnvironmentSlot(u32);

impl FunctionArgumentsProtocol {
    pub(crate) const fn script_main() -> Self {
        Self(FunctionArgumentsState::Pending(
            FunctionArgumentsKind::Absent,
        ))
    }

    pub(crate) const fn strict_internal_callable() -> Self {
        Self(FunctionArgumentsState::Pending(
            FunctionArgumentsKind::Present(PresentArgumentsObjectProtocol::Unmapped(
                UnmappedArgumentsPlan { _private: () },
            )),
        ))
    }

    pub(crate) fn for_user_function(function: &FunctionIr) -> Result<Self, EmitError> {
        if arguments_are_unobservable(function) {
            return Ok(Self(FunctionArgumentsState::Pending(
                FunctionArgumentsKind::EmptyStrictBody(EmptyStrictBody { _private: () }),
            )));
        }
        match function.protocol.flavor() {
            FunctionFlavor::Arrow => Ok(Self(FunctionArgumentsState::Pending(
                FunctionArgumentsKind::Absent,
            ))),
            FunctionFlavor::Ordinary if function.strict || !has_simple_parameter_list(function) => {
                Ok(Self(FunctionArgumentsState::Pending(
                    FunctionArgumentsKind::Present(PresentArgumentsObjectProtocol::Unmapped(
                        UnmappedArgumentsPlan { _private: () },
                    )),
                )))
            }
            FunctionFlavor::Ordinary => Ok(Self(FunctionArgumentsState::Pending(
                FunctionArgumentsKind::Present(PresentArgumentsObjectProtocol::Mapped(
                    MappedArgumentsPlan::for_function(function)?,
                )),
            ))),
        }
    }

    pub(crate) fn take_for_binding(&mut self) -> Result<ArgumentsBindingProtocol, EmitError> {
        match core::mem::replace(&mut self.0, FunctionArgumentsState::BoundAbsent) {
            FunctionArgumentsState::Pending(
                FunctionArgumentsKind::Absent
                | FunctionArgumentsKind::EmptyStrictBody(_)
                | FunctionArgumentsKind::UnobservableLeaf
                | FunctionArgumentsKind::UnobservedStrict,
            ) => Ok(ArgumentsBindingProtocol(None)),
            FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(protocol)) => {
                self.0 = FunctionArgumentsState::BoundPresent;
                Ok(ArgumentsBindingProtocol(Some(protocol)))
            }
            FunctionArgumentsState::BoundAbsent => Err(EmitError::unsupported(
                "compiler invariant violated: function arguments protocol was bound more than once",
            )),
            FunctionArgumentsState::BoundPresent => {
                self.0 = FunctionArgumentsState::BoundPresent;
                Err(EmitError::unsupported(
                    "compiler invariant violated: function arguments protocol was bound more than once",
                ))
            }
        }
    }

    pub(crate) fn take_empty_strict_body(&mut self) -> Option<EmptyStrictBody> {
        match core::mem::replace(&mut self.0, FunctionArgumentsState::BoundAbsent) {
            FunctionArgumentsState::Pending(FunctionArgumentsKind::EmptyStrictBody(plan)) => {
                Some(plan)
            }
            other => {
                self.0 = other;
                None
            }
        }
    }

    pub(crate) const fn present(&self) -> Option<()> {
        match &self.0 {
            FunctionArgumentsState::Pending(
                FunctionArgumentsKind::Absent
                | FunctionArgumentsKind::EmptyStrictBody(_)
                | FunctionArgumentsKind::UnobservableLeaf
                | FunctionArgumentsKind::UnobservedStrict,
            )
            | FunctionArgumentsState::BoundAbsent => None,
            FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(_))
            | FunctionArgumentsState::BoundPresent => Some(()),
        }
    }
}

fn arguments_are_unobservable(function: &FunctionIr) -> bool {
    // Strict callables expose no legacy .arguments activation property. An
    // empty ordinary body with inert parameters has no remaining observer.
    // Eval-visible named-binding metadata alone is not an observer: no eval
    // or closure can run in this empty body or its inert parameter list.
    function.strict
        && function.protocol.flavor() == FunctionFlavor::Ordinary
        && function.protocol.execution_kind() == FunctionExecutionKind::Ordinary
        && function.protocol != FunctionProtocolIr::ClassConstructor
        && function.class_instance_element_plan.is_none()
        && function.generator_plan.is_none()
        && function.resumable_plan.is_none()
        && function.captured_bindings.is_empty()
        && function.body.lexical_environment.is_none()
        && function.body.statements.is_empty()
        && has_simple_parameter_list(function)
}

impl ArgumentsBindingProtocol {
    pub(crate) fn into_present(self) -> Option<PresentArgumentsObjectProtocol> {
        self.0
    }
}

impl MappedArgumentsPlan {
    fn for_function(function: &FunctionIr) -> Result<Self, EmitError> {
        let mut entries = Vec::with_capacity(function.params.len());
        for (index, parameter) in function.params.iter().enumerate() {
            if function.params[index + 1..]
                .iter()
                .any(|later| later.name == parameter.name)
            {
                continue;
            }

            let argument = u32::try_from(index).map(ArgumentIndex).map_err(|_| {
                mapped_arguments_invariant_error(
                    function,
                    format!("argument index {index} does not fit the backend index domain"),
                )
            })?;
            let mut matching_bindings = function
                .owned_env_bindings
                .iter()
                .filter(|binding| binding.name == parameter.name);
            let binding = matching_bindings.next().ok_or_else(|| {
                mapped_arguments_invariant_error(
                    function,
                    format!(
                        "parameter `{}` has no owned environment slot",
                        parameter.name
                    ),
                )
            })?;
            if matching_bindings.next().is_some() {
                return Err(mapped_arguments_invariant_error(
                    function,
                    format!(
                        "parameter `{}` has more than one owned environment slot",
                        parameter.name
                    ),
                ));
            }
            if function
                .owned_env_bindings
                .iter()
                .filter(|candidate| candidate.slot == binding.slot)
                .count()
                != 1
            {
                return Err(mapped_arguments_invariant_error(
                    function,
                    format!(
                        "parameter `{}` shares environment slot {} with another binding",
                        parameter.name, binding.slot
                    ),
                ));
            }

            entries.push(MappedArgumentEntry {
                argument,
                environment: ParameterEnvironmentSlot(binding.slot),
            });
        }
        Ok(Self {
            entries: entries.into_boxed_slice(),
        })
    }

    pub(crate) fn entries(&self) -> &[MappedArgumentEntry] {
        &self.entries
    }
}

impl MappedArgumentEntry {
    pub(crate) const fn argument_index(self) -> u32 {
        self.argument.0
    }

    pub(crate) const fn environment_slot(self) -> u32 {
        self.environment.0
    }
}

fn has_simple_parameter_list(function: &FunctionIr) -> bool {
    function
        .params
        .iter()
        .all(|parameter| !parameter.is_rest && parameter.default_init.is_none())
        && !function
            .body
            .statements
            .iter()
            .any(|statement| matches!(statement, StatementIr::ParameterInitialization { .. }))
}

fn mapped_arguments_invariant_error(function: &FunctionIr, detail: String) -> EmitError {
    EmitError::unsupported(format!(
        "compiler invariant violated while planning mapped arguments for `{}`: {detail}",
        function.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower, ProgramIr};

    fn lower_script(source: &str) -> ProgramIr {
        let parsed = parse(source, ParseOptions::script()).expect("script should parse");
        lower(&parsed)
    }

    fn only_function(program: &ProgramIr) -> &FunctionIr {
        let functions = &program
            .script
            .as_ref()
            .expect("script IR should exist")
            .functions;
        assert_eq!(functions.len(), 1, "fixture should lower one function");
        &functions[0]
    }

    fn protocol_for(source: &str) -> FunctionArgumentsProtocol {
        let program = lower_script(source);
        FunctionArgumentsProtocol::for_user_function(only_function(&program))
            .expect("valid lowered function should have an arguments protocol")
    }

    fn take_present(
        protocol: &mut FunctionArgumentsProtocol,
    ) -> Option<PresentArgumentsObjectProtocol> {
        protocol
            .take_for_binding()
            .expect("fresh protocol should be bindable")
            .into_present()
    }

    fn mapped_entries(protocol: &mut FunctionArgumentsProtocol) -> Vec<(u32, u32)> {
        let Some(PresentArgumentsObjectProtocol::Mapped(plan)) = take_present(protocol) else {
            panic!("expected a mapped arguments protocol");
        };
        plan.entries
            .iter()
            .map(|entry| (entry.argument.0, entry.environment.0))
            .collect()
    }

    #[test]
    fn function_arguments_protocol_distinguishes_absent_unmapped_and_mapped_empty() {
        assert!(take_present(&mut protocol_for("const f = (value) => value;")).is_none());

        for source in [
            "function f(value) { 'use strict'; }",
            "function f(value = 1) {}",
            "function f(...values) {}",
            "function f({ value }) {}",
        ] {
            assert!(matches!(
                take_present(&mut protocol_for(source)),
                Some(PresentArgumentsObjectProtocol::Unmapped(_))
            ));
        }

        let mut empty = protocol_for("function f() {}");
        assert!(matches!(
            take_present(&mut empty),
            Some(PresentArgumentsObjectProtocol::Mapped(_))
        ));
        let mut empty = protocol_for("function f() {}");
        assert!(mapped_entries(&mut empty).is_empty());
    }

    #[test]
    fn function_arguments_mapped_plan_uses_validated_slots_and_last_duplicate() {
        assert_eq!(
            mapped_entries(&mut protocol_for("function f(first, second) {}")),
            vec![(0, 0), (1, 1)]
        );
        assert_eq!(
            mapped_entries(&mut protocol_for("function f(value, other, value) {}")),
            vec![(1, 1), (2, 0)]
        );
    }

    #[test]
    fn function_arguments_protocol_can_bind_only_once() {
        let mut protocol = protocol_for("function f(value) {}");
        assert!(take_present(&mut protocol).is_some());

        let Err(error) = protocol.take_for_binding() else {
            panic!("a consumed arguments protocol must reject a second binding");
        };
        assert!(error.to_string().contains("was bound more than once"));
    }

    #[test]
    fn empty_strict_methods_do_not_materialize_unobservable_arguments() {
        let program =
            lower_script("class E { constructor() {} method() {} get value() {} set value(v) {} }");
        let functions = &program.script.as_ref().unwrap().functions;
        assert_eq!(functions.len(), 4);
        for function in functions {
            let mut protocol = FunctionArgumentsProtocol::for_user_function(function).unwrap();
            assert_eq!(
                take_present(&mut protocol).is_some(),
                function.protocol == FunctionProtocolIr::ClassConstructor,
            );
        }
        let mut empty = protocol_for("'use strict'; function f(value) {}");
        assert!(empty.take_empty_strict_body().is_some());
        assert!(empty.take_empty_strict_body().is_none());
        assert!(empty.take_for_binding().is_err());
    }

    #[test]
    fn observable_and_resumable_strict_arguments_keep_their_construction_protocol() {
        for source in [
            "'use strict'; function f(value) { return arguments; }",
            "'use strict'; function f(value) { return () => arguments; }",
            "'use strict'; function f(value) { return eval('arguments'); }",
            "'use strict'; function f(value = arguments.length) {}",
            "'use strict'; function f(...values) {}",
            "'use strict'; function f({ value }) {}",
            "'use strict'; function* f(value) {}",
            "'use strict'; async function f(value) {}",
        ] {
            let program = lower_script(source);
            let function = program
                .script
                .as_ref()
                .unwrap()
                .functions
                .iter()
                .find(|function| function.name == "f")
                .expect("outer function f");
            let mut protocol = FunctionArgumentsProtocol::for_user_function(function).unwrap();
            assert!(
                matches!(
                    take_present(&mut protocol),
                    Some(PresentArgumentsObjectProtocol::Unmapped(_))
                ),
                "arguments must remain materialized: {source}",
            );
        }
    }

    #[test]
    fn eval_visible_empty_strict_methods_have_no_arguments_observer() {
        let mut descriptor = b"SCRWDE01".to_vec();
        descriptor.extend_from_slice(&[0, 1, 0, 0]);
        descriptor.extend_from_slice(&0u64.to_le_bytes());
        let caller = scriptw_ir::RuntimeDirectEvalCaller::validate(
            scriptw_ir::RuntimeDirectEvalCallerDescriptor::decode(&descriptor).unwrap(),
            [],
        )
        .unwrap();
        let source = "(class E { method() {} get value() {} set value(v) {} })";
        let program = scriptw_ir::lower_runtime_direct_eval(
            &caller,
            &source.encode_utf16().collect::<Vec<_>>(),
            scriptw_ir::HostSurfacePolicy::Test262,
        )
        .unwrap();
        let methods = program
            .program()
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .filter(|f| {
                matches!(
                    f.protocol,
                    FunctionProtocolIr::ClassMethod(FunctionExecutionKind::Ordinary)
                        | FunctionProtocolIr::ClassGetter
                        | FunctionProtocolIr::ClassSetter
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(methods.len(), 3);
        for function in methods {
            assert!(function.eval_environment.is_some());
            let mut protocol = FunctionArgumentsProtocol::for_user_function(function).unwrap();
            assert!(take_present(&mut protocol).is_none());
        }
    }

    #[test]
    fn function_emission_rejects_missing_mapped_parameter_storage() {
        let mut program = lower_script("function f(value) {}");
        let function = program
            .script
            .as_mut()
            .expect("script IR should exist")
            .functions
            .first_mut()
            .expect("fixture should lower one function");
        function
            .owned_env_bindings
            .retain(|binding| binding.name != "value");

        let error = crate::emit(&program).expect_err("malformed mapped storage must be rejected");
        assert!(
            error
                .to_string()
                .contains("parameter `value` has no owned environment slot"),
            "unexpected error: {error}"
        );
    }
}
