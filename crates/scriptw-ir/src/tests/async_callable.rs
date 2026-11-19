#[test]
fn records_named_async_function_expression() {
    let program = lower_script(
        "const resume = async function inner(value) {
                 await Promise.resolve();
                 return value;
             };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "inner")
        .expect("async function expression should be registered");

    assert_eq!(
        function.protocol.execution_kind(),
        FunctionExecutionKind::Async
    );
    assert!(!function.protocol.is_constructable());
    assert!(function.is_named_expression);
    assert!(function
        .body
        .statements
        .iter()
        .any(|statement| matches!(statement, StatementIr::AsyncAwait { .. })));
}

#[test]
fn records_async_object_method_as_non_constructable_async_function() {
    let program = lower_script(
        "const holder = {
                 marker: 40,
                 async method(delta) {
                     await Promise.resolve();
                     return this.marker + delta;
                 }
             };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let function = script
        .functions
        .iter()
        .find(|function| function.name == "method")
        .expect("async object method should be registered");

    assert_eq!(
        function.protocol.execution_kind(),
        FunctionExecutionKind::Async
    );
    assert!(!function.protocol.is_constructable());
    assert!(function.body.statements.iter().any(|statement| matches!(
        statement,
        StatementIr::AsyncAwait {
            suspend_state: 0,
            resume_state: 1,
            ..
        }
    )));

    let StatementIr::Lexical { init, .. } = &script.body.statements[0] else {
        panic!("expected object binding");
    };
    let ExprIr::ObjectLiteral(properties) = &init.expr else {
        panic!("expected object literal");
    };
    assert!(properties.iter().any(|property| matches!(
        property,
        ObjectPropertyIr::Method {
            key,
            function: object_method,
        } if key == "method" && object_method.function_id() == &function.id
    )));
}

#[test]
fn object_literal_home_object_is_carried_by_method_protocol_and_super_references() {
    let program = lower_script(
        r#"
                const key = "computed";
                const holder = {
                    method(value = super.seed) { return super.seed; },
                    get read() { return super.seed; },
                    set write(value) { super.seed = value; },
                    [key]() { return super.seed; }
                };
            "#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("script ir should exist");
    let StatementIr::Lexical { init, .. } = &script.body.statements[1] else {
        panic!("expected object binding");
    };
    let ExprIr::ObjectLiteral(properties) = &init.expr else {
        panic!("expected object literal");
    };

    let mut method_id = None;
    let mut setter_id = None;
    let mut protocols = Vec::new();
    for property in properties {
        let function = match property {
            ObjectPropertyIr::Method { key, function } if key == "method" => {
                method_id = Some(function.function_id().clone());
                function
            }
            ObjectPropertyIr::Getter { function, .. } => function,
            ObjectPropertyIr::Setter { function, .. } => {
                setter_id = Some(function.function_id().clone());
                function
            }
            ObjectPropertyIr::ComputedMethod { function, .. } => function,
            _ => continue,
        };
        protocols.push(function.protocol());
        let lowered = script
            .functions
            .iter()
            .find(|candidate| &candidate.id == function.function_id())
            .expect("method carrier must name a lowered function");
        assert_eq!(lowered.protocol, function.protocol());
    }
    assert_eq!(
        protocols,
        [
            FunctionProtocolIr::ObjectMethod(FunctionExecutionKind::Ordinary),
            FunctionProtocolIr::ObjectGetter,
            FunctionProtocolIr::ObjectSetter,
            FunctionProtocolIr::ObjectMethod(FunctionExecutionKind::Ordinary),
        ]
    );

    let method = script
        .functions
        .iter()
        .find(|function| Some(&function.id) == method_id.as_ref())
        .expect("ordinary method function");
    let default_init = method.params[0]
        .default_init
        .as_ref()
        .expect("method default initializer");
    assert!(matches!(
        &default_init.expr,
        ExprIr::SuperPropertyRead { receiver, .. }
            if matches!(&receiver.expr, ExprIr::This)
    ));

    let setter = script
        .functions
        .iter()
        .find(|function| Some(&function.id) == setter_id.as_ref())
        .expect("setter function");
    assert!(setter.body.statements.iter().any(|statement| matches!(
        statement,
        StatementIr::Expression(TypedExpr {
            expr: ExprIr::SuperPropertyWrite { receiver, .. },
            ..
        }) if matches!(&receiver.expr, ExprIr::This)
    )));
}

#[test]
fn lowers_async_object_method_later_parameter_read_as_tdz_throw() {
    let program = lower_script("const holder = { async method(value = later, later) {} };");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "method")
        .expect("async object method should be registered");
    let default_init = function.params[0]
        .default_init
        .as_ref()
        .expect("first parameter should have a default initializer");

    assert!(matches!(
        default_init.expr,
        ExprIr::RuntimeThrow {
            name: NativeErrorKind::ReferenceError,
            ..
        }
    ));
}

fn instantiated_function_body(function: &FunctionIr) -> &BlockIr {
    function
        .body
        .statements
        .iter()
        .find_map(|statement| {
            let StatementIr::Block(body) = statement else {
                return None;
            };
            body.lexical_environment
                .as_ref()
                .filter(|environment| {
                    matches!(
                        environment.initialization,
                        LexicalEnvironmentInitializationIr::FunctionBody { .. }
                    )
                })
                .map(|_| body)
        })
        .unwrap_or(&function.body)
}

#[test]
fn records_instance_and_static_async_class_methods_with_resume_state() {
    let program = lower_script(
        "class Base {
                 method() { return this.marker; }
                 static staticMethod() { return this.marker; }
             }
             class Derived extends Base {
                 async method(delta = later, later) {
                     await Promise.resolve();
                     return super.method() + delta;
                 }
                 static async staticMethod(delta) {
                     await Promise.resolve();
                     return super.staticMethod() + delta;
                 }
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let async_methods = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .filter(|function| function.protocol.execution_kind() == FunctionExecutionKind::Async)
        .collect::<Vec<_>>();

    assert_eq!(async_methods.len(), 2);
    assert!(async_methods
        .iter()
        .all(|function| !function.protocol.is_constructable()));
    assert!(async_methods.iter().all(|function| {
        instantiated_function_body(function)
            .statements
            .iter()
            .any(|statement| {
                matches!(
                    statement,
                    StatementIr::AsyncAwait {
                        suspend_state: 0,
                        resume_state: 1,
                        ..
                    }
                )
            })
    }));
    let instance_method = async_methods
        .iter()
        .find(|function| !function.is_static_class_member)
        .expect("instance async method should be registered");
    assert!(matches!(
        instance_method.params[0]
            .default_init
            .as_ref()
            .map(|init| &init.expr),
        Some(ExprIr::RuntimeThrow {
            name: NativeErrorKind::ReferenceError,
            ..
        })
    ));
}

#[test]
fn records_async_arrows_with_lexical_captures_and_parameter_tdz() {
    let program = lower_script(
        "function viaEval() { eval(''); }
             function make(marker) {
                 const expression = async delta =>
                     this.value + arguments[0] + delta + (new.target === undefined ? 1 : 0);
                 const block = async (value = later, later) => {
                     await Promise.resolve();
                     return this.value + arguments[0] + value
                         + (new.target === undefined ? 1 : 0);
                 };
                 return [expression, block];
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let async_arrows = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .filter(|function| {
            function.protocol.flavor() == FunctionFlavor::Arrow
                && function.protocol.execution_kind() == FunctionExecutionKind::Async
        })
        .collect::<Vec<_>>();

    assert_eq!(async_arrows.len(), 2);
    assert!(async_arrows
        .iter()
        .all(|function| !function.protocol.is_constructable()));
    assert!(async_arrows
        .iter()
        .all(|function| function.captures_lexical_this));
    assert!(async_arrows
        .iter()
        .all(|function| function.captures_lexical_arguments));
    assert!(async_arrows.iter().all(|function| {
        function
            .captured_bindings
            .iter()
            .any(|binding| binding.name == LEXICAL_NEW_TARGET_NAME)
    }));
    assert!(async_arrows.iter().any(|function| {
        function
            .body
            .statements
            .iter()
            .any(|statement| matches!(statement, StatementIr::Return(_)))
    }));
    let block = async_arrows
        .iter()
        .find(|function| {
            instantiated_function_body(function)
                .statements
                .iter()
                .any(|statement| matches!(statement, StatementIr::AsyncAwait { .. }))
        })
        .expect("block-bodied async arrow should suspend");
    let Some(ExprIr::EnvironmentIdentifier(reference)) =
        block.params[0].default_init.as_ref().map(|init| &init.expr)
    else {
        panic!("the original forward default must read the live parameter cell");
    };
    assert_eq!(reference.name, "later");
    assert!(matches!(
        reference.operation,
        EnvironmentIdentifierOperationIr::Read
    ));
    assert_eq!(
        reference.resolution_start(),
        EnvironmentIdentifierResolutionStart::CurrentEnvironment
    );
    assert_eq!(block.params[1].name, "later");
    assert!(block.params[1].default_init.is_none());
    let Some(EvalEnvironmentRoleIr::Declarative { bindings, .. }) = &block.eval_environment else {
        panic!("the forward default's parameter record must be named at runtime");
    };
    for parameter in ["value", "later"] {
        let binding = bindings
            .iter()
            .find(|binding| binding.source_name == parameter)
            .expect("both formal cells precede default evaluation");
        assert_eq!(binding.declaration, EvalBindingDeclarationIr::Parameter);
        assert!(block
            .owned_env_bindings
            .iter()
            .any(|owned| owned.name == parameter && owned.slot == binding.slot));
    }
}

#[test]
fn lowers_expression_bodied_async_arrow_awaits_in_source_order() {
    let program = lower_script("const add = async () => await 1 + await 2;");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| {
            function.protocol.flavor() == FunctionFlavor::Arrow
                && function.protocol.execution_kind() == FunctionExecutionKind::Async
        })
        .expect("async arrow should be lowered");
    let StatementIr::Block(block) = &function.body.statements[0] else {
        panic!("expression-bodied async arrow should contain a linear await block");
    };

    assert!(matches!(
        &block.statements[1],
        StatementIr::AsyncAwait {
            suspend_state: 0,
            resume_state: 1,
            resume_mode: AsyncResumeModeIr::AssignIdentifier(_),
            ..
        }
    ));
    assert!(matches!(
        &block.statements[3],
        StatementIr::AsyncAwait {
            suspend_state: 1,
            resume_state: 2,
            resume_mode: AsyncResumeModeIr::AssignIdentifier(_),
            ..
        }
    ));
    assert!(matches!(&block.statements[4], StatementIr::Return(_)));
}

#[test]
fn records_async_try_catch_finally_resume_boundaries() {
    let program = lower_script(
        "const settle = async function() {
                 try { await Promise.reject(\"early\"); }
                 catch (error) { await Promise.resolve(error); }
                 finally { return await Promise.resolve(\"override\"); }
             };",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.protocol.execution_kind() == FunctionExecutionKind::Async)
        .expect("async function expression should be registered");
    let StatementIr::TryCatchFinally {
        try_block,
        catch_block,
        finally_block,
        async_plan,
        ..
    } = &function.body.statements[0]
    else {
        panic!("expected async try/catch/finally statement");
    };

    assert_eq!(
        *async_plan,
        Some(AsyncTryPlanIr {
            entry_state: 0,
            try_exit_state: 2,
            catch_entry_state: Some(2),
            catch_exit_state: Some(4),
            finally_entry_state: Some(4),
            finally_exit_state: Some(6),
            exit_state: 6,
        })
    );
    assert!(matches!(
        try_block.statements[0],
        StatementIr::AsyncAwait {
            suspend_state: 0,
            resume_state: 1,
            resume_mode: AsyncResumeModeIr::Ignore,
            ..
        }
    ));
    assert!(matches!(
        catch_block.statements[0],
        StatementIr::AsyncAwait {
            suspend_state: 2,
            resume_state: 3,
            resume_mode: AsyncResumeModeIr::Ignore,
            ..
        }
    ));
    assert!(matches!(
        finally_block.statements[0],
        StatementIr::AsyncAwait {
            suspend_state: 4,
            resume_state: 5,
            resume_mode: AsyncResumeModeIr::Return,
            ..
        }
    ));
}

#[test]
fn async_generator_try_catch_shares_preplanned_clause_boundaries() {
    let program = lower_script(
        "async function* outer(source) {
                 let caught;
                 try { yield* source; }
                 catch (error) { caught = error; }
                 return caught;
             }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let function = program
        .script
        .as_ref()
        .expect("script ir should exist")
        .functions
        .iter()
        .find(|function| function.name == "outer")
        .expect("async generator should be registered");
    assert_eq!(
        function.resumable_plan,
        Some(ResumablePlanIr {
            entry_state: 0,
            state_count: 5,
            suspension_points: vec![
                ResumableSuspensionPointIr {
                    kind: ResumableSuspensionKindIr::Yield,
                    suspend_state: 0,
                    resume_state: 1,
                    resume_environment: ResumableResumeEnvironmentIr::InvocationOuter,
                },
                ResumableSuspensionPointIr {
                    kind: ResumableSuspensionKindIr::Await,
                    suspend_state: 3,
                    resume_state: 4,
                    resume_environment: ResumableResumeEnvironmentIr::SavedLexicalChain,
                },
            ],
            ..function.resumable_plan.as_ref().unwrap().clone()
        })
    );
    let [StatementIr::Lexical { .. }, StatementIr::TryCatch {
        generator_plan: Some(generator_plan),
        async_plan: Some(async_plan),
        ..
    }, StatementIr::AsyncAwait {
        suspend_state: 3,
        resume_state: 4,
        resume_mode: AsyncResumeModeIr::Return,
        ..
    }] = function.body.statements.as_slice()
    else {
        panic!(
            "expected planned async-generator try/catch followed by terminal Await: {:#?}",
            function.body.statements
        );
    };
    let expected_try_plan = AsyncTryPlanIr {
        entry_state: 0,
        try_exit_state: 2,
        catch_entry_state: Some(2),
        catch_exit_state: Some(3),
        finally_entry_state: None,
        finally_exit_state: None,
        exit_state: 3,
    };
    assert_eq!(*async_plan, expected_try_plan);
    assert_eq!(
        *generator_plan,
        GeneratorTryPlanIr {
            entry_state: expected_try_plan.entry_state,
            try_exit_state: expected_try_plan.try_exit_state,
            catch_entry_state: expected_try_plan.catch_entry_state,
            catch_exit_state: expected_try_plan.catch_exit_state,
            finally_entry_state: expected_try_plan.finally_entry_state,
            finally_exit_state: expected_try_plan.finally_exit_state,
            exit_state: expected_try_plan.exit_state,
        }
    );
}
