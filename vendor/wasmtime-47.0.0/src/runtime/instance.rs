use crate::linker::{Definition, DefinitionType};
use crate::prelude::*;
use crate::runtime::vm::{
    self, Imports, ModuleRuntimeInfo, VMFuncRef, VMFunctionImport, VMGlobalImport, VMMemoryImport,
    VMStore, VMTableImport, VMTagImport,
};
use crate::store::{
    AllocateInstanceKind, Asyncness, InstanceId, StoreInstanceId, StoreOpaque, StoreResourceLimiter,
};
use crate::types::matching;
use crate::{
    AsContextMut, Engine, Export, Extern, Func, Global, Memory, MemoryType, Module, ModuleExport,
    SharedMemory, StoreContext, StoreContextMut, Table, Tag, TypedFunc,
};
use alloc::sync::Arc;
use core::ptr::NonNull;
use wasmtime_environ::{
    EntityIndex, EntityType, FuncIndex, GlobalIndex, MemoryIndex, TableIndex, TagIndex, TypeTrace,
};

/// An instantiated WebAssembly module.
///
/// This type represents the instantiation of a [`Module`]. Once instantiated
/// you can access the [`exports`](Instance::exports) which are of type
/// [`Extern`] and provide the ability to call functions, set globals, read
/// memory, etc. When interacting with any wasm code you'll want to make an
/// [`Instance`] to call any code or execute anything.
///
/// Instances are owned by a [`Store`](crate::Store) which is passed in at
/// creation time. It's recommended to create instances with
/// [`Linker::instantiate`](crate::Linker::instantiate) or similar
/// [`Linker`](crate::Linker) methods, but a more low-level constructor is also
/// available as [`Instance::new`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Instance {
    pub(crate) id: StoreInstanceId,
}

// Double-check that the C representation in `instance.h` matches our in-Rust
// representation here in terms of size/alignment/etc.
const _: () = {
    #[repr(C)]
    struct C(u64, usize);
    assert!(core::mem::size_of::<C>() == core::mem::size_of::<Instance>());
    assert!(core::mem::align_of::<C>() == core::mem::align_of::<Instance>());
    assert!(core::mem::offset_of!(Instance, id) == 0);
};

impl Instance {
    /// Creates a new [`Instance`] from the previously compiled [`Module`] and
    /// list of `imports` specified.
    ///
    /// This method instantiates the `module` provided with the `imports`,
    /// following the procedure in the [core specification][inst] to
    /// instantiate. Instantiation can fail for a number of reasons (many
    /// specified below), but if successful the `start` function will be
    /// automatically run (if specified in the `module`) and then the
    /// [`Instance`] will be returned.
    ///
    /// Per the WebAssembly spec, instantiation includes running the module's
    /// start function, if it has one (not to be confused with the `_start`
    /// function, which is not run).
    ///
    /// Note that this is a low-level function that just performs an
    /// instantiation. See the [`Linker`](crate::Linker) struct for an API which
    /// provides a convenient way to link imports and provides automatic Command
    /// and Reactor behavior.
    ///
    /// ## Providing Imports
    ///
    /// The entries in the list of `imports` are intended to correspond 1:1
    /// with the list of imports returned by [`Module::imports`]. Before
    /// calling [`Instance::new`] you'll want to inspect the return value of
    /// [`Module::imports`] and, for each import type, create an [`Extern`]
    /// which corresponds to that type.  These [`Extern`] values are all then
    /// collected into a list and passed to this function.
    ///
    /// Note that this function is intentionally relatively low level. For an
    /// easier time passing imports by doing name-based resolution it's
    /// recommended to instead use the [`Linker`](crate::Linker) type.
    ///
    /// ## Errors
    ///
    /// This function can fail for a number of reasons, including, but not
    /// limited to:
    ///
    /// * The number of `imports` provided doesn't match the number of imports
    ///   returned by the `module`'s [`Module::imports`] method.
    /// * The type of any [`Extern`] doesn't match the corresponding
    ///   [`ExternType`] entry that it maps to.
    /// * The `start` function in the instance, if present, traps.
    /// * Module/instance resource limits are exceeded.
    /// * The `store` provided requires the use of [`Instance::new_async`]
    ///   instead, such as if epochs or fuel are configured.
    ///
    /// When instantiation fails it's recommended to inspect the return value to
    /// see why it failed, or bubble it upwards. If you'd like to specifically
    /// check for trap errors, you can use `error.downcast::<Trap>()`. For more
    /// about error handling see the [`Trap`] documentation.
    ///
    /// [`Trap`]: crate::Trap
    ///
    /// # Panics
    ///
    /// This function will panic if any [`Extern`] supplied is not owned by
    /// `store`.
    ///
    /// [inst]: https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation
    /// [`ExternType`]: crate::ExternType
    ///
    /// This function will return an [`OutOfMemory`][crate::OutOfMemory] error when
    /// memory allocation fails. See the `OutOfMemory` type's documentation for
    /// details on Wasmtime's out-of-memory handling.
    pub fn new(
        mut store: impl AsContextMut,
        module: &Module,
        imports: &[Extern],
    ) -> Result<Instance> {
        let mut store = store.as_context_mut();
        store.0.validate_sync_call()?;
        let imports = Instance::typecheck_externs(store.0, module, imports)?;
        // Note that the unsafety here should be satisfied by the call to
        // `typecheck_externs` above which satisfies the condition that all
        // the imports are valid for this module.
        vm::assert_ready(unsafe {
            Instance::new_started(&mut store, module, imports.as_ref(), Asyncness::No)
        })
    }

    /// Same as [`Instance::new`], except for usage in [asynchronous stores].
    ///
    /// For more details about this function see the documentation on
    /// [`Instance::new`]. The only difference between these two methods is that
    /// this one will asynchronously invoke the wasm start function in case it
    /// calls any imported function which is an asynchronous host function (e.g.
    /// created with [`Func::new_async`](crate::Func::new_async).
    ///
    /// # Panics
    ///
    /// This function will panic, like [`Instance::new`], if any [`Extern`]
    /// specified does not belong to `store`.
    ///
    /// # Examples
    ///
    /// An example of using this function:
    ///
    /// ```
    /// use wasmtime::{Result, Store, Engine, Module, Instance};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let engine = Engine::default();
    ///
    ///     // For this example, a module with no imports is being used hence
    ///     // the empty array to `Instance::new_async`.
    ///     let module = Module::new(&engine, "(module)")?;
    ///     let mut store = Store::new(&engine, ());
    ///     let instance = Instance::new_async(&mut store, &module, &[]).await?;
    ///
    ///     // ... use `instance` and exports and such ...
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Note, though, that the future returned from this function is only
    /// `Send` if the store's own data is `Send` meaning that this does not
    /// compile for example:
    ///
    /// ```compile_fail
    /// use wasmtime::{Result, Store, Engine, Module, Instance};
    /// use std::rc::Rc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let engine = Engine::default();
    ///
    ///     let module = Module::new(&engine, "(module)")?;
    ///
    ///     // Note that `Rc<()>` is NOT `Send`, which is what many future
    ///     // runtimes require and below will cause a failure.
    ///     let mut store = Store::new(&engine, Rc::new(()));
    ///
    ///     // Compile failure because `Store<Rc<()>>` is not `Send`
    ///     assert_send(Instance::new_async(&mut store, &module, &[])).await?;
    ///
    ///     Ok(())
    /// }
    ///
    /// fn assert_send<T: Send>(t: T) -> T { t }
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return an [`OutOfMemory`][crate::OutOfMemory] error when
    /// memory allocation fails. See the `OutOfMemory` type's documentation for
    /// details on Wasmtime's out-of-memory handling.
    #[cfg(feature = "async")]
    pub async fn new_async(
        mut store: impl AsContextMut,
        module: &Module,
        imports: &[Extern],
    ) -> Result<Instance> {
        let mut store = store.as_context_mut();
        let imports = Instance::typecheck_externs(store.0, module, imports)?;
        // See `new` for notes on this unsafety
        unsafe { Instance::new_started(&mut store, module, imports.as_ref(), Asyncness::Yes).await }
    }

    fn typecheck_externs(
        store: &mut StoreOpaque,
        module: &Module,
        imports: &[Extern],
    ) -> Result<OwnedImports> {
        for import in imports {
            if !import.comes_from_same_store(store) {
                bail!("cross-`Store` instantiation is not currently supported");
            }
        }

        typecheck(module, imports, |cx, ty, item| {
            let item = DefinitionType::from(store, item);
            cx.definition(ty, &item)
        })?;

        // When pushing functions into `OwnedImports` it's required that their
        // `wasm_call` fields are all filled out. This `module` is guaranteed
        // to have any trampolines necessary for functions so register the
        // module with the store and then attempt to fill out any outstanding
        // holes.
        //
        // Note that under normal operation this shouldn't do much as the list
        // of funcs-with-holes should generally be empty. As a result the
        // process of filling this out is not super optimized at this point.
        let (modules, engine, breakpoints) = store.modules_and_engine_and_breakpoints_mut();
        modules.register_module(module, engine, breakpoints)?;
        let (funcrefs, modules) = store.func_refs_and_modules();
        funcrefs.fill(modules);

        let mut owned_imports = OwnedImports::new(module)?;
        for import in imports {
            owned_imports.push(import, store)?;
        }
        Ok(owned_imports)
    }

    /// Internal function to create an instance and run the start function.
    ///
    /// This function's unsafety is the same as `Instance::new_raw`.
    pub(crate) async unsafe fn new_started<T>(
        store: &mut StoreContextMut<'_, T>,
        module: &Module,
        imports: Imports<'_>,
        asyncness: Asyncness,
    ) -> Result<Instance> {
        let instance = {
            let (mut limiter, store) = store.0.resource_limiter_and_store_opaque();
            // SAFETY: the safety contract of `new_raw` is the same as this
            // function.
            unsafe { Instance::new_raw(store, limiter.as_mut(), module, imports).await? }
        };

        // If this instance requires startup, which is a dynamic decision made
        // at this point in conjunction with analysis at compile time, the
        // instance gets started. Note that this isn't just the wasm start
        // function itself, but it's finalization of initialization of this
        // instance, for example for complicated global initialization
        // expressions.
        if instance.id.get_mut(store.0).needs_startup() {
            if asyncness == Asyncness::No {
                instance.start_raw(store)?;
            } else {
                #[cfg(feature = "async")]
                {
                    store.on_fiber(|store| instance.start_raw(store)).await??;
                }
                #[cfg(not(feature = "async"))]
                unreachable!();
            }
        }
        Ok(instance)
    }

    /// Internal function to create an instance which doesn't have its `start`
    /// function run yet.
    ///
    /// This is not intended to be exposed from Wasmtime, it's intended to
    /// refactor out common code from `new_started` and `new_started_async`.
    ///
    /// Note that this step needs to be run on a fiber in async mode even
    /// though it doesn't do any blocking work because an async resource
    /// limiter may need to yield.
    ///
    /// # Unsafety
    ///
    /// This method is unsafe because it does not type-check the `imports`
    /// provided. The `imports` provided must be suitable for the module
    /// provided as well.
    async unsafe fn new_raw(
        store: &mut StoreOpaque,
        mut limiter: Option<&mut StoreResourceLimiter<'_>>,
        module: &Module,
        imports: Imports<'_>,
    ) -> Result<Instance> {
        if !Engine::same(store.engine(), module.engine()) {
            bail!("cross-`Engine` instantiation is not currently supported");
        }
        store.bump_resource_counts(module)?;

        // Allocate the GC heap, if necessary.
        if module.env_module().needs_gc_heap {
            store.ensure_gc_store(limiter.as_deref_mut()).await?;
        }

        // Register the module just before instantiation to ensure we keep the module
        // properly referenced while in use by the store.
        let (modules, engine, breakpoints) = store.modules_and_engine_and_breakpoints_mut();
        let module_id = modules.register_module(module, engine, breakpoints)?;

        // The first thing we do is issue an instance allocation request
        // to the instance allocator. This, on success, will give us an
        // instance handle.
        //
        // SAFETY: this module, by construction, was already validated within
        // the store.
        let id = unsafe {
            store
                .allocate_instance(
                    limiter.as_deref_mut(),
                    AllocateInstanceKind::Module(module_id),
                    &ModuleRuntimeInfo::Module(module.clone()),
                    imports,
                )
                .await?
        };

        // At this point the instance is created and stored within the store,
        // but it's also not quite usable just yet. Initialization hasn't
        // completed (e.g. active data/element segments) and the `start`
        // function additionally has not yet been invoked. That's the
        // responsibility of the caller to handle, however.
        Ok(Instance::from_wasmtime(id, store))
    }

    pub(crate) fn from_wasmtime(id: InstanceId, store: &mut StoreOpaque) -> Instance {
        Instance {
            id: StoreInstanceId::new(store.id(), id),
        }
    }

    fn start_raw<T>(&self, store: &mut StoreContextMut<'_, T>) -> Result<()> {
        // If a start function is present, invoke it. Make sure we use all the
        // trap-handling configuration in `store` as well.
        let store_id = store.0.id();
        let (mut instance, registry) = self.id.get_mut_and_module_registry(store.0);
        // SAFETY: the `store_id` is the id of the store that owns this
        // instance and any function stored within the instance.
        let f = unsafe {
            instance
                .as_mut()
                .get_startup_func(registry, store_id)
                .expect("should have a startup function")
        };
        let caller_vmctx = instance.vmctx();
        unsafe {
            let funcref = f.vm_func_ref(store.0);
            super::func::invoke_wasm_and_catch_traps(store, |_default_caller, vm| {
                VMFuncRef::array_call(funcref, vm, caller_vmctx, NonNull::from(&mut []))
            })?;
        }
        Ok(())
    }

    /// Get this instance's module.
    pub fn module<'a, T: 'static>(&self, store: impl Into<StoreContext<'a, T>>) -> &'a Module {
        self._module(store.into().0)
    }

    pub(crate) fn _module<'a>(&self, store: &'a StoreOpaque) -> &'a Module {
        store.module_for_instance(self.id).unwrap()
    }

    /// Returns the list of exported items from this [`Instance`].
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance, or if memory allocation
    /// fails.
    pub fn exports<'a, T: 'static>(
        &'a self,
        store: impl Into<StoreContextMut<'a, T>>,
    ) -> impl ExactSizeIterator<Item = Export<'a>> + 'a {
        let store = store.into().0;
        let store_id = store.id();
        let engine = store.engine().clone();

        let (instance, registry) = store.instance_and_module_registry_mut(self.id());
        let (module, mut instance) = instance.module_and_self();
        module.exports.iter().map(move |(name, entity)| {
            // SAFETY: the `store_id` owns this instance and all exports
            // contained within.
            let export = unsafe {
                instance
                    .as_mut()
                    .get_export_by_index_mut(registry, store_id, *entity)
            };

            let ext = Extern::from_wasmtime_export(export, &engine);
            Export::new(&module.strings[name], ext)
        })
    }

    /// Looks up an exported [`Extern`] value by name.
    ///
    /// This method will search the module for an export named `name` and return
    /// the value, if found.
    ///
    /// Returns `None` if there was no export named `name`.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    ///
    /// # Why does `get_export` take a mutable context?
    ///
    /// This method requires a mutable context because an instance's exports are
    /// lazily populated, and we cache them as they are accessed. This makes
    /// instantiating a module faster, but also means this method requires a
    /// mutable context.
    pub fn get_export(&self, mut store: impl AsContextMut, name: &str) -> Option<Extern> {
        let store = store.as_context_mut().0;
        let module = store[self.id].env_module();
        let name = module.strings.get_atom(name)?;
        let entity = *module.exports.get(&name)?;
        Some(self._get_export(store, entity))
    }

    /// Looks up an exported [`Extern`] value by a [`ModuleExport`] value.
    ///
    /// This is similar to [`Instance::get_export`] but uses a [`ModuleExport`] value to avoid
    /// string lookups where possible. [`ModuleExport`]s can be obtained by calling
    /// [`Module::get_export_index`] on the [`Module`] that this instance was instantiated with.
    ///
    /// This method will search the module for an export with a matching entity index and return
    /// the value, if found.
    ///
    /// Returns `None` if there was no export with a matching entity index.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_module_export(
        &self,
        mut store: impl AsContextMut,
        export: &ModuleExport,
    ) -> Option<Extern> {
        let store = store.as_context_mut().0;

        // Verify the `ModuleExport` matches the module used in this instance.
        // A dummy caller instance (from a direct Rust host call) has no module.
        if store.module_for_instance(self.id).map(|m| m.id()) != Some(export.module) {
            return None;
        }

        Some(self._get_export(store, export.entity))
    }

    fn _get_export(&self, store: &mut StoreOpaque, entity: EntityIndex) -> Extern {
        Extern::from_wasmtime_export(self._get_export_raw(store, entity), store.engine())
    }

    fn _get_export_raw(&self, store: &mut StoreOpaque, entity: EntityIndex) -> vm::Export {
        let id = store.id();
        // SAFETY: the store `id` owns this instance and all exports contained
        // within.
        unsafe {
            let (instance, registry) = self.id.get_mut_and_module_registry(store);
            instance.get_export_by_index_mut(registry, id, entity)
        }
    }

    /// Looks up an exported [`Func`] value by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a function.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_func(&self, store: impl AsContextMut, name: &str) -> Option<Func> {
        self.get_export(store, name)?.into_func()
    }

    /// Looks up an exported [`Func`] value by name and with its type.
    ///
    /// This function is a convenience wrapper over [`Instance::get_func`] and
    /// [`Func::typed`]. For more information see the linked documentation.
    ///
    /// Returns an error if `name` isn't a function export or if the export's
    /// type did not match `Params` or `Results`
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    ///
    /// # Errors
    ///
    /// This function will return an [`OutOfMemory`][crate::OutOfMemory] error when
    /// memory allocation fails. See the `OutOfMemory` type's documentation for
    /// details on Wasmtime's out-of-memory handling.
    pub fn get_typed_func<Params, Results>(
        &self,
        mut store: impl AsContextMut,
        name: &str,
    ) -> Result<TypedFunc<Params, Results>>
    where
        Params: crate::WasmParams,
        Results: crate::WasmResults,
    {
        let f = self
            .get_export(store.as_context_mut(), name)
            .and_then(|f| f.into_func())
            .ok_or_else(|| format_err!("failed to find function export `{name}`"))?;
        Ok(f.typed::<Params, Results>(store)
            .with_context(|| format!("failed to convert function `{name}` to given type"))?)
    }

    /// Looks up an exported [`Table`] value by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a table.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_table(&self, store: impl AsContextMut, name: &str) -> Option<Table> {
        self.get_export(store, name)?.into_table()
    }

    /// Looks up an exported [`Memory`] value by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a memory.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_memory(&self, store: impl AsContextMut, name: &str) -> Option<Memory> {
        self.get_export(store, name)?.into_memory()
    }

    /// Looks up an exported [`SharedMemory`] value by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a shared memory.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_shared_memory(
        &self,
        mut store: impl AsContextMut,
        name: &str,
    ) -> Option<SharedMemory> {
        let mut store = store.as_context_mut();
        self.get_export(&mut store, name)?.into_shared_memory()
    }

    /// Looks up an exported [`Global`] value by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a global.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_global(&self, store: impl AsContextMut, name: &str) -> Option<Global> {
        self.get_export(store, name)?.into_global()
    }

    /// Looks up a tag [`Tag`] by name.
    ///
    /// Returns `None` if there was no export named `name`, or if there was but
    /// it wasn't a tag.
    ///
    /// # Panics
    ///
    /// Panics if `store` does not own this instance.
    pub fn get_tag(&self, store: impl AsContextMut, name: &str) -> Option<Tag> {
        self.get_export(store, name)?.into_tag()
    }

    #[allow(
        dead_code,
        reason = "c-api crate does not yet support exnrefs and causes this method to be dead."
    )]
    pub(crate) fn id(&self) -> InstanceId {
        self.id.instance()
    }

    /// Return a unique-within-Store index for this `Instance`.
    ///
    /// Allows distinguishing instance identities when introspecting
    /// the `Store`, e.g. via debug APIs.
    ///
    /// This index will match the instance's position in the sequence
    /// returned by `Store::debug_all_instances()`.
    #[cfg(feature = "debug")]
    pub fn debug_index_in_store(&self) -> u32 {
        self.id.instance().as_u32()
    }

    /// Get all globals within this instance.
    ///
    /// Returns both import and defined globals.
    ///
    /// Returns both exported and non-exported globals.
    ///
    /// Gives access to the full globals space.
    #[cfg(feature = "coredump")]
    pub(crate) fn all_globals<'a>(
        &'a self,
        store: &'a mut StoreOpaque,
    ) -> impl ExactSizeIterator<Item = (GlobalIndex, Global)> + 'a {
        let store_id = store.id();
        store[self.id].all_globals(store_id)
    }

    /// Get all memories within this instance.
    ///
    /// Returns both import and defined memories.
    ///
    /// Returns both exported and non-exported memories.
    ///
    /// Gives access to the full memories space.
    #[cfg(feature = "coredump")]
    pub(crate) fn all_memories<'a>(
        &'a self,
        store: &'a StoreOpaque,
    ) -> impl ExactSizeIterator<Item = (MemoryIndex, vm::ExportMemory)> + 'a {
        let store_id = store.id();
        store[self.id].all_memories(store_id)
    }
}

pub(crate) struct OwnedImports {
    functions: TryPrimaryMap<FuncIndex, VMFunctionImport>,
    tables: TryPrimaryMap<TableIndex, VMTableImport>,
    memories: TryPrimaryMap<MemoryIndex, VMMemoryImport>,
    globals: TryPrimaryMap<GlobalIndex, VMGlobalImport>,
    tags: TryPrimaryMap<TagIndex, VMTagImport>,
}

impl OwnedImports {
    fn new(module: &Module) -> Result<OwnedImports, OutOfMemory> {
        let mut ret = OwnedImports::empty();
        ret.reserve(module)?;
        Ok(ret)
    }

    pub(crate) fn empty() -> OwnedImports {
        OwnedImports {
            functions: TryPrimaryMap::new(),
            tables: TryPrimaryMap::new(),
            memories: TryPrimaryMap::new(),
            globals: TryPrimaryMap::new(),
            tags: TryPrimaryMap::new(),
        }
    }

    pub(crate) fn reserve(&mut self, module: &Module) -> Result<(), OutOfMemory> {
        let raw = module.compiled_module().module();
        self.functions.reserve(raw.num_imported_funcs)?;
        self.tables.reserve(raw.num_imported_tables)?;
        self.memories.reserve(raw.num_imported_memories)?;
        self.globals.reserve(raw.num_imported_globals)?;
        self.tags.reserve(raw.num_imported_tags)?;
        Ok(())
    }

    #[cfg(feature = "component-model")]
    pub(crate) fn clear(&mut self) {
        self.functions.clear();
        self.tables.clear();
        self.memories.clear();
        self.globals.clear();
        self.tags.clear();
    }

    fn push(&mut self, item: &Extern, store: &mut StoreOpaque) -> Result<(), OutOfMemory> {
        match item {
            Extern::Func(i) => {
                self.functions.push(i.vmimport(store))?;
            }
            Extern::Global(i) => {
                self.globals.push(i.vmimport(store))?;
            }
            Extern::Table(i) => {
                self.tables.push(i.vmimport(store))?;
            }
            Extern::Memory(i) => {
                self.memories.push(i.vmimport(store))?;
            }
            Extern::SharedMemory(i) => {
                self.memories.push(i.vmimport(store))?;
            }
            Extern::Tag(i) => {
                self.tags.push(i.vmimport(store))?;
            }
        }
        Ok(())
    }

    /// Note that this is unsafe as the validity of `item` is not verified and
    /// it contains a bunch of raw pointers.
    pub(crate) fn push_export(
        &mut self,
        store: &StoreOpaque,
        item: &crate::runtime::vm::Export,
    ) -> Result<(), OutOfMemory> {
        match item {
            crate::runtime::vm::Export::Function(f) => {
                self.functions.push(f.vmimport(store))?;
            }
            crate::runtime::vm::Export::Global(g) => {
                self.globals.push(g.vmimport(store))?;
            }
            crate::runtime::vm::Export::Table(t) => {
                self.tables.push(t.vmimport(store))?;
            }
            crate::runtime::vm::Export::Memory(m) => {
                self.memories.push(m.vmimport(store))?;
            }
            crate::runtime::vm::Export::SharedMemory(_, vmimport) => {
                self.memories.push(*vmimport)?;
            }
            crate::runtime::vm::Export::Tag(t) => {
                self.tags.push(t.vmimport(store))?;
            }
        }
        Ok(())
    }

    pub(crate) fn as_ref(&self) -> Imports<'_> {
        Imports {
            tables: self.tables.values().as_slice(),
            globals: self.globals.values().as_slice(),
            memories: self.memories.values().as_slice(),
            functions: self.functions.values().as_slice(),
            tags: self.tags.values().as_slice(),
        }
    }
}

/// An instance, pre-instantiation, that is ready to be instantiated.
///
/// This structure represents an instance *just before* it was instantiated,
/// after all type-checking and imports have been resolved. The only thing left
/// to do for this instance is to actually run the process of instantiation.
///
/// Note that an `InstancePre` may not be tied to any particular [`Store`] if
/// none of the imports it closed over are tied to any particular [`Store`].
///
/// This structure is created through the [`Linker::instantiate_pre`] method,
/// which also has some more information and examples. Linker methods that
/// leave memory or module exports to be supplied at instantiation likewise
/// pre-check the types of all imports.
///
/// [`Store`]: crate::Store
/// [`Linker::instantiate_pre`]: crate::Linker::instantiate_pre
pub struct InstancePre<T> {
    module: Module,

    /// The items which this `InstancePre` use to instantiate the `module`
    /// provided, passed to `Instance::new_started` after inserting them into a
    /// `Store`.
    ///
    /// Note that this is stored as an `Arc` to quickly move a strong reference
    /// to everything internally into a `Store<T>` without having to clone each
    /// individual item.
    items: Arc<TryVec<Definition>>,

    late_imports: LateImports,

    /// A count of `Definition::HostFunc` entries in `items` above to
    /// preallocate space in a `Store` up front for all entries to be inserted.
    host_funcs: usize,

    /// The `VMFuncRef`s for the functions in `items` that do not
    /// have a `wasm_call` trampoline. We pre-allocate and pre-patch these
    /// `VMFuncRef`s so that we don't have to do it at
    /// instantiation time.
    ///
    /// This is an `Arc` for the same reason as `items`.
    func_refs: Arc<TryVec<VMFuncRef>>,

    /// Whether or not any import in `items` is flagged as needing async.
    ///
    /// This is used to update stores during instantiation as to whether they
    /// require async entrypoints.
    asyncness: Asyncness,

    _marker: core::marker::PhantomData<fn() -> T>,
}

/// A late binding describes where imports omitted from `InstancePre::items`
/// come from. Fixed definitions occupy all remaining positions in order.
#[derive(Clone, Default)]
pub(crate) struct LateImports {
    pub shared_memory: Option<SharedMemorySlot>,
    pub module_exports: Option<ModuleExportBindings>,
}

impl LateImports {
    fn is_empty(&self) -> bool {
        self.shared_memory.is_none() && self.module_exports.is_none()
    }
}

#[derive(Clone)]
pub(crate) struct SharedMemorySlot {
    pub position: usize,
    pub ty: MemoryType,
}

#[derive(Clone)]
pub(crate) struct ModuleExportBindings {
    pub source: Module,
    pub exports: Arc<TryVec<Option<ModuleExport>>>,
}

#[derive(Default)]
struct BoundImports<'a> {
    shared_memory: Option<(usize, &'a SharedMemory)>,
    module_exports: Option<(Instance, &'a [Option<ModuleExport>])>,
}

/// InstancePre's clone does not require T: Clone
impl<T> Clone for InstancePre<T> {
    fn clone(&self) -> Self {
        Self {
            module: self.module.clone(),
            items: self.items.clone(),
            late_imports: self.late_imports.clone(),
            host_funcs: self.host_funcs,
            func_refs: self.func_refs.clone(),
            asyncness: self.asyncness,
            _marker: self._marker,
        }
    }
}

impl<T: 'static> InstancePre<T> {
    /// Creates a new `InstancePre` which type-checks the `items` provided and
    /// on success is ready to instantiate a new instance.
    ///
    /// # Unsafety
    ///
    /// This method is unsafe as the `T` of the `InstancePre<T>` is not
    /// guaranteed to be the same as the `T` within the `Store`, the caller must
    /// verify that.
    pub(crate) unsafe fn new(
        module: &Module,
        items: TryVec<Definition>,
        late_imports: LateImports,
    ) -> Result<InstancePre<T>> {
        if late_imports.is_empty() {
            typecheck(module, &items, |cx, ty, item| cx.definition(ty, &item.ty()))?;
        } else {
            typecheck_late_imports(module, &items, &late_imports)?;
        }

        let mut func_refs = TryVec::with_capacity(items.len())?;
        let mut host_funcs = 0;
        let mut asyncness = Asyncness::No;
        for item in &items {
            match item {
                Definition::Extern(_, _) => {}
                Definition::HostFunc(f) => {
                    host_funcs += 1;
                    if f.func_ref().wasm_call.is_none() {
                        func_refs.push(VMFuncRef {
                            wasm_call: module
                                .wasm_to_array_trampoline(f.sig_index())
                                .map(|f| f.into()),
                            ..*f.func_ref()
                        })?;
                    }
                    asyncness = asyncness | f.asyncness();
                }
            }
        }

        Ok(InstancePre {
            module: module.clone(),
            items: try_new::<Arc<_>>(items)?,
            late_imports,
            host_funcs,
            func_refs: try_new::<Arc<_>>(func_refs)?,
            asyncness,
            _marker: core::marker::PhantomData,
        })
    }

    /// Returns a reference to the module that this [`InstancePre`] will be
    /// instantiating.
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Instantiates this instance, creating a new instance within the provided
    /// `store`.
    ///
    /// This function will run the actual process of instantiation to
    /// completion. This will use all of the previously-closed-over items as
    /// imports to instantiate the module that this was originally created with.
    ///
    /// For more information about instantiation see [`Instance::new`].
    ///
    /// # Panics
    ///
    /// Panics if any import closed over by this [`InstancePre`] isn't owned by
    /// `store`, or if `store` has async support enabled.
    ///
    /// # Errors
    ///
    /// The store must use the same [`Engine`] as this [`InstancePre`].
    ///
    /// This function will return an [`OutOfMemory`][crate::OutOfMemory] error when
    /// memory allocation fails. See the `OutOfMemory` type's documentation for
    /// details on Wasmtime's out-of-memory handling.
    pub fn instantiate(&self, store: impl AsContextMut<Data = T>) -> Result<Instance> {
        self.instantiate_bound(store, None, None)
    }

    /// Instantiates with the memory slot prepared by
    /// [`Linker::instantiate_pre_with_shared_memory`](crate::Linker::instantiate_pre_with_shared_memory).
    ///
    /// The memory must come from the same engine and have the exact declared
    /// memory type. Each instantiation can supply a different memory.
    pub fn instantiate_with_shared_memory(
        &self,
        store: impl AsContextMut<Data = T>,
        memory: &SharedMemory,
    ) -> Result<Instance> {
        self.instantiate_bound(store, Some(memory), None)
    }

    /// Instantiates with the exports prepared by
    /// [`Linker::instantiate_pre_with_module_exports`](crate::Linker::instantiate_pre_with_module_exports).
    ///
    /// The source instance must belong to this store and have been instantiated
    /// from the same source module used to prepare the imports. If preparation
    /// designated a shared memory import, supply the matching memory here.
    pub fn instantiate_with_module_exports(
        &self,
        store: impl AsContextMut<Data = T>,
        source: Instance,
        memory: Option<&SharedMemory>,
    ) -> Result<Instance> {
        self.instantiate_bound(store, memory, Some(source))
    }

    fn bind_shared_memory<'a>(
        &self,
        memory: Option<&'a SharedMemory>,
    ) -> Result<Option<(usize, &'a SharedMemory)>> {
        match (&self.late_imports.shared_memory, memory) {
            (Some(slot), Some(memory)) => {
                ensure!(
                    Engine::same(memory.engine(), self.module.engine()),
                    "shared memory belongs to a different engine"
                );
                ensure!(
                    memory.ty() == slot.ty,
                    "shared memory type differs from its import"
                );
                Ok(Some((slot.position, memory)))
            }
            (None, None) => Ok(None),
            (None, Some(_)) => bail!("prepared instance has no shared memory slot"),
            (Some(_), None) => bail!("prepared instance requires shared memory"),
        }
    }

    fn instantiate_bound(
        &self,
        mut store: impl AsContextMut<Data = T>,
        memory: Option<&SharedMemory>,
        source: Option<Instance>,
    ) -> Result<Instance> {
        let mut store = store.as_context_mut();
        let shared_memory = self.bind_shared_memory(memory)?;
        let module_exports = match (&self.late_imports.module_exports, source) {
            (Some(bindings), Some(source)) => {
                ensure!(
                    source.id.store_id() == store.0.id(),
                    "source instance belongs to a different store"
                );
                ensure!(
                    store
                        .0
                        .module_for_instance(source.id)
                        .is_some_and(|actual| Module::same(actual, &bindings.source)),
                    "source instance belongs to a different module"
                );
                Some((source, &bindings.exports[..]))
            }
            (None, None) => None,
            (None, Some(_)) => bail!("prepared instance has no source module exports"),
            (Some(_), None) => bail!("prepared instance requires source module exports"),
        };
        let imports = pre_instantiate_raw(
            &mut store.0,
            &self.module,
            &self.items,
            self.host_funcs,
            &self.func_refs,
            self.asyncness,
            BoundImports {
                shared_memory,
                module_exports,
            },
        )?;

        // Note that this is specifically done after `pre_instantiate_raw` to
        // handle the case that if any imports in this `InstancePre` require
        // async that it's flagged in the store by that point which will reject
        // this instantiation to say "use `instantiate_async` instead".
        store.0.validate_sync_call()?;

        // SAFETY: construction checked the fixed imports and the late import
        // declarations. The bound memory and source instance were checked
        // against those declarations above.
        vm::assert_ready(unsafe {
            Instance::new_started(&mut store, &self.module, imports.as_ref(), Asyncness::No)
        })
    }

    /// Creates a new instance, running the start function asynchronously
    /// instead of inline.
    ///
    /// For more information about asynchronous instantiation see the
    /// documentation on [`Instance::new_async`].
    ///
    /// # Panics
    ///
    /// Panics if any import closed over by this [`InstancePre`] isn't owned by
    /// `store`, or if `store` does not have async support enabled.
    ///
    /// # Errors
    ///
    /// This function will return an [`OutOfMemory`][crate::OutOfMemory] error when
    /// memory allocation fails. See the `OutOfMemory` type's documentation for
    /// details on Wasmtime's out-of-memory handling.
    #[cfg(feature = "async")]
    pub async fn instantiate_async(
        &self,
        mut store: impl AsContextMut<Data = T>,
    ) -> Result<Instance> {
        ensure!(
            self.late_imports.is_empty(),
            "prepared instance requires late imports"
        );
        let mut store = store.as_context_mut();
        let imports = pre_instantiate_raw(
            &mut store.0,
            &self.module,
            &self.items,
            self.host_funcs,
            &self.func_refs,
            self.asyncness,
            BoundImports::default(),
        )?;

        // This unsafety should be handled by the type-checking performed by the
        // constructor of `InstancePre` to assert that all the imports we're passing
        // in match the module we're instantiating.
        unsafe {
            Instance::new_started(&mut store, &self.module, imports.as_ref(), Asyncness::Yes).await
        }
    }
}

/// Helper function shared between
/// `InstancePre::{instantiate,instantiate_async}`
///
/// This is an out-of-line function to avoid the generic on `InstancePre` and
/// get this compiled into the `wasmtime` crate to avoid having it monomorphized
/// elsewhere.
fn pre_instantiate_raw(
    store: &mut StoreOpaque,
    module: &Module,
    items: &Arc<TryVec<Definition>>,
    host_funcs: usize,
    func_refs: &Arc<TryVec<VMFuncRef>>,
    asyncness: Asyncness,
    bound_imports: BoundImports<'_>,
) -> Result<OwnedImports> {
    ensure!(
        Engine::same(store.engine(), module.engine()),
        "cross-`Engine` instantiation is not currently supported"
    );
    // Register this module and use it to fill out any funcref wasm_call holes
    // we can. For more comments on this see `typecheck_externs`.
    let (modules, engine, breakpoints) = store.modules_and_engine_and_breakpoints_mut();
    modules.register_module(module, engine, breakpoints)?;
    let (funcrefs, modules) = store.func_refs_and_modules();
    funcrefs.fill(modules);

    if host_funcs > 0 {
        // Any linker-defined function of the `Definition::HostFunc` variant
        // will insert a function into the store automatically as part of
        // instantiation, so reserve space here to make insertion more efficient
        // as it won't have to realloc during the instantiation.
        funcrefs.reserve_storage(host_funcs)?;

        // The usage of `to_extern_store_rooted` requires that the items are
        // rooted via another means, which happens here by cloning the list of
        // items into the store once. This avoids cloning each individual item
        // below.
        funcrefs.push_instance_pre_definitions(items.clone())?;
        funcrefs.push_instance_pre_func_refs(func_refs.clone())?;
    }

    store.set_async_required(asyncness);

    let mut func_refs = func_refs.iter().map(|f| NonNull::from(f));
    let mut imports = OwnedImports::new(module)?;
    let import_count = bound_imports.module_exports.map_or_else(
        || items.len() + usize::from(bound_imports.shared_memory.is_some()),
        |(_, exports)| exports.len(),
    );
    let mut items = items.iter();
    for position in 0..import_count {
        if let Some((memory_position, memory)) = bound_imports.shared_memory {
            if memory_position == position {
                imports.memories.push(memory.vmimport(store))?;
                continue;
            }
        }
        if let Some((source, exports)) = bound_imports.module_exports {
            if let Some(export) = &exports[position] {
                let item = source._get_export_raw(store, export.entity);
                imports.push_export(store, &item)?;
                continue;
            }
        }

        let import = items.next().unwrap();
        if !import.comes_from_same_store(store) {
            bail!("cross-`Store` instantiation is not currently supported");
        }
        // This unsafety should be encapsulated in the constructor of
        // `InstancePre` where the `T` of the original item should match the
        // `T` of the store. Additionally the rooting necessary has happened
        // above.
        let item = match import {
            Definition::Extern(e, _) => e.clone(),
            Definition::HostFunc(func) => unsafe {
                func.to_func_store_rooted(
                    store,
                    if func.func_ref().wasm_call.is_none() {
                        Some(func_refs.next().unwrap())
                    } else {
                        None
                    },
                )
                .into()
            },
        };
        imports.push(&item, store)?;
    }

    Ok(imports)
}

fn typecheck_late_imports(
    module: &Module,
    items: &[Definition],
    late_imports: &LateImports,
) -> Result<()> {
    let imports = module.env_module().imports();
    if let Some(slot) = &late_imports.shared_memory {
        ensure!(
            slot.position < imports.len(),
            "shared memory import missing"
        );
    }
    if let Some(bindings) = &late_imports.module_exports {
        ensure!(
            Engine::same(bindings.source.engine(), module.engine()),
            "source module belongs to a different engine"
        );
        ensure!(
            imports.len() == bindings.exports.len(),
            "wrong number of import bindings"
        );
    }
    let cx = matching::MatchCx::new(module.engine());
    let mut items = items.iter();
    for (position, (namespace, name, expected)) in imports.enumerate() {
        let export = late_imports
            .module_exports
            .as_ref()
            .and_then(|bindings| Some((&bindings.source, bindings.exports[position]?)));
        if let Some(slot) = &late_imports.shared_memory {
            if slot.position == position {
                ensure!(
                    export.is_none()
                        && slot.ty.is_shared()
                        && matches!(
                            expected,
                            EntityType::Memory(memory) if &memory == slot.ty.wasmtime_memory()
                        ),
                    "incompatible shared memory import `{namespace}::{name}`"
                );
                continue;
            }
        }
        let actual = match export {
            Some((source, export)) => {
                ensure!(
                    export.module == source.id(),
                    "export belongs to a different module"
                );
                // A source instance can re-export a subtype of its declared
                // type. The declared limits are lower bounds on current
                // sizes, since memories and tables can only grow.
                match source.env_module().type_of(export.entity) {
                    EntityType::Function(ty) => DefinitionType::Func(ty.unwrap_engine_type_index()),
                    EntityType::Global(ty) => DefinitionType::Global(ty),
                    EntityType::Table(ty) => DefinitionType::Table(ty, ty.limits.min),
                    EntityType::Memory(ty) => DefinitionType::Memory(ty, ty.limits.min),
                    EntityType::Tag(ty) => DefinitionType::Tag(ty),
                }
            }
            None => items
                .next()
                .ok_or_else(|| format_err!("missing import of `{namespace}::{name}`"))?
                .ty(),
        };
        cx.definition(&expected, &actual)
            .with_context(|| format!("incompatible import type for `{namespace}::{name}`"))?;
    }
    ensure!(items.next().is_none(), "too many fixed imports");
    Ok(())
}

fn typecheck<I>(
    module: &Module,
    import_args: &[I],
    check: impl Fn(&matching::MatchCx<'_>, &EntityType, &I) -> Result<()>,
) -> Result<()> {
    let env_module = module.compiled_module().module();
    let expected_len = env_module.imports().count();
    let actual_len = import_args.len();
    if expected_len != actual_len {
        bail!("expected {expected_len} imports, found {actual_len}");
    }
    let cx = matching::MatchCx::new(module.engine());
    for ((name, field, expected_ty), actual) in env_module.imports().zip(import_args) {
        debug_assert!(expected_ty.is_canonicalized_for_runtime_usage());
        check(&cx, &expected_ty, actual)
            .with_context(|| format!("incompatible import type for `{name}::{field}`"))?;
    }
    Ok(())
}

#[cfg(all(test, feature = "cranelift", feature = "threads", feature = "wat"))]
mod late_import_tests {
    use crate::prelude::ToString;
    use crate::{
        Caller, Config, Engine, Instance, Linker, MemoryType, Module, Result, SharedMemory, Store,
    };

    const RUNTIME: &str = r#"
        (module
            (import "host" "memory" (memory 1 2 shared))
            (import "host" "bias" (func $bias (result i32)))
            (func (export "write") (param i32)
                i32.const 0
                local.get 0
                call $bias
                i32.add
                i32.store)
            (func (export "read") (result i32)
                i32.const 0
                i32.load))
    "#;

    const PROGRAM: &str = r#"
        (module
            (import "runtime" "write" (func $write (param i32)))
            (import "host" "memory" (memory 1 2 shared))
            (import "host" "bias" (func $bias (result i32)))
            (func (export "write") (param i32)
                local.get 0
                call $bias
                i32.add
                call $write)
            (func (export "read") (result i32)
                i32.const 0
                i32.load))
    "#;

    fn engine() -> Result<Engine> {
        let mut config = Config::new();
        config.wasm_threads(true).shared_memory(true);
        Engine::new(&config)
    }

    fn linker(engine: &Engine) -> Result<Linker<i32>> {
        let mut linker = Linker::new(engine);
        linker.func_wrap("host", "bias", |caller: Caller<'_, i32>| *caller.data())?;
        Ok(linker)
    }

    #[test]
    fn late_shared_memory_requires_exact_type_and_engine() -> Result<()> {
        let engine = engine()?;
        let runtime = Module::new(&engine, RUNTIME)?;
        let pre =
            linker(&engine)?.instantiate_pre_with_shared_memory(&runtime, "host", "memory")?;
        let mut store = Store::new(&engine, 0);

        // The smaller maximum normally satisfies Wasm's memory subtyping
        // rule. Rebinding requires the exact type checked during preparation.
        let smaller = SharedMemory::new(&engine, MemoryType::shared(1, 1))?;
        let error = pre
            .instantiate_with_shared_memory(&mut store, &smaller)
            .unwrap_err();
        assert!(error.to_string().contains("shared memory type differs"));

        let other_engine = self::engine()?;
        let foreign = SharedMemory::new(&other_engine, MemoryType::shared(1, 2))?;
        let error = pre
            .instantiate_with_shared_memory(&mut store, &foreign)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("shared memory belongs to a different engine")
        );
        assert!(pre.instantiate(&mut store).is_err());

        let memory = SharedMemory::new(&engine, MemoryType::shared(1, 2))?;
        pre.instantiate_with_shared_memory(&mut store, &memory)?;
        Ok(())
    }

    #[test]
    fn late_imports_use_live_exports_and_isolated_shared_memory() -> Result<()> {
        let engine = engine()?;
        let runtime = Module::new(&engine, RUNTIME)?;
        let program = Module::new(&engine, PROGRAM)?;
        let linker = linker(&engine)?;
        let runtime_pre = linker.instantiate_pre_with_shared_memory(&runtime, "host", "memory")?;
        let slot = Some(("host", "memory"));
        assert!(
            linker
                .instantiate_pre_with_module_exports(
                    &program,
                    &runtime,
                    &[runtime.get_export_index("read"), None, None],
                    slot,
                )
                .is_err()
        );
        let program_pre = linker.instantiate_pre_with_module_exports(
            &program,
            &runtime,
            &[runtime.get_export_index("write"), None, None],
            slot,
        )?;
        drop(linker);

        let first_memory = SharedMemory::new(&engine, MemoryType::shared(1, 2))?;
        let second_memory = SharedMemory::new(&engine, MemoryType::shared(1, 2))?;
        let mut first_store = Store::new(&engine, 1);
        let mut second_store = Store::new(&engine, 10);
        let first_runtime =
            runtime_pre.instantiate_with_shared_memory(&mut first_store, &first_memory)?;
        let second_runtime =
            runtime_pre.instantiate_with_shared_memory(&mut second_store, &second_memory)?;
        assert!(
            program_pre
                .instantiate_with_module_exports(
                    &mut second_store,
                    first_runtime,
                    Some(&second_memory)
                )
                .is_err()
        );
        assert!(
            program_pre
                .instantiate_with_module_exports(&mut second_store, second_runtime, None)
                .is_err()
        );
        let other_module = Module::new(&engine, "(module)")?;
        let other_instance = Instance::new(&mut second_store, &other_module, &[])?;
        assert!(
            program_pre
                .instantiate_with_module_exports(
                    &mut second_store,
                    other_instance,
                    Some(&second_memory)
                )
                .is_err()
        );

        let first = program_pre.instantiate_with_module_exports(
            &mut first_store,
            first_runtime,
            Some(&first_memory),
        )?;
        let second = program_pre.instantiate_with_module_exports(
            &mut second_store,
            second_runtime,
            Some(&second_memory),
        )?;
        let first_read = first.get_typed_func::<(), i32>(&mut first_store, "read")?;
        let second_read = second.get_typed_func::<(), i32>(&mut second_store, "read")?;
        first
            .get_typed_func::<i32, ()>(&mut first_store, "write")?
            .call(&mut first_store, 3)?;
        assert_eq!(first_read.call(&mut first_store, ())?, 5);
        assert_eq!(second_read.call(&mut second_store, ())?, 0);
        second
            .get_typed_func::<i32, ()>(&mut second_store, "write")?
            .call(&mut second_store, 3)?;
        assert_eq!(second_read.call(&mut second_store, ())?, 23);
        assert_eq!(first_read.call(&mut first_store, ())?, 5);
        Ok(())
    }
}

#[cfg(all(test, feature = "cranelift", feature = "wat"))]
mod func_ref_text_tests {
    use crate::code::ModuleWithCode;
    use crate::{AsContextMut, Engine, Instance, Module, Store};
    use wasmtime_environ::FuncKey;

    /// Three escaping functions (exported, in a table, named by `ref.func`),
    /// non-escaping ones around them and an imported one before them.
    const WAT: &str = r#"
        (module
          (import "host" "f" (func $imported))
          (table 4 funcref)
          (elem (i32.const 0) $in_table $exported)
          (elem declare func $referenced)
          (func $private (result i32) (i32.const 1))
          (func $exported (export "exported") (result i32) (call $private))
          (func $in_table (param i32) (result i32) (local.get 0))
          (func $referenced (export "referenced") (result funcref) (ref.func $referenced))
          (func $start (call $imported))
          (start $start)
        )
    "#;

    /// The array-call and Wasm-call addresses of every escaping defined
    /// function of `instance`, as its `VMFuncRef`s hold them and as the
    /// function table and the store's registry give them.
    fn check(store: &mut Store<()>, module: &Module, instance: Instance) -> usize {
        let (mut vm, registry) = instance
            .id
            .get_mut_and_module_registry(store.as_context_mut().0);
        let compiled = module.compiled_module();
        let env = module.env_module();
        let code = ModuleWithCode::in_store(registry, module).unwrap();
        let mut escaping = 0;
        for defined in env.defined_func_indices() {
            let index = env.func_index(defined);
            if !env.functions[index].is_escaping() {
                continue;
            }
            escaping += 1;
            let array = code.function(FuncKey::ArrayToWasmTrampoline(env.module_index, defined));
            let wasm = code.function(FuncKey::DefinedWasmFunction(env.module_index, defined));
            // First request, then the instance's initialized entry.
            for _ in 0..2 {
                let func_ref = vm.as_mut().get_func_ref(registry, index).unwrap();
                // SAFETY: the instance's own initialized `VMFuncRef`.
                let func_ref = unsafe { func_ref.as_ref() };
                assert_eq!(
                    func_ref.array_call.as_ptr().cast::<u8>().cast_const(),
                    array.as_ptr()
                );
                let wasm_call = func_ref.wasm_call.unwrap();
                assert_eq!(wasm_call.as_ptr().cast::<u8>().cast_const(), wasm.as_ptr());
            }
            // The module's cache, filled by now, and the table agree.
            assert_eq!(
                compiled.func_ref_text(defined),
                compiled.locate_func_ref_text(defined)
            );
        }
        escaping
    }

    #[test]
    fn func_refs_match_the_function_table_in_every_store_and_instance() {
        let engine = Engine::default();
        let module = Module::new(&engine, WAT).unwrap();
        let compiled = module.compiled_module();
        for defined in module.env_module().defined_func_indices() {
            let index = module.env_module().func_index(defined);
            if module.env_module().functions[index].is_escaping() {
                // Before any instance: located, then cached.
                let located = compiled.locate_func_ref_text(defined);
                assert_ne!(located.array_call, located.wasm_call);
                assert_eq!(compiled.func_ref_text(defined), located);
                assert_eq!(compiled.func_ref_text(defined), located);
            }
        }
        // A second module of the same engine has its own cache.
        let other = Module::new(&engine, WAT).unwrap();
        for _ in 0..2 {
            let mut store = Store::new(&engine, ());
            let host = crate::Func::wrap(&mut store, || {});
            for module in [&module, &other, &module] {
                let instance = Instance::new(&mut store, module, &[host.into()]).unwrap();
                assert_eq!(check(&mut store, module, instance), 3);
                let exported = instance
                    .get_typed_func::<(), i32>(&mut store, "exported")
                    .unwrap();
                assert_eq!(exported.call(&mut store, ()).unwrap(), 1);
            }
        }
    }
}
