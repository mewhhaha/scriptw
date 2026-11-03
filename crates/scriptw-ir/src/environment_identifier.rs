//! Located ResolveBinding lifecycles; private continuation References never become JS values.
use crate::{
    ArithmeticBinaryOp, BitwiseBinaryOp, LogicalBinaryOp, NumericUpdateOp, Strictness, TypedExpr,
    UpdateReturnMode,
};

/// One compiler-private activation cell. Only lowering can allocate the slot;
/// its GC field retains a Reference separately from the JavaScript value field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedIdentifierReferenceIr {
    binding: Box<TypedExpr>,
}

impl CapturedIdentifierReferenceIr {
    pub(crate) fn new(name: String) -> Self {
        Self {
            binding: Box::new(TypedExpr::from_info(
                crate::ValueInfo::undefined(),
                crate::ExprIr::Identifier(name),
            )),
        }
    }
    pub fn binding(&self) -> &TypedExpr {
        &self.binding
    }
    pub fn storage_name(&self) -> &str {
        let crate::ExprIr::Identifier(name) = &self.binding.expr else {
            unreachable!("private Reference allocation owns an identifier")
        };
        name
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdentifierReferenceFallbackIr {
    binding: Option<Box<TypedExpr>>,
}

#[derive(Debug, Clone, Copy)]
pub enum IdentifierReferenceFallbackDisposition<'a> {
    Declarative { binding: &'a TypedExpr },
    Global,
}

impl IdentifierReferenceFallbackIr {
    pub(crate) fn declarative(storage_name: String) -> Self {
        Self {
            binding: Some(Box::new(TypedExpr::from_info(
                crate::ValueInfo {
                    kind: crate::ValueKind::Dynamic,
                    possible_kinds: crate::KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: crate::FunctionTargetKnowledge::unknown(),
                },
                crate::ExprIr::Identifier(storage_name),
            ))),
        }
    }
    pub(crate) fn global() -> Self {
        Self { binding: None }
    }
    fn disposition(&self) -> IdentifierReferenceFallbackDisposition<'_> {
        match &self.binding {
            Some(binding) => IdentifierReferenceFallbackDisposition::Declarative { binding },
            None => IdentifierReferenceFallbackDisposition::Global,
        }
    }
    fn binding(&self) -> Option<&TypedExpr> {
        self.binding.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IdentifierReferenceCaptureBaseIr {
    RuntimeEnvironment,
    Located(IdentifierReferenceFallbackIr),
    WithObject {
        selection: Box<TypedExpr>,
        fallback: IdentifierReferenceFallbackIr,
    },
}

/// Plain assignment locates its Reference before the RHS but never reads it.
/// Compound and logical assignment additionally acquire the old whole value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierReferenceCaptureAccess {
    ReadBeforeRhs,
    WriteOnly,
}

/// Ordered selection is evaluated exactly once by the capture operation. A
/// declarative fallback names the actual existing cell, not a second binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierReferenceCaptureIr {
    reference: CapturedIdentifierReferenceIr,
    base: IdentifierReferenceCaptureBaseIr,
    access: IdentifierReferenceCaptureAccess,
}

#[derive(Debug, Clone, Copy)]
pub enum IdentifierReferenceCaptureDisposition<'a> {
    RuntimeEnvironment,
    Located(IdentifierReferenceFallbackDisposition<'a>),
    WithObject {
        selection: &'a TypedExpr,
        fallback: IdentifierReferenceFallbackDisposition<'a>,
    },
}

impl IdentifierReferenceCaptureIr {
    pub(crate) fn runtime(
        reference: CapturedIdentifierReferenceIr,
        access: IdentifierReferenceCaptureAccess,
    ) -> Self {
        Self {
            reference,
            base: IdentifierReferenceCaptureBaseIr::RuntimeEnvironment,
            access,
        }
    }
    pub(crate) fn located(
        reference: CapturedIdentifierReferenceIr,
        fallback: IdentifierReferenceFallbackIr,
        access: IdentifierReferenceCaptureAccess,
    ) -> Self {
        Self {
            reference,
            base: IdentifierReferenceCaptureBaseIr::Located(fallback),
            access,
        }
    }
    pub(crate) fn with_object(
        reference: CapturedIdentifierReferenceIr,
        selection: TypedExpr,
        fallback: IdentifierReferenceFallbackIr,
        access: IdentifierReferenceCaptureAccess,
    ) -> Self {
        Self {
            reference,
            base: IdentifierReferenceCaptureBaseIr::WithObject {
                selection: Box::new(selection),
                fallback,
            },
            access,
        }
    }
    pub fn reference(&self) -> &CapturedIdentifierReferenceIr {
        &self.reference
    }
    pub fn access(&self) -> IdentifierReferenceCaptureAccess {
        self.access
    }
    pub fn disposition(&self) -> IdentifierReferenceCaptureDisposition<'_> {
        match &self.base {
            IdentifierReferenceCaptureBaseIr::RuntimeEnvironment => {
                IdentifierReferenceCaptureDisposition::RuntimeEnvironment
            }
            IdentifierReferenceCaptureBaseIr::Located(fallback) => {
                IdentifierReferenceCaptureDisposition::Located(fallback.disposition())
            }
            IdentifierReferenceCaptureBaseIr::WithObject {
                selection,
                fallback,
            } => IdentifierReferenceCaptureDisposition::WithObject {
                selection,
                fallback: fallback.disposition(),
            },
        }
    }
    pub fn operands(&self) -> impl Iterator<Item = &TypedExpr> {
        let (selection, binding) = match &self.base {
            IdentifierReferenceCaptureBaseIr::RuntimeEnvironment => (None, None),
            IdentifierReferenceCaptureBaseIr::Located(fallback) => (None, fallback.binding()),
            IdentifierReferenceCaptureBaseIr::WithObject {
                selection,
                fallback,
            } => (Some(selection.as_ref()), fallback.binding()),
        };
        std::iter::once(self.reference.binding())
            .chain(selection)
            .chain(binding)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentIdentifierIr {
    pub name: String,
    pub strictness: Strictness,
    pub operation: EnvironmentIdentifierOperationIr,
    resolution_start: EnvironmentIdentifierResolutionStart,
}

/// Where a fresh ResolveBinding starts. Captured operations already own their
/// Reference and do not resolve again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentIdentifierResolutionStart {
    CurrentEnvironment,
    GlobalEnvironment,
    Cell(crate::ProvenBindingCellIr),
}

impl EnvironmentIdentifierIr {
    pub(crate) fn cell(
        name: String,
        strictness: Strictness,
        operation: EnvironmentIdentifierOperationIr,
        cell: crate::ProvenBindingCellIr,
    ) -> Self {
        Self {
            name,
            strictness,
            operation,
            resolution_start: EnvironmentIdentifierResolutionStart::Cell(cell),
        }
    }

    pub(crate) fn current(
        name: String,
        strictness: Strictness,
        operation: EnvironmentIdentifierOperationIr,
    ) -> Self {
        Self {
            name,
            strictness,
            operation,
            resolution_start: EnvironmentIdentifierResolutionStart::CurrentEnvironment,
        }
    }

    pub(crate) fn global(
        name: String,
        strictness: Strictness,
        operation: EnvironmentIdentifierOperationIr,
    ) -> Self {
        Self {
            name,
            strictness,
            operation,
            resolution_start: EnvironmentIdentifierResolutionStart::GlobalEnvironment,
        }
    }

    pub fn resolution_start(&self) -> EnvironmentIdentifierResolutionStart {
        self.resolution_start.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentIdentifierOperationIr {
    Read,
    Typeof,
    CaptureAssignmentReference {
        capture: IdentifierReferenceCaptureIr,
    },
    PutCapturedReference {
        reference: CapturedIdentifierReferenceIr,
        value: Box<TypedExpr>,
    },
    ReleaseCapturedReference {
        reference: CapturedIdentifierReferenceIr,
    },
    CaptureCallReference {
        receiver: crate::CapturedCallReceiverIr,
    },
    Assign {
        value: Box<TypedExpr>,
    },
    /// Ordered Object Environment selection and its global fallback are one
    /// ResolveBinding, completed before the single write-only RHS.
    AssignWithGlobalFallback {
        selection: Box<TypedExpr>,
        value: Box<TypedExpr>,
    },
    Delete,
    Update {
        operation: NumericUpdateOp,
        return_mode: UpdateReturnMode,
    },
    EagerCompound {
        operation: EnvironmentCompoundOperationIr,
        rhs: Box<TypedExpr>,
    },
    LogicalCompound {
        operation: LogicalBinaryOp,
        rhs: Box<TypedExpr>,
    },
    Call {
        args: Vec<TypedExpr>,
        direct_eval: Option<crate::DirectEvalContextIr>,
        /// Speculative original callees for guarded code selection only. This
        /// says nothing about the live Reference's value or the Call result.
        callee_targets: crate::FunctionTargetKnowledge,
    },
}

impl EnvironmentIdentifierOperationIr {
    pub fn operands(&self) -> impl Iterator<Item = &TypedExpr> {
        let reference = match self {
            Self::PutCapturedReference { reference, .. } => Some(reference.binding()),
            Self::Read
            | Self::Typeof
            | Self::CaptureAssignmentReference { .. }
            | Self::ReleaseCapturedReference { .. }
            | Self::CaptureCallReference { .. }
            | Self::Assign { .. }
            | Self::AssignWithGlobalFallback { .. }
            | Self::Delete
            | Self::Update { .. }
            | Self::EagerCompound { .. }
            | Self::LogicalCompound { .. }
            | Self::Call { .. } => None,
        };
        let (operands, captured): (&[TypedExpr], Option<&IdentifierReferenceCaptureIr>) = match self
        {
            Self::Read | Self::Typeof | Self::Delete | Self::Update { .. } => (&[], None),
            Self::CaptureAssignmentReference { capture } => (&[], Some(capture)),
            Self::PutCapturedReference { value, .. } => (std::slice::from_ref(value), None),
            Self::ReleaseCapturedReference { reference } => {
                (std::slice::from_ref(reference.binding()), None)
            }
            Self::CaptureCallReference { receiver } => {
                (std::slice::from_ref(receiver.binding()), None)
            }
            Self::Assign { value } | Self::AssignWithGlobalFallback { value, .. } => {
                (std::slice::from_ref(value), None)
            }
            Self::EagerCompound { rhs, .. } | Self::LogicalCompound { rhs, .. } => {
                (std::slice::from_ref(rhs), None)
            }
            Self::Call { args, .. } => (args, None),
        };
        let selection = if let Self::AssignWithGlobalFallback { selection, .. } = self {
            Some(selection.as_ref())
        } else {
            None
        };
        reference
            .into_iter()
            .chain(selection)
            .chain(operands.iter())
            .chain(
                captured
                    .into_iter()
                    .flat_map(IdentifierReferenceCaptureIr::operands),
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentCompoundOperationIr {
    Add,
    Arithmetic(ArithmeticBinaryOp),
    Bitwise(BitwiseBinaryOp),
}
