use super::*;
use crate::function_layout::ModuleEntryRole;
use crate::runtime_artifact::{EmittedModule, ModuleKind, RuntimeLayout};
use crate::runtime_helpers::ProgramHook;
mod intl;
use scriptw_intl::IntlDataSelection;
use wasm_encoder::{
    ConstExpr, DataSection, ExportKind, ExportSection, GlobalType, ImportSection, MemorySection,
    MemoryType, Module,
};

mod array_iterator_helpers;
mod import_indices;
mod metadata;
use import_indices::claim_optional_host_import_function_index;
mod native_program;
mod program_entries;
mod program_helper_bodies;
use native_program::NativeProgramFactory;
pub(crate) use native_program::{
    emit_runtime_direct_eval_module, emit_runtime_global_script_module,
};
pub(crate) use program_entries::emit_runtime_function_module;
pub(super) use program_entries::emit_script_module;
use program_entries::ProgramExports;

fn emit_script_module_with_factory(
    script: &ScriptIr,
    kind: ModuleKind<'_>,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_selection: &IntlDataSelection,
    factory: Option<NativeProgramFactory<'_>>,
    validated_runtime: Option<&crate::ValidatedRuntimeArtifact>,
) -> Result<EmittedModule, EmitError> {
    let uses_heap = kind.uses_heap();
    let compile_runtime = kind.compiles_runtime();
    let compile_program = kind.compiles_program();
    let compile_program_helpers = compile_program && uses_heap;
    // Names the script's host surface resolves to. Compiling every host
    // builtin's body must not widen what a bare identifier can resolve to, so
    // this stays the script's own set (plus the bodies it materializes).
    let mut host_surface = script.host_builtins.clone();
    if host_surface.contains(&HostBuiltinId::CreateHTMLDDA)
        && !host_surface.contains(&HostBuiltinId::HTMLDDA)
    {
        host_surface.push(HostBuiltinId::HTMLDDA);
    }
    if host_surface.contains(&HostBuiltinId::CreateRealm)
        && !host_surface.contains(&HostBuiltinId::RealmEvalScript)
    {
        host_surface.push(HostBuiltinId::RealmEvalScript);
    }
    // `createRealm` materializes the new realm's `$262.detachArrayBuffer` from
    // the shared host builtin; root it alongside the realm so the builtin
    // never observes a missing meta.
    if host_surface.contains(&HostBuiltinId::CreateRealm)
        && !host_surface.contains(&HostBuiltinId::DetachArrayBuffer)
    {
        host_surface.push(HostBuiltinId::DetachArrayBuffer);
    }
    let compiled_host_builtins = if uses_heap {
        HostBuiltinId::ALL.to_vec()
    } else {
        host_surface.clone()
    };
    // The main job checkpoint reports unhandled Promise rejections through the
    // same line-oriented host ABI as `print`. Its import therefore belongs to
    // every heap-backed module even when user source never names `print`.
    let uses_host_print = uses_heap || compiled_host_builtins.contains(&HostBuiltinId::Print);
    let uses_agent_host = compiled_host_builtins.iter().any(|builtin| {
        matches!(
            builtin,
            HostBuiltinId::AgentStart
                | HostBuiltinId::AgentBroadcast
                | HostBuiltinId::AgentReceiveBroadcast
                | HostBuiltinId::AgentReport
                | HostBuiltinId::AgentGetReport
                | HostBuiltinId::AgentSleep
                | HostBuiltinId::AgentMonotonicNow
                | HostBuiltinId::AgentLeaving
        )
    });
    let compiled_standard_builtins = if uses_heap {
        StandardBuiltinId::all_functions().to_vec()
    } else {
        Vec::new()
    };
    let uses_shared_memory = uses_agent_host
        || script_references_memory_atomics(script)
        || compiled_standard_builtins.iter().copied().any(|builtin| {
            standard_builtin_uses_memory_atomics(builtin)
                || standard_builtin_uses_shared_buffer_resource(builtin)
        });
    let uses_atomics_wait = compiled_standard_builtins.contains(&StandardBuiltinId::AtomicsWait);
    let uses_atomics_wait_async =
        compiled_standard_builtins.contains(&StandardBuiltinId::AtomicsWaitAsync);
    let uses_agent_call = uses_agent_host || uses_atomics_wait_async;
    // Carrier bodies and these Duration bodies can inline calendar helpers in
    // the initial compilation pass. Namespace bootstrap roots are materialized
    // only in the later dependency fixpoint, so Duration cannot wait for a
    // carrier body to make its immutable calendar image available.
    let uses_temporal_calendar = compiled_standard_builtins.iter().any(|builtin| {
        let name = builtin.debug_name();
        matches!(
            builtin,
            StandardBuiltinId::TemporalDurationCompare
                | StandardBuiltinId::TemporalDurationPrototypeAdd
                | StandardBuiltinId::TemporalDurationPrototypeSubtract
                | StandardBuiltinId::TemporalDurationPrototypeRound
                | StandardBuiltinId::TemporalDurationPrototypeTotal
        ) || name.contains("Temporal.PlainDate")
            || name.contains("Temporal.PlainYearMonth")
            || name.contains("Temporal.PlainMonthDay")
            || name.contains("Temporal.ZonedDateTime")
    });
    let uses_wall_clock_millis = compiled_standard_builtins
        .iter()
        .any(|builtin| builtin.requires_wall_clock());
    // The calendar gate emits the complete ZonedDateTime converter, including
    // named-zone provider calls, before public-builtin discovery is complete.
    // Its imports and literals must follow that same physical helper ownership.
    let uses_intl_host = uses_temporal_calendar
        || compiled_standard_builtins
            .iter()
            .any(|builtin| builtin.requires_intl_host());
    let uses_system_time_zone = compiled_standard_builtins
        .iter()
        .any(|builtin| builtin.requires_system_time_zone());
    let uses_random_f64 = compiled_standard_builtins
        .iter()
        .any(|builtin| builtin.requires_random());
    let uses_number_pow_import = script.has_scalar_exponentiation()
        || compiled_standard_builtins.contains(&StandardBuiltinId::MathPow);
    // Each Math transcendental is gated on its own compiled builtin body, the
    // same compiled-builtin authority as `uses_random_f64` but per function:
    // a script that never reaches `Math.log` must not drag `math_log` into
    // its import surface. `Math.pow` shares the separately selected
    // `number_pow` import with source Number exponentiation.
    let uses_math_acos = compiled_standard_builtins.contains(&StandardBuiltinId::MathAcos);
    let uses_math_acosh = compiled_standard_builtins.contains(&StandardBuiltinId::MathAcosh);
    let uses_math_asin = compiled_standard_builtins.contains(&StandardBuiltinId::MathAsin);
    let uses_math_asinh = compiled_standard_builtins.contains(&StandardBuiltinId::MathAsinh);
    let uses_math_atan = compiled_standard_builtins.contains(&StandardBuiltinId::MathAtan);
    let uses_math_atanh = compiled_standard_builtins.contains(&StandardBuiltinId::MathAtanh);
    let uses_math_cbrt = compiled_standard_builtins.contains(&StandardBuiltinId::MathCbrt);
    let uses_math_cos = compiled_standard_builtins.contains(&StandardBuiltinId::MathCos);
    let uses_math_cosh = compiled_standard_builtins.contains(&StandardBuiltinId::MathCosh);
    let uses_math_exp = compiled_standard_builtins.contains(&StandardBuiltinId::MathExp);
    let uses_math_expm1 = compiled_standard_builtins.contains(&StandardBuiltinId::MathExpm1);
    let uses_math_log = compiled_standard_builtins.contains(&StandardBuiltinId::MathLog);
    let uses_math_log10 = compiled_standard_builtins.contains(&StandardBuiltinId::MathLog10);
    let uses_math_log1p = compiled_standard_builtins.contains(&StandardBuiltinId::MathLog1p);
    let uses_math_log2 = compiled_standard_builtins.contains(&StandardBuiltinId::MathLog2);
    let uses_math_sin = compiled_standard_builtins.contains(&StandardBuiltinId::MathSin);
    let uses_math_sinh = compiled_standard_builtins.contains(&StandardBuiltinId::MathSinh);
    let uses_math_tan = compiled_standard_builtins.contains(&StandardBuiltinId::MathTan);
    let uses_math_tanh = compiled_standard_builtins.contains(&StandardBuiltinId::MathTanh);
    let uses_math_atan2 = compiled_standard_builtins.contains(&StandardBuiltinId::MathAtan2);
    let number_pow_import_function_index =
        uses_number_pow_import.then_some(1 + u32::from(uses_host_print));
    let wall_clock_millis_import_function_index = uses_wall_clock_millis
        .then_some(1 + u32::from(uses_host_print) + u32::from(uses_number_pow_import));
    let monotonic_clock_nanos_import_function_index = uses_atomics_wait_async.then_some(
        1 + u32::from(uses_host_print)
            + u32::from(uses_number_pow_import)
            + u32::from(uses_wall_clock_millis),
    );
    let sleep_nanos_import_function_index =
        monotonic_clock_nanos_import_function_index.map(|index| index + 1);
    let agent_call_import_function_index = uses_agent_call.then_some(
        1 + u32::from(uses_host_print)
            + u32::from(uses_number_pow_import)
            + u32::from(uses_wall_clock_millis)
            + 2 * u32::from(uses_atomics_wait_async),
    );
    // Provider calls are declared in the typed GC host profile below. Numeric
    // scalar imports carry no JavaScript reference or private byte-array handle.
    let random_f64_import_function_index = uses_random_f64.then_some(
        1 + u32::from(uses_host_print)
            + u32::from(uses_number_pow_import)
            + u32::from(uses_wall_clock_millis)
            + 2 * u32::from(uses_atomics_wait_async)
            + u32::from(uses_agent_call),
    );
    // The Math block is appended after every existing optional host
    // function, preserving each of their indices, and the required rejection
    // import closes the function-import run after it. Twenty cumulative
    // `1 + u32::from(...) + ...` chains would restate the same prefix twenty
    // times; a claimed running index keeps the dense allocation reviewable in
    // one place. The `let` order below is the index order.
    let mut next_math_import_function_index = 1
        + u32::from(uses_host_print)
        + u32::from(uses_number_pow_import)
        + u32::from(uses_wall_clock_millis)
        + 2 * u32::from(uses_atomics_wait_async)
        + u32::from(uses_agent_call)
        + u32::from(uses_random_f64);
    let math_acos_import_function_index = uses_math_acos
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_acosh_import_function_index = uses_math_acosh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_asin_import_function_index = uses_math_asin
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_asinh_import_function_index = uses_math_asinh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_atan_import_function_index = uses_math_atan
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_atanh_import_function_index = uses_math_atanh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_cbrt_import_function_index = uses_math_cbrt
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_cos_import_function_index = uses_math_cos
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_cosh_import_function_index = uses_math_cosh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_exp_import_function_index = uses_math_exp
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_expm1_import_function_index = uses_math_expm1
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_log_import_function_index = uses_math_log
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_log10_import_function_index = uses_math_log10
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_log1p_import_function_index = uses_math_log1p
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_log2_import_function_index = uses_math_log2
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_sin_import_function_index = uses_math_sin
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_sinh_import_function_index = uses_math_sinh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_tan_import_function_index = uses_math_tan
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_tanh_import_function_index = uses_math_tanh
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let math_atan2_import_function_index = uses_math_atan2
        .then(|| claim_optional_host_import_function_index(&mut next_math_import_function_index));
    let reject_runtime_semantics_import_function_index = next_math_import_function_index;
    let mut imported_function_count = reject_runtime_semantics_import_function_index + 1;
    let gc_host_imports = crate::gc_types::GcHostImports::plan(
        &mut imported_function_count,
        uses_shared_memory,
        uses_agent_host,
        uses_atomics_wait_async,
        uses_atomics_wait,
        uses_intl_host,
        uses_system_time_zone,
        uses_heap,
        kind.source_compilation_profile(),
    );
    let uses_json_stringify =
        compiled_standard_builtins.contains(&StandardBuiltinId::JsonStringify);
    let runtime_bootstrap_plan = if uses_heap {
        RuntimeBootstrapPlan::full()
    } else {
        RuntimeBootstrapPlan::default()
    };
    let host_import_function_indices = HostImportFunctionIndices::new(
        number_pow_import_function_index.map(NumberPowImportFunctionIndex::new),
        wall_clock_millis_import_function_index.map(WallClockMillisImportFunctionIndex::new),
        monotonic_clock_nanos_import_function_index
            .map(MonotonicClockNanosImportFunctionIndex::new),
        sleep_nanos_import_function_index.map(SleepNanosImportFunctionIndex::new),
        agent_call_import_function_index.map(AgentCallImportFunctionIndex::new),
        random_f64_import_function_index.map(RandomF64ImportFunctionIndex::new),
        math_acos_import_function_index.map(MathAcosImportFunctionIndex::new),
        math_acosh_import_function_index.map(MathAcoshImportFunctionIndex::new),
        math_asin_import_function_index.map(MathAsinImportFunctionIndex::new),
        math_asinh_import_function_index.map(MathAsinhImportFunctionIndex::new),
        math_atan_import_function_index.map(MathAtanImportFunctionIndex::new),
        math_atanh_import_function_index.map(MathAtanhImportFunctionIndex::new),
        math_cbrt_import_function_index.map(MathCbrtImportFunctionIndex::new),
        math_cos_import_function_index.map(MathCosImportFunctionIndex::new),
        math_cosh_import_function_index.map(MathCoshImportFunctionIndex::new),
        math_exp_import_function_index.map(MathExpImportFunctionIndex::new),
        math_expm1_import_function_index.map(MathExpm1ImportFunctionIndex::new),
        math_log_import_function_index.map(MathLogImportFunctionIndex::new),
        math_log10_import_function_index.map(MathLog10ImportFunctionIndex::new),
        math_log1p_import_function_index.map(MathLog1pImportFunctionIndex::new),
        math_log2_import_function_index.map(MathLog2ImportFunctionIndex::new),
        math_sin_import_function_index.map(MathSinImportFunctionIndex::new),
        math_sinh_import_function_index.map(MathSinhImportFunctionIndex::new),
        math_tan_import_function_index.map(MathTanImportFunctionIndex::new),
        math_tanh_import_function_index.map(MathTanhImportFunctionIndex::new),
        math_atan2_import_function_index.map(MathAtan2ImportFunctionIndex::new),
        RejectRuntimeSemanticsImportFunctionIndex::new(
            reject_runtime_semantics_import_function_index,
        ),
    )
    .with_gc_imports(gc_host_imports);
    let source = SourceFunctions::partition(script);
    let helper_emission =
        RuntimeHelperEmission::NONE.with(RuntimeHelperFact::UsesJsonStringify, uses_json_stringify);
    let helpers_of = |program_owned: bool| {
        if uses_heap {
            RuntimeHelperId::ALL
                .iter()
                .filter(|helper| {
                    helper.is_program_owned() == program_owned && helper.is_emitted(helper_emission)
                })
                .count()
        } else {
            0
        }
    };
    let runtime_helper_count = helpers_of(false);
    let program_helper_count = helpers_of(true);
    let layout = FunctionIndexLayout::new(
        imported_function_count,
        source.runtime_owned.len(),
        compiled_standard_builtins.len(),
        compiled_host_builtins.len(),
        runtime_helper_count,
        source.program.len() + script.prepared_script_units().count(),
        program_helper_count,
        kind,
        if factory.is_some() {
            ModuleEntryRole::NativeFactory
        } else {
            ModuleEntryRole::for_module(kind)
        },
    )
    .with_foreign_imports(script.foreign_modules.functions().count(), kind)?;
    let (module_types, foreign_types) = ModuleTypeRegistry::new(kind)
        .with_global_prelude(compile_program && script.module_prelude.is_some())
        .with_foreign_modules(&script.foreign_modules)?;
    let foreign_imports =
        crate::foreign::ForeignImportPlan::new(&script.foreign_modules, foreign_types, &layout)?;
    let function_metas = FunctionMetaRegistry::new(
        build_function_metas(
            &source,
            script.prepared_script_units(),
            &compiled_standard_builtins,
            &compiled_host_builtins,
            &layout,
        )?,
        host_surface.iter().copied().collect(),
        host_import_function_indices,
        script.prepared_dynamic_functions.clone(),
        script.prepared_scripts.clone(),
        crate::planning::ModuleGraphPlan::from_script(script)?,
    )
    .with_foreign_imports(foreign_imports);
    let entry_exports = ProgramExports::new(kind, &layout, factory, &function_metas)?;
    let string_pool = match (kind, validated_runtime) {
        (ModuleKind::Program(runtime), Some(validated))
            if std::sync::Arc::ptr_eq(runtime, validated.runtime()) =>
        {
            StringPool::collect_with_validated_runtime(
                script,
                function_metas.metas(),
                &compiled_standard_builtins,
                uses_temporal_calendar,
                validated,
            )?
        }
        (_, None) => StringPool::collect(
            script,
            function_metas.metas(),
            &compiled_standard_builtins,
            uses_temporal_calendar,
            intl_selection,
        )?,
        _ => {
            return Err(EmitError::unsupported(
                "native pool proof must retain its exact linked runtime",
            ));
        }
    };
    match kind {
        ModuleKind::Standalone => {}
        ModuleKind::Runtime(_) => string_pool.require_runtime_only()?,
        ModuleKind::Program(runtime) => runtime.layout().require_pool(&string_pool)?,
    }
    let provided_hooks = ProvidedHooks::of(&function_metas, &string_pool);
    let module_guard_count = module_unit_guard_count(script);

    // Only numeric byte-scratch state and module status are scalar globals
    // ahead of the roots. All semantic roots are declared by RuntimeSchema,
    // and the program-sized once guards follow them so no root index depends
    // on the program.
    let mut globals = ModuleGlobalSectionBuilder::new();
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        // Linked modules share R's transient scratch cursor. P-owned immutable
        // metadata is published in GC storage and never moves this cursor.
        &ConstExpr::i64_const(align_heap_start(if uses_heap {
            string_pool.compiler_owned_boundary().static_len()
        } else {
            string_pool.bytes.len()
        }) as i64),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(0),
    );
    let module_sections = module_types.finalize_globals(globals, uses_heap, module_guard_count);

    let runtime_helper_base = uses_heap.then(|| layout.helper_base());

    let first_defined_function = if compile_runtime {
        layout.defined_functions().start
    } else {
        layout.first_program_index()
    };
    let mut module_package = module_sections.begin(first_defined_function, layout.entry_role());
    if let Some(index) = layout.program_initialize_index() {
        let image =
            ProgramInstanceImage::from_program_pool(&string_pool, kind, script.language_features)?;
        module_package.compile_program_initialize(ProgramInitializeCompilation::new(
            &image,
            &string_pool,
            &function_metas,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base.expect("linked P has registered R helpers"),
            index,
        )?)?;
    }
    if layout.main_index().is_some() {
        module_package.compile_main(MainFunctionCompilation::new(
            script,
            promise_rejection_policy,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        ))?;
    }

    // Runtime callables first (compiler-owned source bodies, standard builtins,
    // host builtins), then the helpers, then main and the program's callables:
    // the push order is the planned index order.
    let mut compiled_functions: Vec<EmittedFunction> = Vec::new();
    let mut program_functions: Vec<EmittedFunction> = Vec::new();
    for (compiled_here, functions, destination) in [
        (
            compile_runtime,
            &source.runtime_owned,
            &mut compiled_functions,
        ),
        (compile_program, &source.program, &mut program_functions),
    ] {
        if !compiled_here {
            continue;
        }
        for function in functions {
            let mut builder = FunctionBuilder::new_function(
                module_package.schema(),
                function,
                script
                    .prepared_script_units()
                    .find(|unit| unit.function_ids.contains(&function.id))
                    .map_or(&script.global_bindings, |unit| &unit.global_bindings),
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            )?;
            destination.push(builder.compile_callable()?);
        }
    }
    for unit in script.prepared_script_units().filter(|_| compile_program) {
        let mut builder = FunctionBuilder::new_prepared_script(
            module_package.schema(),
            unit,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        program_functions.push(builder.compile_callable()?);
    }
    for builtin in compiled_standard_builtins
        .iter()
        .filter(|_| compile_runtime)
    {
        let mut builder = FunctionBuilder::new_standard_builtin(
            module_package.schema(),
            *builtin,
            &function_metas
                .get(&builtin.function_id())
                .expect("standard builtin has its planned entry")
                .entry,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        compiled_functions.push(builder.compile_builtin_callable()?);
    }
    for builtin in compiled_host_builtins.iter().filter(|_| compile_runtime) {
        let mut builder = FunctionBuilder::new_host_builtin(
            module_package.schema(),
            *builtin,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        compiled_functions.push(builder.compile_builtin_callable()?);
    }

    // Shared object-read / object-write runtime helpers. These carry the large
    // property-access state machines that would otherwise be inlined at every
    // read/write site, blowing single functions past Cranelift's per-function
    // code-size limit. They are emitted once, directly after the heap helpers,
    // and are reached with plain `call`s (never through the funcref table).
    let object_read_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectRead,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_read_helper()
        })
        .transpose()?;
    let object_write_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectWrite,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_write_helper()
        })
        .transpose()?;
    let object_define_data_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectDefineData,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_define_data_helper()
        })
        .transpose()?;
    let proxy_call_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ProxyCall,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_proxy_call_helper()
        })
        .transpose()?;
    let proxy_construct_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ProxyConstruct,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_proxy_construct_helper()
        })
        .transpose()?;
    let string_equality_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::StringEquality,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_string_equality_helper()
        })
        .transpose()?;
    let number_to_string_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::NumberToString,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_number_to_string_helper()
        })
        .transpose()?;
    let string_to_number_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::StringToNumber,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_string_to_number_helper()
        })
        .transpose()?;
    let value_to_string_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ValueToString,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_value_to_string_helper()
        })
        .transpose()?;
    let value_to_number_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ValueToNumber,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_value_to_number_helper()
        })
        .transpose()?;
    let value_to_numeric_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ValueToNumeric,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_value_to_numeric_helper()
        })
        .transpose()?;
    let object_get_prototype_of_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectGetPrototypeOf,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_get_prototype_of_helper()
        })
        .transpose()?;
    let object_set_prototype_of_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectSetPrototypeOf,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_set_prototype_of_helper()
        })
        .transpose()?;
    let object_delete_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectDelete,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_delete_helper()
        })
        .transpose()?;
    let object_is_extensible_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectIsExtensible,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_is_extensible_helper()
        })
        .transpose()?;
    let object_prevent_extensions_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectPreventExtensions,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_prevent_extensions_helper()
        })
        .transpose()?;
    let object_read_proxy_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectReadProxy,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_read_proxy_helper()
        })
        .transpose()?;
    let regexp_matcher_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::RegExpMatcher,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_regexp_matcher_helper()
        })
        .transpose()?;
    let regexp_compiler_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::RegExpCompiler,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_regexp_compiler_helper()
        })
        .transpose()?;
    let function_call_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::FunctionCall,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_function_call_helper()
        })
        .transpose()?;
    let dynamic_property_read_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::DynamicPropertyRead,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_dynamic_property_read_helper()
        })
        .transpose()?;
    let ordinary_set_data_on_receiver_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::OrdinarySetDataOnReceiver,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_ordinary_set_data_on_receiver_helper()
        })
        .transpose()?;

    let ordinary_set_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::OrdinarySet,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_ordinary_set_helper()
        })
        .transpose()?;
    let decimal_to_binary64_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::DecimalToBinary64,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_decimal_to_binary64_helper()
        })
        .transpose()?;
    let bigint_arithmetic_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::BigIntArithmetic,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_bigint_arithmetic_helper()
        })
        .transpose()?;
    let json_stringify_value_helper_function = (compile_runtime && uses_json_stringify)
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::JsonStringifyValue,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_json_stringify_value_helper()
        })
        .transpose()?;
    let object_has_property_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ObjectHasProperty,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_object_has_property_helper()
        })
        .transpose()?;
    let runtime_error_object_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::RuntimeErrorObject,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_runtime_error_object_helper()
        })
        .transpose()?;
    let with_environment_has_binding_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::WithEnvironmentHasBinding,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_with_environment_has_binding_helper()
        })
        .transpose()?;
    let indexed_element_read_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::IndexedElementRead,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_indexed_element_read_helper()
        })
        .transpose()?;
    let indexed_element_write_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::IndexedElementWrite,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_indexed_element_write_helper()
        })
        .transpose()?;
    // One ToPrimitive body per hint, built from the closed `ToPrimitiveHint`
    // domain rather than from three copied blocks, so a fourth hint is a
    // compile error in `RuntimeHelperId::helper_for` and not a missing body
    // here. `BTreeMap` keys them by helper id, so the order of this loop does
    // not decide the order they are written in — `RuntimeHelperId::ALL` does.
    let mut value_to_primitive_helper_functions: BTreeMap<RuntimeHelperId, Function> =
        BTreeMap::new();
    if compile_runtime {
        for hint in [
            ToPrimitiveHint::Default,
            ToPrimitiveHint::Number,
            ToPrimitiveHint::String,
        ] {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::helper_for(hint),
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            value_to_primitive_helper_functions.insert(
                RuntimeHelperId::helper_for(hint),
                builder.compile_value_to_primitive_helper(hint)?,
            );
        }
    }
    let value_to_property_key_helper_function = compile_runtime
        .then(|| {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                module_package.schema(),
                RuntimeHelperId::ValueToPropertyKey,
                &string_pool,
                &function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_value_to_property_key_helper()
        })
        .transpose()?;
    // Calendar arithmetic, identifier parsers and complete carrier conversions
    // retain stable typed declarations during discovery, before the immutable
    // calendar image is planned. Fallible compiler calls propagate with `; ?`.
    let mut temporal_helper_functions = BTreeMap::new();
    if compile_runtime {
        macro_rules! register_temporal_helper {
            ($helper:expr, $compile:ident $(, $argument:expr)* $(; $propagate:tt)?) => {{
                let helper = $helper;
                let mut builder = FunctionBuilder::new_runtime_operation_helper(
                    module_package.schema(), helper, &string_pool, &function_metas,
                    uses_heap, runtime_bootstrap_plan.clone(), runtime_helper_base,
                );
                temporal_helper_functions.insert(
                    helper, builder.$compile($($argument,)* uses_temporal_calendar) $($propagate)?,
                );
            }};
        }
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarIsoDateProbe,
            compile_temporal_calendar_iso_date_probe_helper; ?
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarIdentifier,
            compile_temporal_calendar_identifier_helper; ?
        );
        for calendar in [
            crate::data::TemporalEastAsianCalendar::Chinese,
            crate::data::TemporalEastAsianCalendar::Dangi,
        ] {
            register_temporal_helper!(
                RuntimeHelperId::for_east_asian_year(calendar),
                compile_temporal_east_asian_year_helper,
                calendar
            );
        }
        register_temporal_helper!(
            RuntimeHelperId::TemporalUmmAlQuraYear,
            compile_temporal_umalqura_year_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalUmmAlQuraEpoch,
            compile_temporal_umalqura_epoch_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarProjectDate,
            compile_temporal_calendar_project_date_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarFieldsToIso,
            compile_temporal_calendar_fields_to_iso_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarDaysInMonth,
            compile_temporal_calendar_days_in_month_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarBalanceYearMonth,
            compile_temporal_calendar_balance_year_month_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalCalendarDifferenceDate,
            compile_temporal_calendar_difference_date_helper
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalZonedDateTimeConvert,
            compile_temporal_zoned_date_time_convert_helper; ?
        );
        register_temporal_helper!(
            RuntimeHelperId::TemporalPlainDateConvert,
            compile_temporal_plain_date_convert_helper; ?
        );
    }

    let mut array_indexed_helper_functions = BTreeMap::new();
    if compile_runtime {
        let mut default_publisher = FunctionBuilder::new_runtime_operation_helper(
            module_package.schema(),
            RuntimeHelperId::ArrayIndexedPublishDefaultData,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        array_indexed_helper_functions.insert(
            RuntimeHelperId::ArrayIndexedPublishDefaultData,
            default_publisher.compile_array_indexed_publish_default_data_helper()?,
        );
        let mut publisher = FunctionBuilder::new_runtime_operation_helper(
            module_package.schema(),
            RuntimeHelperId::ArrayIndexedPublish,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        array_indexed_helper_functions.insert(
            RuntimeHelperId::ArrayIndexedPublish,
            publisher.compile_array_indexed_publish_helper()?,
        );
        let mut deleter = FunctionBuilder::new_runtime_operation_helper(
            module_package.schema(),
            RuntimeHelperId::ArrayIndexedDelete,
            &string_pool,
            &function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        array_indexed_helper_functions.insert(
            RuntimeHelperId::ArrayIndexedDelete,
            deleter.compile_array_indexed_delete_helper()?,
        );
    }

    // Compiled helper bodies, keyed by identity rather than by position. The
    // code section below is generated by walking `RuntimeHelperId::ALL` and
    // draining this map, so emission order comes from the enum's declaration
    // order alone and a helper whose body was compiled but never emitted (or
    // vice versa) is a panic here instead of a silently shifted function index
    // at every call site.
    let mut helper_bodies: BTreeMap<RuntimeHelperId, Function> = BTreeMap::new();
    if compile_runtime {
        for (helper, body) in array_indexed_helper_functions {
            helper_bodies.insert(helper, body);
        }
        helper_bodies.insert(
            RuntimeHelperId::TransientByteAlloc,
            emit_transient_byte_alloc_helper_function(),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectRead,
            object_read_helper_function
                .expect("object-read helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectWrite,
            object_write_helper_function
                .expect("object-write helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectDefineData,
            object_define_data_helper_function
                .expect("object-define-data helper must exist when heap is enabled"),
        );
        helper_bodies.extend(array_iterator_helpers::compile(
            module_package.schema(),
            &string_pool,
            &function_metas,
            uses_heap,
            &runtime_bootstrap_plan,
            runtime_helper_base,
        )?);
        helper_bodies.insert(
            RuntimeHelperId::ProxyCall,
            proxy_call_helper_function.expect("proxy-call helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ProxyConstruct,
            proxy_construct_helper_function
                .expect("proxy-construct helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::StringEquality,
            string_equality_helper_function
                .expect("string-equality helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::NumberToString,
            number_to_string_helper_function
                .expect("number-to-string helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::StringToNumber,
            string_to_number_helper_function
                .expect("string-to-number helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ValueToString,
            value_to_string_helper_function
                .expect("value-to-string helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ValueToNumber,
            value_to_number_helper_function
                .expect("value-to-number helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ValueToNumeric,
            value_to_numeric_helper_function
                .expect("value-to-numeric helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectGetPrototypeOf,
            object_get_prototype_of_helper_function
                .expect("get-prototype-of helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectSetPrototypeOf,
            object_set_prototype_of_helper_function
                .expect("set-prototype-of helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectDelete,
            object_delete_helper_function.expect("delete helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectIsExtensible,
            object_is_extensible_helper_function
                .expect("is-extensible helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectPreventExtensions,
            object_prevent_extensions_helper_function
                .expect("prevent-extensions helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::ObjectReadProxy,
            object_read_proxy_helper_function
                .expect("object-read-proxy helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::RegExpMatcher,
            regexp_matcher_helper_function
                .expect("regexp matcher helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::RegExpCompiler,
            regexp_compiler_helper_function
                .expect("regexp compiler helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::FunctionCall,
            function_call_helper_function
                .expect("function-call helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::DynamicPropertyRead,
            dynamic_property_read_helper_function
                .expect("dynamic property-read helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::OrdinarySetDataOnReceiver,
            ordinary_set_data_on_receiver_helper_function
                .expect("ordinary receiver-set helper must exist when heap is enabled"),
        );

        helper_bodies.insert(
            RuntimeHelperId::OrdinarySet,
            ordinary_set_helper_function
                .expect("ordinary-set helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::DecimalToBinary64,
            decimal_to_binary64_helper_function
                .expect("decimal converter helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::BigIntArithmetic,
            bigint_arithmetic_helper_function
                .expect("BigInt arithmetic helper must exist when heap is enabled"),
        );
        helper_bodies.extend(temporal_helper_functions);
        helper_bodies.insert(
            RuntimeHelperId::ObjectHasProperty,
            object_has_property_helper_function
                .expect("object has-property helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::WithEnvironmentHasBinding,
            with_environment_has_binding_helper_function
                .expect("with HasBinding helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::RuntimeErrorObject,
            runtime_error_object_helper_function
                .expect("runtime error-object helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::IndexedElementRead,
            indexed_element_read_helper_function
                .expect("indexed element-read helper must exist when heap is enabled"),
        );
        helper_bodies.insert(
            RuntimeHelperId::IndexedElementWrite,
            indexed_element_write_helper_function
                .expect("indexed element-write helper must exist when heap is enabled"),
        );
        for hint in [
            ToPrimitiveHint::Default,
            ToPrimitiveHint::Number,
            ToPrimitiveHint::String,
        ] {
            let helper = RuntimeHelperId::helper_for(hint);
            helper_bodies.insert(
                helper,
                value_to_primitive_helper_functions
                    .remove(&helper)
                    .expect("every ToPrimitive hint helper must exist when heap is enabled"),
            );
        }
        helper_bodies.insert(
            RuntimeHelperId::ValueToPropertyKey,
            value_to_property_key_helper_function
                .expect("to-property-key helper must exist when heap is enabled"),
        );
        // Each runtime algorithm has one typed body. Calls from bootstrap,
        // builtins and source functions share these declarations.
        macro_rules! register_runtime_helper {
            ($($id:ident, $compile:ident);+ $(;)?) => { $({
                let mut builder = FunctionBuilder::new_runtime_operation_helper(
                    module_package.schema(),
                    RuntimeHelperId::$id,
                    &string_pool,
                    &function_metas,
                    uses_heap,
                    runtime_bootstrap_plan.clone(),
                    runtime_helper_base,
                );
                helper_bodies.insert(RuntimeHelperId::$id, builder.$compile()?);
            })+ };
        }
        register_runtime_helper!(
            GlobalLexicalDeclarationCheck,
            compile_global_lexical_declaration_check_helper
        );
        register_runtime_helper!(
            GlobalVarDeclarationCheck,
            compile_global_var_declaration_check_helper
        );
        register_runtime_helper!(
            GlobalVarDeclarationInstall,
            compile_global_var_declaration_install_helper
        );
        register_runtime_helper!(
            OrdinaryPropertyStorageEnsure, compile_ordinary_property_storage_ensure_helper;
            OrdinaryOwnDescriptor, compile_ordinary_own_descriptor_helper;
            CompactLiteralAllocate, compile_compact_literal_allocate_helper;
            NativeFunctionAllocate, compile_native_function_allocate_helper;
            IntrinsicMethodPublish, compile_intrinsic_method_publish_helper;
            PropertyValueResolve, compile_property_value_resolve_helper;
            ArgumentListAppend, compile_argument_list_append_helper;
            CachedPropertyRead, compile_cached_property_read_helper;
        );
        register_runtime_helper!(OrdinaryPropertyFind, compile_ordinary_property_find_helper);
        register_runtime_helper!(
            OrdinaryPropertyAppend,
            compile_ordinary_property_append_helper
        );
        register_runtime_helper!(
            OrdinaryPropertyPrepare,
            compile_ordinary_property_prepare_helper
        );
        register_runtime_helper!(CollectionPrepareMap, compile_collection_prepare_map_helper);
        register_runtime_helper!(CollectionPrepareSet, compile_collection_prepare_set_helper);
        register_runtime_helper!(CollectionNextMap, compile_collection_next_map_helper);
        register_runtime_helper!(CollectionNextSet, compile_collection_next_set_helper);
        register_runtime_helper!(
            ObjectHeaderProjection,
            compile_object_header_projection_helper
        );
        register_runtime_helper!(
            OrdinaryObjectAllocate,
            compile_ordinary_object_allocate_helper
        );
        register_runtime_helper!(ValueToObject, compile_value_to_object_helper);
        register_runtime_helper!(CoerciveAdd, compile_coercive_add_helper);
        register_runtime_helper!(
            LexicalBindingUninitialized,
            compile_lexical_binding_uninitialized_helper
        );
        register_runtime_helper!(RelationalComparison, compile_relational_comparison_helper);
        register_runtime_helper!(
            EnvironmentIdentifierReadSloppy, compile_environment_identifier_read_sloppy_helper;
            EnvironmentIdentifierReadStrict, compile_environment_identifier_read_strict_helper;
            EnvironmentIdentifierReadTypeofSloppy, compile_environment_identifier_read_typeof_sloppy_helper;
            EnvironmentIdentifierReadTypeofStrict, compile_environment_identifier_read_typeof_strict_helper;
            EnvironmentIdentifierResolve, compile_environment_identifier_resolve_helper;
            WithEnvironmentGetBindingValueSloppy, compile_with_environment_get_binding_value_sloppy_helper;
            WithEnvironmentGetBindingValueStrict, compile_with_environment_get_binding_value_strict_helper;
            EnvironmentIdentifierGetSloppy, compile_environment_identifier_get_sloppy_helper;
            EnvironmentIdentifierGetStrict, compile_environment_identifier_get_strict_helper;
            EnvironmentIdentifierTypeofSloppy, compile_environment_identifier_typeof_sloppy_helper;
            EnvironmentIdentifierTypeofStrict, compile_environment_identifier_typeof_strict_helper;
            EnvironmentIdentifierPutSloppy, compile_environment_identifier_put_sloppy_helper;
            EnvironmentIdentifierPutStrict, compile_environment_identifier_put_strict_helper;
            GlobalIdentifierReadSloppy, compile_global_identifier_read_sloppy_helper;
            GlobalIdentifierReadStrict, compile_global_identifier_read_strict_helper;
            GlobalIdentifierTypeofSloppy, compile_global_identifier_typeof_sloppy_helper;
            GlobalIdentifierTypeofStrict, compile_global_identifier_typeof_strict_helper;
        );
        register_runtime_helper!(
            IntegerDecimalString, compile_integer_decimal_string_helper;
            PooledStringResolve, compile_pooled_string_resolve_helper;
            PooledStringsInitialize, compile_pooled_strings_initialize_helper
        );
        register_runtime_helper!(
            PrivateElementAdd, compile_private_element_add_helper;
            PrivateFieldDefine, compile_private_field_define_helper;
            FunctionNameFromKey, compile_function_name_from_key_helper;
            RegExpLiteralAllocate, compile_regexp_literal_allocate_helper;
            NativeRegExpLiteralFactory, compile_native_regexp_literal_factory;
            FunctionMetadataPublish, compile_function_metadata_publish_helper;
            RealmInitializeIntrinsics, compile_realm_initialize_intrinsics_helper;
            AsyncGeneratorStartBody, compile_async_generator_start_body_helper;
            AsyncGeneratorDrainQueue, compile_async_generator_drain_queue_helper;
            PromiseDrainJobs, compile_promise_drain_jobs_helper;
            AsyncAwaitReactions, compile_async_await_reactions_helper;
            AsyncGeneratorAwaitReactions, compile_async_generator_await_reactions_helper;
            AsyncGeneratorYieldReactions, compile_async_generator_yield_reactions_helper;
            AsyncGeneratorYieldReturnReactions, compile_async_generator_yield_return_reactions_helper;
            AsyncGeneratorAwaitReturnReactions, compile_async_generator_await_return_reactions_helper;
            EmbeddingCall, compile_embedding_call;
            EmbeddingRead, compile_embedding_read;
            EmbeddingString, compile_embedding_string;
            EmbeddingBytes, compile_embedding_bytes;
            EmbeddingBytesRead, compile_embedding_bytes_read;
            EmbeddingPromise, compile_embedding_promise;
            EmbeddingCancel, compile_embedding_cancel;
        );
    }
    if compile_program_helpers {
        program_helper_bodies::compile_program_helper_bodies(
            module_package.schema(),
            &string_pool,
            &function_metas,
            uses_heap,
            &runtime_bootstrap_plan,
            runtime_helper_base,
            provided_hooks,
            promise_rejection_policy,
            &mut helper_bodies,
        )?;
    }
    if compile_runtime {
        if let Some(json_stringify_value_helper_function) = json_stringify_value_helper_function {
            helper_bodies.insert(
                RuntimeHelperId::JsonStringifyValue,
                json_stringify_value_helper_function,
            );
        }
    }

    // One RuntimeHelperId traversal selects actual helper bodies. ModuleCode
    // derives each matching declaration from the body identity at the push;
    // helper-index accessors and attribution use the same closed helper domain.
    let mut exports = ExportSection::new();
    entry_exports.append(
        module_package.schema(),
        module_package.global_prelude_phase(),
        script.module_entry_evaluation().is_some(),
        runtime_helper_base,
        &mut exports,
    );

    // Runtime helpers close the runtime half; program helpers follow the
    // program's callables.
    let mut program_helpers = Vec::new();
    if uses_heap {
        for helper in RuntimeHelperId::ALL {
            if !helper.is_emitted(helper_emission) {
                continue;
            }
            let compiled_here = if helper.is_program_owned() {
                compile_program
            } else {
                compile_runtime
            };
            if !compiled_here {
                continue;
            }
            let body = helper_bodies.remove(&helper).unwrap_or_else(|| {
                panic!(
                    "runtime helper `{}` is emitted in this module but has no compiled body",
                    helper.debug_name()
                )
            });
            let emitted = EmittedFunction::runtime_helper(*helper, body);
            if helper.is_program_owned() {
                program_helpers.push(emitted);
            } else {
                compiled_functions.push(emitted);
            }
        }
    }
    assert!(
        helper_bodies.is_empty(),
        "compiled runtime helper bodies were never emitted: {:?}",
        helper_bodies.keys().collect::<Vec<_>>()
    );
    program_functions.extend(program_helpers);
    module_package.append_functions(compiled_functions, program_functions);
    let runtime_global_types = module_package.runtime_global_types();

    // A program module imports memory sized for the runtime's data alone; its
    // own bytes are copied in by `main`.
    let static_memory_bytes = if uses_heap {
        string_pool.compiler_owned_boundary().static_len()
    } else {
        string_pool.bytes.len()
    };
    let mut imports = ImportSection::new();
    imports.import(
        HOST_IMPORT_MODULE,
        HOST_IMPORT_AGENT_CAN_SUSPEND,
        wasm_encoder::EntityType::Function(StaticSignature::HostAgentCanSuspend.type_index()),
    );
    if uses_host_print {
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_PRINT_LINE_UTF8,
            wasm_encoder::EntityType::Function(StaticSignature::HostPrint.type_index()),
        );
    }
    if uses_number_pow_import {
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_NUMBER_POW,
            wasm_encoder::EntityType::Function(StaticSignature::HostNumberPow.type_index()),
        );
    }
    if uses_wall_clock_millis {
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_WALL_CLOCK_MILLIS,
            wasm_encoder::EntityType::Function(StaticSignature::HostWallClockMillis.type_index()),
        );
    }
    if uses_shared_memory {
        if uses_atomics_wait_async {
            imports.import(
                HOST_IMPORT_MODULE,
                HOST_IMPORT_MONOTONIC_CLOCK_NANOS,
                wasm_encoder::EntityType::Function(
                    StaticSignature::HostMonotonicClockNanos.type_index(),
                ),
            );
            imports.import(
                HOST_IMPORT_MODULE,
                HOST_IMPORT_SLEEP_NANOS,
                wasm_encoder::EntityType::Function(StaticSignature::HostSleepNanos.type_index()),
            );
        }
        let initial_pages = initial_memory_pages(static_memory_bytes, uses_heap);
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_PRIVATE_MEMORY,
            wasm_encoder::EntityType::Memory(MemoryType {
                minimum: initial_pages,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        );
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_SHARED_MEMORY,
            wasm_encoder::EntityType::Memory(MemoryType {
                minimum: 1,
                maximum: Some(16_384),
                memory64: false,
                shared: true,
                page_size_log2: None,
            }),
        );
    }
    if uses_agent_call {
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_AGENT_CALL,
            wasm_encoder::EntityType::Function(StaticSignature::HostAgentCall.type_index()),
        );
    }
    if uses_random_f64 {
        imports.import(
            HOST_IMPORT_MODULE,
            HOST_IMPORT_RANDOM_F64,
            wasm_encoder::EntityType::Function(StaticSignature::HostWallClockMillis.type_index()),
        );
    }
    // The Math block, in the same order as the claimed indices above. The
    // nineteen unary imports share one `(f64) -> f64` type; `math_atan2`
    // reuses the `(f64, f64) -> f64` number-power type.
    for (used, name, type_index) in [
        (
            uses_math_acos,
            HOST_IMPORT_MATH_ACOS,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_acosh,
            HOST_IMPORT_MATH_ACOSH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_asin,
            HOST_IMPORT_MATH_ASIN,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_asinh,
            HOST_IMPORT_MATH_ASINH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_atan,
            HOST_IMPORT_MATH_ATAN,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_atanh,
            HOST_IMPORT_MATH_ATANH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_cbrt,
            HOST_IMPORT_MATH_CBRT,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_cos,
            HOST_IMPORT_MATH_COS,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_cosh,
            HOST_IMPORT_MATH_COSH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_exp,
            HOST_IMPORT_MATH_EXP,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_expm1,
            HOST_IMPORT_MATH_EXPM1,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_log,
            HOST_IMPORT_MATH_LOG,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_log10,
            HOST_IMPORT_MATH_LOG10,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_log1p,
            HOST_IMPORT_MATH_LOG1P,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_log2,
            HOST_IMPORT_MATH_LOG2,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_sin,
            HOST_IMPORT_MATH_SIN,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_sinh,
            HOST_IMPORT_MATH_SINH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_tan,
            HOST_IMPORT_MATH_TAN,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_tanh,
            HOST_IMPORT_MATH_TANH,
            StaticSignature::HostMathUnary.type_index(),
        ),
        (
            uses_math_atan2,
            HOST_IMPORT_MATH_ATAN2,
            StaticSignature::HostNumberPow.type_index(),
        ),
    ] {
        if used {
            imports.import(
                HOST_IMPORT_MODULE,
                name,
                wasm_encoder::EntityType::Function(type_index),
            );
        }
    }
    imports.import(
        HOST_IMPORT_MODULE,
        HOST_IMPORT_REJECT_RUNTIME_SEMANTICS,
        wasm_encoder::EntityType::Function(StaticSignature::HostSleepNanos.type_index()),
    );

    function_metas
        .gc_host_imports()
        .emit_declarations(&mut imports);
    if let ModuleKind::Program(runtime) = kind {
        runtime
            .layout()
            .emit_runtime_imports(layout.defined_functions().start, &mut imports)?;
    }
    function_metas.foreign_imports().emit_imports(&mut imports);

    let mut memories = None;
    let mut data = None;
    if !string_pool.bytes.is_empty() || uses_heap {
        if !uses_shared_memory {
            let mut section = MemorySection::new();
            section.memory(MemoryType {
                minimum: initial_memory_pages(static_memory_bytes, uses_heap),
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            memories = Some(section);
        }
        exports.export("memory", ExportKind::Memory, 0);
        let mut section = DataSection::new();
        match kind {
            ModuleKind::Standalone => {
                section.active(
                    0,
                    &ConstExpr::i32_const(STATIC_DATA_OFFSET as i32),
                    string_pool.bytes.iter().copied(),
                );
            }
            ModuleKind::Runtime(_) => string_pool.append_runtime_data(&mut section),
            ModuleKind::Program(_) => string_pool.append_program_data(&mut section),
        }
        data = Some(section);
    }
    if compile_runtime {
        exports.export(
            crate::NATIVE_REGEXP_LITERAL_FACTORY_EXPORT,
            ExportKind::Func,
            RuntimeHelperId::NativeRegExpLiteralFactory
                .index(runtime_helper_base.expect("runtime has typed helpers")),
        );
        crate::runtime_artifact::export_runtime_symbols(
            &mut exports,
            layout.defined_functions().start..layout.first_program_index(),
            runtime_global_types.len() as u32,
        );
    }

    let main_emitted_local_count = module_package.main_emitted_local_count();
    let mut module = Module::new();
    let mut function_table = module_package.append_to_module(
        &mut module,
        ModuleAssemblySections::new(
            imports,
            if compile_runtime {
                layout.defined_functions().start..layout.first_program_index()
            } else {
                layout.defined_functions()
            },
            memories,
            exports,
            data,
            match kind {
                ModuleKind::Program(_) => runtime_global_types.len() as u32,
                ModuleKind::Standalone | ModuleKind::Runtime(_) => 0,
            },
        ),
    );
    let mut debug_dump = vec![
        "module: js-aot".to_string(),
        entry_exports.debug_entry().to_string(),
        format!("static result kind: {}", script.result_kind().as_str()),
        format!("locals: {main_emitted_local_count}"),
        format!(
            "internal functions: {}",
            layout.defined_functions().len()
                - layout.foreign_import_count() as usize
                - layout.program_prefix_count()
                - function_table.runtime_helper_count()
        ),
        // Derived from the same table that owns the emitted helper bodies.
        format!(
            "runtime helper functions: {}",
            function_table.runtime_helper_count()
        ),
        format!(
            "standard builtin bodies: {} real, 0 shared-stubbed",
            compiled_standard_builtins.len()
        ),
        format!(
            "runtime bootstrap: {} standard roots, full globals={}",
            runtime_bootstrap_plan.standard_roots.len(),
            runtime_bootstrap_plan.full_standard_globals
        ),
        format!(
            "standard builtin real names: {}",
            compiled_standard_builtins
                .iter()
                .map(|builtin| builtin.debug_name())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!(
            "host builtin bodies: {} real, 0 shared-stubbed",
            compiled_host_builtins.len()
        ),
        format!(
            "host builtin real names: {}",
            compiled_host_builtins
                .iter()
                .map(|builtin| builtin.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!(
            "scalar globals: {}",
            NUMERIC_STATE_GLOBAL_COUNT + module_guard_count
        ),
        "main completion ABI: tag/scalar/reference/kind/target".to_owned(),
        format!("export global: {THROW_ERROR_NAME_EXPORT}"),
        format!("export global: {THROW_ERROR_MESSAGE_EXPORT}"),
        format!("export global: {THROW_ERROR_CONSTRUCTOR_NAME_EXPORT}"),
        format!("import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_AGENT_CAN_SUSPEND}"),
    ];

    let attribution_position = debug_dump.len();
    if uses_host_print {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_PRINT_LINE_UTF8}"
        ));
    }
    if uses_number_pow_import {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_NUMBER_POW}"
        ));
    }
    if uses_wall_clock_millis {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_WALL_CLOCK_MILLIS}"
        ));
    }
    for import in function_metas.gc_host_imports().imports() {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{}",
            import.name()
        ));
    }
    if uses_shared_memory {
        debug_dump.push(format!(
            "import memory: {HOST_IMPORT_MODULE}.{HOST_IMPORT_PRIVATE_MEMORY}"
        ));
        debug_dump.push(format!(
            "import memory: {HOST_IMPORT_MODULE}.{HOST_IMPORT_SHARED_MEMORY}"
        ));
    }
    if uses_atomics_wait_async {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_MONOTONIC_CLOCK_NANOS}"
        ));
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_SLEEP_NANOS}"
        ));
    }
    if uses_agent_call {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_AGENT_CALL}"
        ));
    }
    if uses_random_f64 {
        debug_dump.push(format!(
            "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_RANDOM_F64}"
        ));
    }
    for (used, name) in [
        (uses_math_acos, HOST_IMPORT_MATH_ACOS),
        (uses_math_acosh, HOST_IMPORT_MATH_ACOSH),
        (uses_math_asin, HOST_IMPORT_MATH_ASIN),
        (uses_math_asinh, HOST_IMPORT_MATH_ASINH),
        (uses_math_atan, HOST_IMPORT_MATH_ATAN),
        (uses_math_atanh, HOST_IMPORT_MATH_ATANH),
        (uses_math_cbrt, HOST_IMPORT_MATH_CBRT),
        (uses_math_cos, HOST_IMPORT_MATH_COS),
        (uses_math_cosh, HOST_IMPORT_MATH_COSH),
        (uses_math_exp, HOST_IMPORT_MATH_EXP),
        (uses_math_expm1, HOST_IMPORT_MATH_EXPM1),
        (uses_math_log, HOST_IMPORT_MATH_LOG),
        (uses_math_log10, HOST_IMPORT_MATH_LOG10),
        (uses_math_log1p, HOST_IMPORT_MATH_LOG1P),
        (uses_math_log2, HOST_IMPORT_MATH_LOG2),
        (uses_math_sin, HOST_IMPORT_MATH_SIN),
        (uses_math_sinh, HOST_IMPORT_MATH_SINH),
        (uses_math_tan, HOST_IMPORT_MATH_TAN),
        (uses_math_tanh, HOST_IMPORT_MATH_TANH),
        (uses_math_atan2, HOST_IMPORT_MATH_ATAN2),
    ] {
        if used {
            debug_dump.push(format!("import func: {HOST_IMPORT_MODULE}.{name}"));
        }
    }
    debug_dump.push(format!(
        "import func: {HOST_IMPORT_MODULE}.{HOST_IMPORT_REJECT_RUNTIME_SEMANTICS}"
    ));

    metadata::append_memory_attribution(
        &mut debug_dump,
        &string_pool,
        uses_heap,
        uses_shared_memory,
    );

    intl::append_sections(
        &mut module,
        kind,
        &string_pool,
        intl_selection,
        uses_intl_host,
        uses_system_time_zone,
        &mut debug_dump,
    )?;

    metadata::append_module_declarations(&mut module, kind, &script.foreign_modules);
    metadata::append_function_name(&mut module, &mut debug_dump, &function_table)?;
    if let Some(section) = factory.and_then(|_| function_table.native_program_context_section()) {
        module.section(&section);
    }

    let runtime_layout = matches!(kind, ModuleKind::Runtime(_)).then(|| {
        RuntimeLayout::new(
            layout.defined_functions().start,
            function_table.type_indices().to_vec(),
            runtime_global_types,
            string_pool.compiler_owned_boundary(),
        )
    });
    let bytes = module.finish();
    let bytes = match kind {
        ModuleKind::Program(_) => crate::program_imports::compact(bytes, &mut function_table)?,
        ModuleKind::Standalone | ModuleKind::Runtime(_) => bytes,
    };
    let mut attribution = Vec::new();
    let function_sizes = metadata::append_function_attribution(&mut attribution, &function_table);
    debug_dump.splice(attribution_position..attribution_position, attribution);
    if let Some(budget) = FunctionBodyBudget::from_env() {
        function_table.check_budget(budget)?;
    }
    let artifact = WasmArtifact {
        bytes,
        invariant_note: "direct-js-to-wasm module",
        debug_dump: debug_dump.join("\n"),
        function_sizes,
        gc_host_imports: function_metas.gc_host_imports().imports().collect(),
        runtime: match kind {
            ModuleKind::Program(runtime) => Some(runtime.clone()),
            ModuleKind::Standalone | ModuleKind::Runtime(_) => None,
        },
    };
    Ok(EmittedModule {
        artifact,
        runtime_layout,
    })
}
