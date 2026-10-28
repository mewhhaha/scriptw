//! The one Wasm function index space.
//!
//! ```text
//! [common host imports][runtime callables/helpers][P foreign imports][main?][P initializer?][program callables/scalar entries/helpers]
//!           ^-- runtime half: identical for every program --^
//!                                                 ^-- program half starts with its actual entry prefix
//! ```
//!
//! Runtime callables are the compiler-owned source bodies, every standard
//! builtin and every host builtin. Program callables are the script's own
//! functions followed by its prepared script units. Validated scalar callback
//! entries follow those callables and precede all program helpers, which are the
//! helper bodies generated from the script (module operations and the runtime
//! hooks, see `HelperOwner`). Every index is derived
//! Native factory modules have an initializer prefix; ordinary programs also
//! have main. Every index is derived here, so the runtime/program boundary is [`Self::first_program_index`]
//! and no other module does position arithmetic.

use scriptw_ir::{FunctionId, FunctionIr, ScriptIr};
use std::collections::BTreeMap;

use crate::runtime_artifact::ModuleKind;
use crate::runtime_helpers::RuntimeHelperFunctionBase;

/// A declared native import is never a JavaScript callable entry. Only the
/// complete P index plan can mint it; wrappers spend it as a direct Wasm call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForeignImportFunctionIndex(u32);

impl ForeignImportFunctionIndex {
    pub(crate) fn emit_call(self, function: &mut crate::code_sink::Function) {
        function.instruction(&wasm_encoder::Instruction::Call(self.0));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleEntryRole {
    RuntimeOnly,
    Main,
    NativeFactory,
}

impl ModuleEntryRole {
    pub(crate) const fn for_module(kind: ModuleKind<'_>) -> Self {
        match kind {
            ModuleKind::Runtime(_) => Self::RuntimeOnly,
            ModuleKind::Standalone | ModuleKind::Program(_) => Self::Main,
        }
    }
}

/// Only the linked-P layout can mint a callable initializer index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramInitializeIndex(u32);
impl ProgramInitializeIndex {
    pub(crate) const fn raw(self) -> u32 {
        self.0
    }
    pub(crate) fn assert_body_index(self, actual: u32) {
        assert_eq!(
            self.0, actual,
            "P initializer emitted outside its planned index"
        );
    }
}

/// The script's source functions split at the runtime/program boundary. Each
/// list keeps the script's own order.
pub(crate) struct SourceFunctions<'a> {
    pub(crate) runtime_owned: Vec<&'a FunctionIr>,
    pub(crate) program: Vec<&'a FunctionIr>,
    numeric_leaves: BTreeMap<FunctionId, crate::functions::NumericLeafPlan>,
    binary_numeric: Vec<BinaryNumericSource<'a>>,
}

struct BinaryNumericSource<'a> {
    function: &'a FunctionIr,
    proof: crate::functions::BinaryNumericLeafPlan,
}

impl<'a> SourceFunctions<'a> {
    pub(crate) fn partition(script: &'a ScriptIr) -> Self {
        let (runtime_owned, program): (Vec<_>, Vec<_>) = script
            .functions
            .iter()
            .partition(|function| crate::builtins::is_empty_dynamic_function_body(function));
        let numeric_leaves: BTreeMap<_, _> = script
            .functions
            .iter()
            .filter_map(|function| {
                crate::functions::NumericLeafPlan::for_function(function)
                    .map(|proof| (function.id.clone(), proof))
            })
            .collect();
        let binary_numeric = program
            .iter()
            .filter_map(|function| {
                numeric_leaves
                    .get(&function.id)?
                    .binary_callback()
                    .map(|proof| BinaryNumericSource { function, proof })
            })
            .collect();
        Self {
            runtime_owned,
            program,
            numeric_leaves,
            binary_numeric,
        }
    }

    pub(crate) fn numeric_leaf(
        &self,
        function: &FunctionIr,
    ) -> Option<&crate::functions::NumericLeafPlan> {
        self.numeric_leaves.get(&function.id)
    }

    pub(crate) fn binary_numeric_count(&self) -> usize {
        self.binary_numeric.len()
    }

    pub(crate) fn binary_numeric_entry(
        &self,
        function: &FunctionIr,
        layout: &FunctionIndexLayout,
    ) -> Option<crate::function_entry::PlannedBinaryNumericEntry> {
        self.binary_numeric
            .iter()
            .enumerate()
            .find_map(|(position, source)| {
                (source.function.id == function.id).then(|| {
                    crate::function_entry::PlannedBinaryNumericEntry::new(
                        source.function,
                        source.proof.clone(),
                        layout.binary_numeric_index(position),
                    )
                })
            })
    }
}

/// Only the complete P index plan can name an auxiliary scalar body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BinaryNumericFunctionIndex(u32);
impl BinaryNumericFunctionIndex {
    pub(crate) fn emit_ref_func(self, function: &mut crate::code_sink::Function) {
        function.instruction(&wasm_encoder::Instruction::RefFunc(self.0));
    }
    pub(crate) fn assert_body_index(self, actual: u32) {
        assert_eq!(self.0, actual, "binary numeric body uses its planned index");
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FunctionIndexLayout {
    imported: u32,
    runtime_owned: u32,
    standard: u32,
    host: u32,
    runtime_helpers: u32,
    foreign_imports: u32,
    entry_role: ModuleEntryRole,
    program_initialize: bool,
    program_callables: u32,
    program_binary_numeric: u32,
    program_helpers: u32,
}

fn count(value: usize) -> u32 {
    u32::try_from(value).expect("function count fits Wasm index domain")
}

impl FunctionIndexLayout {
    pub(crate) fn new(
        imported: u32,
        runtime_owned: usize,
        standard: usize,
        host: usize,
        runtime_helpers: usize,
        program_callables: usize,
        program_binary_numeric: usize,
        program_helpers: usize,
        kind: ModuleKind<'_>,
        entry_role: ModuleEntryRole,
    ) -> Self {
        assert!(
            matches!(
                (kind, entry_role),
                (ModuleKind::Runtime(_), ModuleEntryRole::RuntimeOnly)
                    | (ModuleKind::Standalone, ModuleEntryRole::Main)
                    | (
                        ModuleKind::Program(_),
                        ModuleEntryRole::Main | ModuleEntryRole::NativeFactory
                    )
            ),
            "entry role belongs to the actual module kind"
        );
        let layout = Self {
            imported,
            runtime_owned: count(runtime_owned),
            standard: count(standard),
            host: count(host),
            runtime_helpers: count(runtime_helpers),
            foreign_imports: 0,
            entry_role,
            program_initialize: matches!(kind, ModuleKind::Program(_)),
            program_callables: count(program_callables),
            program_binary_numeric: count(program_binary_numeric),
            program_helpers: count(program_helpers),
        };
        // Fails here, once, if the whole space does not fit.
        layout.program_helper_end();
        layout
    }

    /// P's own imports follow the complete, unchanged H/R function prefix.
    /// R and heap-free standalone modules cannot acquire application imports.
    pub(crate) fn with_foreign_imports(
        mut self,
        imports: usize,
        kind: ModuleKind<'_>,
    ) -> Result<Self, crate::EmitError> {
        if imports != 0 && !matches!(kind, ModuleKind::Program(_)) {
            return Err(crate::EmitError::unsupported(
                "foreign functions require a linked program module",
            ));
        }
        let imports = u32::try_from(imports).map_err(|_| {
            crate::EmitError::unsupported("foreign function count exceeds the Wasm index space")
        })?;
        (self.program_helper_end() - self.foreign_imports)
            .checked_add(imports)
            .ok_or_else(|| {
                crate::EmitError::unsupported(
                    "foreign imports exceed the complete P function index space",
                )
            })?;
        self.foreign_imports = imports;
        self.program_helper_end();
        Ok(self)
    }

    fn add(base: u32, offset: u32) -> u32 {
        base.checked_add(offset)
            .expect("function index fits Wasm index domain")
    }

    pub(crate) fn runtime_owned_index(&self, position: usize) -> u32 {
        debug_assert!(position < self.runtime_owned as usize);
        Self::add(self.imported, count(position))
    }

    pub(crate) fn standard_index(&self, position: usize) -> u32 {
        debug_assert!(position < self.standard as usize);
        Self::add(
            Self::add(self.imported, self.runtime_owned),
            count(position),
        )
    }

    pub(crate) fn host_index(&self, position: usize) -> u32 {
        debug_assert!(position < self.host as usize);
        Self::add(
            Self::add(Self::add(self.imported, self.runtime_owned), self.standard),
            count(position),
        )
    }

    fn runtime_helper_first(&self) -> u32 {
        Self::add(
            Self::add(Self::add(self.imported, self.runtime_owned), self.standard),
            self.host,
        )
    }

    pub(crate) fn helper_base(&self) -> RuntimeHelperFunctionBase {
        RuntimeHelperFunctionBase::at(
            self.runtime_helper_first(),
            self.program_binary_numeric_end(),
        )
    }

    /// End of the common H/R prefix, identical regardless of P capabilities.
    fn foreign_import_first(&self) -> u32 {
        Self::add(self.runtime_helper_first(), self.runtime_helpers)
    }

    pub(crate) fn foreign_import_index(
        &self,
        id: scriptw_ir::foreign::ForeignFunctionId,
    ) -> Result<ForeignImportFunctionIndex, crate::EmitError> {
        let position = id.index();
        if position >= self.foreign_imports {
            return Err(crate::EmitError::unsupported(
                "foreign call references an undeclared program import",
            ));
        }
        Ok(ForeignImportFunctionIndex(Self::add(
            self.foreign_import_first(),
            position,
        )))
    }

    pub(crate) const fn foreign_import_count(&self) -> u32 {
        self.foreign_imports
    }

    /// First defined P function, after all common and foreign imports.
    pub(crate) fn first_program_index(&self) -> u32 {
        Self::add(self.foreign_import_first(), self.foreign_imports)
    }

    pub(crate) const fn entry_role(&self) -> ModuleEntryRole {
        self.entry_role
    }

    pub(crate) fn main_index(&self) -> Option<u32> {
        matches!(self.entry_role, ModuleEntryRole::Main).then(|| self.first_program_index())
    }

    pub(crate) fn program_initialize_index(&self) -> Option<ProgramInitializeIndex> {
        self.program_initialize.then(|| {
            ProgramInitializeIndex(Self::add(
                self.first_program_index(),
                u32::from(self.main_index().is_some()),
            ))
        })
    }

    /// Defined P prefix bodies, before source/prepared callables.
    pub(crate) const fn program_prefix_count(&self) -> usize {
        matches!(self.entry_role, ModuleEntryRole::Main) as usize + self.program_initialize as usize
    }

    fn program_callable_first(&self) -> u32 {
        Self::add(
            self.first_program_index(),
            self.program_prefix_count() as u32,
        )
    }

    pub(crate) fn program_callable_index(&self, position: usize) -> u32 {
        debug_assert!(position < self.program_callables as usize);
        Self::add(self.program_callable_first(), count(position))
    }

    fn program_callable_end(&self) -> u32 {
        Self::add(self.program_callable_first(), self.program_callables)
    }

    pub(crate) fn binary_numeric_index(&self, position: usize) -> BinaryNumericFunctionIndex {
        assert!(position < self.program_binary_numeric as usize);
        BinaryNumericFunctionIndex(Self::add(self.program_callable_end(), count(position)))
    }

    fn program_binary_numeric_end(&self) -> u32 {
        Self::add(self.program_callable_end(), self.program_binary_numeric)
    }

    fn program_helper_end(&self) -> u32 {
        Self::add(self.program_binary_numeric_end(), self.program_helpers)
    }

    /// Every defined function: what `ref.func` may name, for the declarative
    /// element segment.
    pub(crate) fn defined_functions(&self) -> std::ops::Range<u32> {
        self.imported..self.program_helper_end()
    }
}
