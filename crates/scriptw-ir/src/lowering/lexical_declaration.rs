use super::*;

impl ScriptLowerer<'_> {
    pub(super) fn lower_lexical_declaration(
        &mut self,
        declaration: &LexicalDeclaration,
        scope: &mut LexicalScopeInstantiation,
    ) -> (StatementIr, ValueKind) {
        let (mode, list) = match declaration {
            LexicalDeclaration::Let(list) => (BindingMode::Let, list),
            LexicalDeclaration::Const(list) => (BindingMode::Const, list),
            LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_) => {
                unreachable!("statement-list lowering owns dedicated using declarations")
            }
        };

        let mut statements = Vec::with_capacity(list.as_ref().len());
        for variable in list.as_ref() {
            match variable.binding() {
                Binding::Identifier(identifier) => {
                    let name = self.interner.resolve_expect(identifier.sym()).to_string();
                    // 14.3.1.2 step 5 InitializeReferencedBinding: claim the
                    // obligation BlockDeclarationInstantiation left for this
                    // name. Every arm below either discharges it or drops it
                    // (`unsupported`), and none can discharge it twice.
                    let pending = scope.take(&name);
                    if let Some(target) = variable
                        .init()
                        .and_then(|init| {
                            self.analysis
                                .module_execution
                                .imports
                                .get(&(std::ptr::from_ref(init) as usize))
                        })
                        .cloned()
                    {
                        let statement = self.lower_lexical_binding_value(
                            BindingMode::Const,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(TypedExpr::from_info(
                                unknown_runtime_value_info(),
                                ExprIr::Undefined,
                            )),
                            pending,
                            None,
                        );
                        let StatementIr::Lexical { name, .. } = statement else {
                            panic!("module import creates one immutable lexical binding");
                        };
                        statements.push(StatementIr::ModuleImportBinding(
                            crate::ModuleImportBindingIr { name, target },
                        ));
                        continue;
                    }
                    if self.async_generator_entry_state().is_some()
                        && variable.init().is_some_and(|source| {
                            contains(source, ContainsSymbol::AwaitExpression)
                                || contains(source, ContainsSymbol::YieldExpression)
                        })
                    {
                        let Some((prefix, value)) = self.lower_mixed_generator_value(
                            variable.init().expect("checked mixed lexical initializer"),
                        ) else {
                            self.unsupported("mixed async-generator lexical initializer");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(prefix);
                        statements.push(self.lower_lexical_binding_value(
                            mode,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(value),
                            pending,
                            None,
                        ));
                        continue;
                    }
                    if self.current_async_resume_state.is_some()
                        && matches!(variable.init(), Some(Expression::Await(_)))
                    {
                        let Some(Expression::Await(await_expression)) = variable.init() else {
                            unreachable!()
                        };
                        let received_info = ValueInfo {
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                        };
                        let received_name = self.alloc_suspension_owned_binding(
                            "async.lexical.received.",
                            received_info,
                        );
                        statements.push(StatementIr::Lexical {
                            mode: BindingMode::Let,
                            name: received_name.clone(),
                            init: TypedExpr::undefined(),
                        });
                        let (await_statement, _) = self.lower_linear_async_await(
                            await_expression.target(),
                            AsyncResumeModeIr::AssignIdentifier(received_name.clone()),
                        );
                        statements.push(await_statement);
                        let init = self.lower_identifier_name(received_name, false);
                        statements.push(self.lower_lexical_binding_value(
                            mode,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(init),
                            pending,
                            None,
                        ));
                        continue;
                    }
                    if self.current_async_resume_state.is_some()
                        && variable.init().is_some_and(|expression| {
                            matches!(
                                Self::unwrap_parenthesized_expr(expression),
                                Expression::ArrayLiteral(array)
                                    if array.as_ref().iter().flatten().any(|element| {
                                        contains(element, ContainsSymbol::AwaitExpression)
                                    })
                            )
                        })
                    {
                        let Some(Expression::ArrayLiteral(array)) =
                            variable.init().map(Self::unwrap_parenthesized_expr)
                        else {
                            unreachable!()
                        };
                        if array
                            .as_ref()
                            .iter()
                            .flatten()
                            .any(|element| matches!(element, Expression::Spread(_)))
                        {
                            self.unsupported("async lexical array initializer spread");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        }
                        if array.as_ref().iter().flatten().any(|element| {
                            match Self::unwrap_parenthesized_expr(element) {
                                Expression::Await(await_expression) => contains(
                                    await_expression.target(),
                                    ContainsSymbol::AwaitExpression,
                                ),
                                _ => {
                                    contains(element, ContainsSymbol::AwaitExpression)
                                        && !self.async_initializer_is_stageable(element)
                                }
                            }
                        }) {
                            self.unsupported(
                                "async lexical array initializer composite await element",
                            );
                            return (StatementIr::Empty, ValueKind::Undefined);
                        }

                        let mut elements = Vec::with_capacity(array.as_ref().len());
                        for element in array.as_ref() {
                            let Some(element) = element else {
                                elements.push(TypedExpr::from_info(
                                    ValueInfo::undefined(),
                                    ExprIr::ArrayHole,
                                ));
                                continue;
                            };
                            let Expression::Await(await_expression) =
                                Self::unwrap_parenthesized_expr(element)
                            else {
                                let value = if contains(element, ContainsSymbol::AwaitExpression) {
                                    let (prefix, staged) = self
                                        .lower_async_prefixed_expression(element)
                                        .expect("stageable async array element must stage");
                                    statements.extend(prefix);
                                    staged
                                } else {
                                    self.lower_expression(element)
                                };
                                let element_name = self.alloc_suspension_owned_binding(
                                    "async.array.element.",
                                    value.value_info(),
                                );
                                statements.push(StatementIr::Lexical {
                                    mode: BindingMode::Let,
                                    name: element_name.clone(),
                                    init: value,
                                });
                                elements.push(self.lower_identifier_name(element_name, false));
                                continue;
                            };
                            let received_name = self.alloc_suspension_owned_binding(
                                "async.array.received.",
                                ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::all_runtime_tags(),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::unknown(),
                                },
                            );
                            statements.push(StatementIr::Lexical {
                                mode: BindingMode::Let,
                                name: received_name.clone(),
                                init: TypedExpr::undefined(),
                            });
                            let (await_statement, _) = self.lower_linear_async_await(
                                await_expression.target(),
                                AsyncResumeModeIr::AssignIdentifier(received_name.clone()),
                            );
                            statements.push(await_statement);
                            elements.push(self.lower_identifier_name(received_name, false));
                        }
                        let init = Self::array_literal_from_lowered(elements);
                        statements.push(self.lower_lexical_binding_value(
                            mode,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(init),
                            pending,
                            None,
                        ));
                        continue;
                    }
                    if variable
                        .init()
                        .is_some_and(|init| self.async_initializer_is_stageable(init))
                    {
                        let (prefix, init) = self
                            .lower_async_prefixed_expression(
                                variable
                                    .init()
                                    .expect("guarded async lexical initializer must exist"),
                            )
                            .expect("stageable async lexical initializer must stage");
                        statements.extend(prefix);
                        statements.push(self.lower_lexical_binding_value(
                            mode,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(init),
                            pending,
                            None,
                        ));
                        continue;
                    }
                    if self.current_async_resume_state.is_some()
                        && variable
                            .init()
                            .is_some_and(|init| self.has_branch_sensitive_await(init))
                    {
                        self.unsupported(
                            "async lexical initializer branch-sensitive await expression",
                        );
                        return (StatementIr::Empty, ValueKind::Undefined);
                    }
                    if self.current_generator_resume_state.is_some()
                        && variable
                            .init()
                            .is_some_and(|init| contains(init, ContainsSymbol::YieldExpression))
                    {
                        let Some((prefix, init)) = self.lower_staged_generator_expression(
                            variable
                                .init()
                                .expect("guarded generator initializer exists"),
                        ) else {
                            self.unsupported("generator lexical initializer suspension expression");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(prefix);
                        statements.push(self.lower_lexical_binding_value(
                            mode,
                            name,
                            identifier.span(),
                            LoweredInitializer::evaluated(init),
                            pending,
                            None,
                        ));
                        continue;
                    }
                    if variable.init().is_some_and(|expression| {
                        self.is_constructor_prototype_expr(expression, ARRAY_NAME)
                    }) {
                        self.array_prototype_mutated = true;
                    }
                    let static_to_string_regexp_object =
                        variable.init().is_some_and(|expression| {
                            self.static_to_string_returns_regexp_object_expr(expression)
                        });
                    let static_string_value = variable
                        .init()
                        .and_then(|expression| self.static_string_expression(expression));
                    let source_candidate = variable
                        .init()
                        .map(|expression| self.function_source_value_candidates(expression));
                    let init = variable
                        .init()
                        .map(|expression| self.lower_expression(expression))
                        .unwrap_or_else(TypedExpr::undefined);
                    if static_to_string_regexp_object {
                        self.static_to_string_regexp_object_bindings
                            .insert(name.clone());
                    } else {
                        self.static_to_string_regexp_object_bindings.remove(&name);
                    }

                    // 14.3.1.2 steps 4-5. The initializer above is already
                    // lowered, which is what makes `LoweredInitializer` cheap
                    // here and what makes the reverse order unwritable.
                    let init = LoweredInitializer::evaluated(init);
                    let initialized = match pending {
                        Some(pending) => pending.initialize(init),
                        None => {
                            // Not a name this statement list created: a for-head
                            // binding, or a declarator form the sweep could not
                            // resolve. Allocate as before.
                            let storage_name =
                                self.direct_lexical_storage_name(&name, identifier.span());
                            InitializedBinding::without_creation(
                                name.clone(),
                                mode,
                                storage_name,
                                init.into_expr(),
                            )
                        }
                    };
                    let statement = initialized.declare(self);
                    let binding = self.lookup_binding(&name).unwrap_or_else(|| {
                        panic!(
                            "lexical binding `{name}` must be declared before installing static string facts"
                        )
                    });
                    if let Some(candidate) = source_candidate {
                        self.set_finite_source_binding_candidates(&name, candidate);
                    }
                    if let Some(value) = static_string_value {
                        self.static_string_bindings.insert(&binding, value);
                    } else {
                        self.static_string_bindings.remove(&binding);
                    }
                    statements.push(statement);
                }
                Binding::Pattern(pattern) => {
                    let Some(init) = variable.init() else {
                        self.unsupported("destructuring binding without initializer");
                        return (StatementIr::Empty, ValueKind::Undefined);
                    };
                    if self.async_generator_entry_state().is_some()
                        && (contains(pattern, ContainsSymbol::AwaitExpression)
                            || contains(pattern, ContainsSymbol::YieldExpression))
                    {
                        let Some(prefix) = self
                            .lower_async_generator_pattern_initializer(pattern, init, mode, None)
                        else {
                            self.unsupported(
                                "mixed async-generator binding pattern continuation ownership",
                            );
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(prefix);
                        continue;
                    }
                    if self.async_generator_entry_state().is_some()
                        && (contains(init, ContainsSymbol::AwaitExpression)
                            || contains(init, ContainsSymbol::YieldExpression))
                    {
                        if contains(pattern, ContainsSymbol::AwaitExpression)
                            || contains(pattern, ContainsSymbol::YieldExpression)
                        {
                            self.unsupported("mixed async-generator pattern-owned suspension");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        }
                        let Some((prefix, value)) = self.lower_mixed_generator_value(init) else {
                            self.unsupported("mixed async-generator pattern initializer");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(prefix);
                        let Some(bindings) =
                            self.lower_pattern_lexical_binding_from_value(mode, pattern, value)
                        else {
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(bindings);
                        continue;
                    }
                    if self.plain_async_entry_state().is_some()
                        && contains(pattern, ContainsSymbol::AwaitExpression)
                    {
                        let Some(bindings) =
                            self.lower_async_pattern_lexical_initializer(mode, pattern, init)
                        else {
                            self.unsupported("async binding pattern continuation ownership");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(bindings);
                        continue;
                    }
                    if matches!(
                        self.generator_value_branch_admission(),
                        GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                    ) && (contains(init, ContainsSymbol::YieldExpression)
                        || contains(pattern, ContainsSymbol::YieldExpression))
                    {
                        let Some(source) = GeneratorPatternInitializerSource::new(
                            variable,
                            self.generator_value_branch_admission(),
                        ) else {
                            self.unsupported("generator binding pattern suspension requires its own continuation");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        let Some(bindings) =
                            self.lower_generator_lexical_pattern_initializer(mode, source)
                        else {
                            self.unsupported("generator pattern initializer suspension expression");
                            return (StatementIr::Empty, ValueKind::Undefined);
                        };
                        statements.extend(bindings);
                        continue;
                    }
                    let Some(mut bindings) =
                        self.lower_pattern_lexical_binding(mode, pattern, init)
                    else {
                        return (StatementIr::Empty, ValueKind::Undefined);
                    };
                    statements.append(&mut bindings);
                }
            }
        }

        if statements.len() == 1 {
            (statements.remove(0), ValueKind::Undefined)
        } else {
            (StatementIr::LexicalBlock(statements), ValueKind::Undefined)
        }
    }
}
