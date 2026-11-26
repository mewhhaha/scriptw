//! Real collector and native byte-resource imports from the emitted GC ABI.

use super::wasm_shared_resource::{AsyncWaitDeadline, AsyncWaitRegistration, NativeSyncWaitResult};
use super::*;
use crate::host_imports::FixedHostFunctions;
use scriptw_aot_wasm::GcHostImport;
use wasmtime::{ExternRef, Rooted};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) enum TypedGcHostImport<'module> {
    #[default]
    Missing,
    Single(wasmtime::ImportType<'module>),
    Duplicate,
}

impl<'module> TypedGcHostImport<'module> {
    pub(super) fn record(&mut self, import: wasmtime::ImportType<'module>) {
        *self = match core::mem::replace(self, Self::Missing) {
            Self::Missing => Self::Single(import),
            Self::Single(_) | Self::Duplicate => Self::Duplicate,
        };
    }

    pub(super) fn is_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }

    pub(super) fn into_func(
        self,
        duplicate_error: &str,
        non_func_error: &str,
    ) -> Result<Option<wasmtime::FuncType>, EngineError> {
        match self {
            Self::Missing => Ok(None),
            Self::Duplicate => Err(EngineError::new(duplicate_error)),
            Self::Single(import) => match import.ty() {
                wasmtime::ExternType::Func(function) => Ok(Some(function)),
                _ => Err(EngineError::new(non_func_error)),
            },
        }
    }
}

#[derive(Default)]
pub(super) struct ScannedGcHostImports<'module> {
    agent_broadcast: TypedGcHostImport<'module>,
    agent_receive: TypedGcHostImport<'module>,
    byte_array_allocate: TypedGcHostImport<'module>,
    compile_dynamic_function: TypedGcHostImport<'module>,
    compile_global_script: TypedGcHostImport<'module>,
    compile_direct_eval: TypedGcHostImport<'module>,
    intl_provider_call: TypedGcHostImport<'module>,
    system_time_zone_snapshot: TypedGcHostImport<'module>,
}

impl<'module> ScannedGcHostImports<'module> {
    pub(super) fn scan(
        module: &'module WasmtimeModule,
        mut record_name: impl FnMut(&'module str) -> Result<(), EngineError>,
    ) -> Result<Self, EngineError> {
        let mut scanned = Self::default();
        let host_module = GcHostImport::CollectGc.module();
        debug_assert!(GcHostImport::ALL
            .iter()
            .all(|row| row.module() == host_module));
        for import in module.imports() {
            if import.module() != host_module {
                continue;
            }
            record_name(import.name())?;
            let slot = match import.name() {
                name if name == GcHostImport::AgentBroadcastResource.name() => {
                    &mut scanned.agent_broadcast
                }
                name if name == GcHostImport::AgentReceiveResource.name() => {
                    &mut scanned.agent_receive
                }
                name if name == GcHostImport::ByteArrayAllocate.name() => {
                    &mut scanned.byte_array_allocate
                }
                name if name == GcHostImport::CompileDynamicFunction.name() => {
                    &mut scanned.compile_dynamic_function
                }
                name if name == GcHostImport::CompileGlobalScript.name() => {
                    &mut scanned.compile_global_script
                }
                name if name == GcHostImport::CompileDirectEval.name() => {
                    &mut scanned.compile_direct_eval
                }
                name if name == GcHostImport::IntlProviderCall.name() => {
                    &mut scanned.intl_provider_call
                }
                name if name == GcHostImport::SystemTimeZoneSnapshot.name() => {
                    &mut scanned.system_time_zone_snapshot
                }
                _ => continue,
            };
            slot.record(import);
        }
        Ok(scanned)
    }

    fn has_native_source_compiler(&self) -> bool {
        self.compile_dynamic_function.is_present()
            || self.compile_global_script.is_present()
            || self.compile_direct_eval.is_present()
    }
}

#[cfg(test)]
fn imports_native_source_compiler(module: &WasmtimeModule) -> bool {
    ScannedGcHostImports::scan(module, |_| Ok(()))
        .expect("recording no names cannot fail")
        .has_native_source_compiler()
}

pub(super) fn resource(
    caller: &WasmtimeCaller<'_, WasmHostState>,
    reference: Rooted<ExternRef>,
) -> wasmtime::Result<Arc<WasmSharedBufferResource>> {
    let resource = reference
        .data(caller)?
        .and_then(|data| data.downcast_ref::<Arc<WasmSharedBufferResource>>())
        .cloned()
        .ok_or_else(|| wasmtime::Error::msg("invalid shared byte resource host type"))?;
    let backing = caller
        .data()
        .shared_memory_backing
        .as_ref()
        .ok_or_else(|| {
            wasmtime::Error::msg("shared byte resource requires imported shared memory")
        })?;
    backing.check_resource(&resource)?;
    Ok(resource)
}

#[derive(Clone, Copy)]
enum SharedResourceScalar {
    Base,
    Length,
    Maximum,
}

#[cfg(test)]
pub(super) fn link(
    linker: &mut WasmtimeLinker<WasmHostState>,
    module: &WasmtimeModule,
) -> Result<(), EngineError> {
    link_scanned(
        &mut FixedHostFunctions::all(linker),
        ScannedGcHostImports::scan(module, |_| Ok(()))?,
    )
}

pub(super) fn link_scanned(
    linker: &mut FixedHostFunctions<'_, '_>,
    scanned: ScannedGcHostImports<'_>,
) -> Result<(), EngineError> {
    let has_native_source_compiler = scanned.has_native_source_compiler();
    #[cfg(not(feature = "native-source-compiler"))]
    if has_native_source_compiler {
        return Err(EngineError::new(
            "native source compilation is unavailable in this build; enable native-source-compiler",
        ));
    }
    let bind_error =
        |error| EngineError::new(format!("wasmtime GC host linker setup failed: {error}"));
    let import = GcHostImport::CollectGc;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>| -> wasmtime::Result<()> {
                caller.gc(None)
            },
        )
        .map_err(bind_error)?;

    let import = GcHostImport::SharedBufferAllocate;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |mut caller: WasmtimeCaller<'_, WasmHostState>,
             initial: i64,
             maximum: i64,
             growable: i32|
             -> wasmtime::Result<Option<Rooted<ExternRef>>> {
                let initial = u64::try_from(initial)
                    .map_err(|_| wasmtime::Error::msg("negative shared-buffer length"))?;
                let maximum = u64::try_from(maximum)
                    .map_err(|_| wasmtime::Error::msg("negative shared-buffer maximum"))?;
                let growable = match growable {
                    0 => false,
                    1 => true,
                    _ => return Err(wasmtime::Error::msg("invalid shared-buffer growability")),
                };
                let backing = caller.data().shared_memory_backing.clone().ok_or_else(|| {
                    wasmtime::Error::msg("shared-buffer allocation requires imported shared memory")
                })?;
                let Some(resource) = backing.allocate(initial, maximum, growable)? else {
                    return Ok(None);
                };
                ExternRef::new(&mut caller, resource).map(Some)
            },
        )
        .map_err(bind_error)?;

    for (import, operation) in [
        (GcHostImport::SharedBufferBase, SharedResourceScalar::Base),
        (
            GcHostImport::SharedBufferLength,
            SharedResourceScalar::Length,
        ),
        (
            GcHostImport::SharedBufferMaximum,
            SharedResourceScalar::Maximum,
        ),
    ] {
        linker
            .func_wrap(
                import.module(),
                import.name(),
                move |caller: WasmtimeCaller<'_, WasmHostState>,
                      reference: Rooted<ExternRef>|
                      -> wasmtime::Result<i64> {
                    let resource = resource(&caller, reference)?;
                    let value = match operation {
                        SharedResourceScalar::Base => resource.base(),
                        SharedResourceScalar::Length => resource.length(),
                        SharedResourceScalar::Maximum => resource.maximum(),
                    };
                    i64::try_from(value)
                        .map_err(|_| wasmtime::Error::msg("shared resource scalar exceeds ABI"))
                },
            )
            .map_err(bind_error)?;
    }
    let import = GcHostImport::SharedBufferGrowable;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |caller: WasmtimeCaller<'_, WasmHostState>,
             reference: Rooted<ExternRef>|
             -> wasmtime::Result<i32> {
                Ok(i32::from(resource(&caller, reference)?.growable()))
            },
        )
        .map_err(bind_error)?;

    let import = GcHostImport::SharedBufferGrow;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |caller: WasmtimeCaller<'_, WasmHostState>,
             reference: Rooted<ExternRef>,
             length: i64|
             -> wasmtime::Result<i32> {
                let length = u64::try_from(length)
                    .map_err(|_| wasmtime::Error::msg("negative shared-buffer growth"))?;
                Ok(i32::from(resource(&caller, reference)?.grow(length)))
            },
        )
        .map_err(bind_error)?;

    wasm_agent_resource_host::link(
        linker.linker,
        scanned.agent_broadcast,
        scanned.agent_receive,
    )?;

    let import = GcHostImport::RegisterAsyncWaiter;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |caller: WasmtimeCaller<'_, WasmHostState>,
             reference: Rooted<ExternRef>,
             offset: i64,
             width: i32,
             expected_word: i64,
             deadline: i64|
             -> wasmtime::Result<i64> {
                let deadline = AsyncWaitDeadline::from_wire(
                    deadline,
                    &caller.data().realm,
                    caller.data().monotonic_clock_origin,
                )?;
                caller
                    .data()
                    .async_waiters
                    .register(
                        resource(&caller, reference)?,
                        offset,
                        width,
                        expected_word,
                        deadline,
                    )
                    .map(AsyncWaitRegistration::wire)
            },
        )
        .map_err(bind_error)?;

    let import = GcHostImport::SharedBufferWait;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |caller: WasmtimeCaller<'_, WasmHostState>,
             reference: Rooted<ExternRef>,
             offset: i64,
             width: i32,
             expected: i64,
             timeout_nanos: i64|
             -> wasmtime::Result<i32> {
                caller
                    .data()
                    .async_waiters
                    .wait(
                        resource(&caller, reference)?,
                        offset,
                        width,
                        expected,
                        timeout_nanos,
                    )
                    .map(NativeSyncWaitResult::wire)
            },
        )
        .map_err(bind_error)?;

    let import = GcHostImport::NotifyAsyncWaiters;
    linker
        .func_wrap(
            import.module(),
            import.name(),
            |caller: WasmtimeCaller<'_, WasmHostState>,
             reference: Rooted<ExternRef>,
             offset: i64,
             width: i32,
             count: i64|
             -> wasmtime::Result<i64> {
                let resource = resource(&caller, reference)?;
                caller
                    .data()
                    .shared_memory_backing
                    .as_ref()
                    .ok_or_else(|| {
                        wasmtime::Error::msg("Atomics.notify requires imported shared memory")
                    })?
                    .notify_async_waiters(&resource, offset, width, count)
            },
        )
        .map_err(bind_error)?;
    wasm_gc_byte_array_host::link(linker.linker, scanned.byte_array_allocate)?;
    // Portable R has no native source compiler imports. Avoid entering these
    // larger binding routines for an immutable module that needs none of them.
    // A present name still enters every original validator, regardless of its
    // type or the Store's compiler context (which may be armed after linking).
    #[cfg(feature = "native-source-compiler")]
    if has_native_source_compiler {
        wasm_native_function_compiler::link(linker.linker, scanned.compile_dynamic_function)?;
        wasm_native_global_script_compiler::link(linker.linker, scanned.compile_global_script)?;
        wasm_native_direct_eval_compiler::link(linker.linker, scanned.compile_direct_eval)?;
    }
    wasm_gc_intl_host::link_imports(
        linker.linker,
        scanned.intl_provider_call,
        scanned.system_time_zone_snapshot,
    )?;
    wasm_gc_weak_host::link(linker)?;
    Ok(())
}
