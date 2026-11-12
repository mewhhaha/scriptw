use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout, GcHostValue};
use scriptw_front::ParseDiagnosticKind;
use scriptw_ir::{NativeErrorKind, RuntimeFunctionCompileError};
use wasmtime::{AnyRef, ArrayRef, ArrayRefPre, AsContextMut, OwnedRooted, StructRef, StructRefPre};

#[derive(Debug)]
pub(super) enum CompileRejection {
    Syntax(Vec<u16>),
}

impl CompileRejection {
    pub(super) fn classify(error: RuntimeFunctionCompileError) -> wasmtime::Result<Self> {
        let message = match error {
            RuntimeFunctionCompileError::Parse(error) => match error.diagnostic().kind() {
                ParseDiagnosticKind::MalformedJavaScript => error.to_string(),
                ParseDiagnosticKind::UnsupportedParserFeature => {
                    return Err(wasmtime::Error::new(EngineError::from_parse_error(error)));
                }
            },
            RuntimeFunctionCompileError::Lowering(diagnostics) => {
                if let Some(syntax) = diagnostics.iter().find(|diagnostic| {
                    diagnostic.code().is_some()
                        && diagnostic.error_type() == Some(NativeErrorKind::SyntaxError)
                }) {
                    syntax.message.clone()
                } else {
                    let error = match diagnostics.into_iter().next() {
                        Some(diagnostic) => EngineError::from_ir_diagnostic(diagnostic),
                        None => EngineError::new(
                            "native Function lowerer rejected without a diagnostic",
                        ),
                    };
                    return Err(wasmtime::Error::new(error));
                }
            }
            RuntimeFunctionCompileError::InvalidCompilerOutput(message) => {
                return Err(wasmtime::Error::new(EngineError::new(format!(
                    "native Function compiler invariant failed: {message}"
                ))));
            }
        };
        let mut units = Vec::new();
        units
            .try_reserve_exact(message.encode_utf16().count())
            .map_err(wasmtime::Error::new)?;
        units.extend(message.encode_utf16());
        Ok(Self::Syntax(units))
    }
}

/// Owned roots bridge the private temporary scope to Caller result roots.
/// No temporary source/initializer references are retained in Store state.
pub(super) enum EscapedReply {
    Completion {
        tag: i32,
        scalar: i64,
        reference: Option<OwnedRooted<AnyRef>>,
        kind: i32,
        target: i32,
    },
    Syntax(OwnedRooted<AnyRef>),
}

impl EscapedReply {
    pub(super) fn completion(
        store: &mut impl AsContextMut<Data = WasmHostState>,
        completion: [Val; 5],
    ) -> wasmtime::Result<Self> {
        let [Val::I32(tag), Val::I64(scalar), Val::AnyRef(reference), Val::I32(kind), Val::I32(target)] =
            completion
        else {
            return Err(abi_error("factory returned malformed Completion words"));
        };
        GcHostValue::check(tag, scalar, reference.is_some()).map_err(abi_error)?;
        let reference = reference
            .map(|reference| reference.to_owned_rooted(&mut *store))
            .transpose()?;
        Ok(Self::Completion {
            tag,
            scalar,
            reference,
            kind,
            target,
        })
    }

    pub(super) fn syntax(
        store: &mut impl AsContextMut<Data = WasmHostState>,
        message: &[u16],
        signature: &NativeFunctionSignature,
    ) -> wasmtime::Result<Self> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(message.len())
            .map_err(wasmtime::Error::new)?;
        values.extend(message.iter().map(|unit| Val::I32(i32::from(*unit))));
        let units_pre = ArrayRefPre::new(&mut *store, signature.units.clone());
        let units = ArrayRef::new_fixed(&mut *store, &units_pre, &values)?;
        let initialized_fields = [
            GcHostField::StringCodeUnits,
            GcHostField::StringRawKeyHash,
            GcHostField::StringEntryHint,
        ];
        let count = signature.string.fields().len();
        if GcHostLayout::String.field_count() != Some(initialized_fields.len() as u32) {
            return Err(abi_error(
                "host String constructor lacks a declared field initializer",
            ));
        }
        let mut fields = vec![Val::I32(0); count];
        fields[GcHostField::StringCodeUnits.definition().index() as usize] = Val::from(units);
        fields[GcHostField::StringRawKeyHash.definition().index() as usize] = Val::I64(0);
        // No dictionary position is remembered for a fresh String.
        fields[GcHostField::StringEntryHint.definition().index() as usize] = Val::I32(-1);
        let pre = StructRefPre::new(&mut *store, signature.string.clone());
        let string = StructRef::new(&mut *store, &pre, &fields)?;
        Ok(Self::Syntax(
            string.to_anyref().to_owned_rooted(&mut *store)?,
        ))
    }

    pub(super) fn write(
        self,
        store: &mut impl AsContextMut<Data = WasmHostState>,
        outputs: &mut [Val],
    ) -> wasmtime::Result<()> {
        let [tag, scalar, reference, kind, target, syntax] = outputs else {
            return Err(abi_error(
                "native Function compiler requires six result slots",
            ));
        };
        match self {
            Self::Completion {
                tag: value_tag,
                scalar: value_scalar,
                reference: value_ref,
                kind: completion_kind,
                target: completion_target,
            } => {
                *tag = Val::I32(value_tag);
                *scalar = Val::I64(value_scalar);
                *reference =
                    Val::AnyRef(value_ref.as_ref().map(|value| value.to_rooted(&mut *store)));
                *kind = Val::I32(completion_kind);
                *target = Val::I32(completion_target);
                *syntax = Val::AnyRef(None);
            }
            Self::Syntax(message) => {
                *tag = Val::I32(WasmRuntimeValueTag::Undefined.tag());
                *scalar = Val::I64(0);
                *reference = Val::AnyRef(None);
                *kind = Val::I32(CompletionKindIr::Normal.abi_code() as i32);
                *target = Val::I32(0);
                *syntax = Val::AnyRef(Some(message.to_rooted(&mut *store)));
            }
        }
        Ok(())
    }
}
