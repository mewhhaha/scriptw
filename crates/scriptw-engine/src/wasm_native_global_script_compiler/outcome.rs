use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout, GcHostValue};
use scriptw_front::ParseDiagnosticKind;
use scriptw_ir::RuntimeGlobalScriptCompileError;
use wasmtime::{AnyRef, ArrayRef, ArrayRefPre, AsContextMut, OwnedRooted, StructRef, StructRefPre};

#[derive(Debug)]
pub(super) enum CompileRejection {
    Syntax(Vec<u16>),
}
impl CompileRejection {
    pub(super) fn classify(error: RuntimeGlobalScriptCompileError) -> wasmtime::Result<Self> {
        let message = match error {
            RuntimeGlobalScriptCompileError::Parse(error) => match error.diagnostic().kind() {
                ParseDiagnosticKind::MalformedJavaScript => error.to_string(),
                ParseDiagnosticKind::UnsupportedParserFeature => {
                    return Err(wasmtime::Error::new(EngineError::from_parse_error(error)));
                }
            },
            // Only the real parser mints the syntax-message branch here.
            RuntimeGlobalScriptCompileError::Lowering(diagnostics) => {
                return Err(wasmtime::Error::new(match diagnostics.into_iter().next() {
                    Some(diagnostic) => EngineError::from_ir_diagnostic(diagnostic),
                    None => EngineError::new(
                        "native global Script lowerer rejected without a diagnostic",
                    ),
                }));
            }
            RuntimeGlobalScriptCompileError::InvalidCompilerOutput(message) => {
                return Err(abi_error(message));
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

/// Owned roots preserve the actual output across the temporary RootScope exit.
pub(super) enum EscapedReply {
    Entered {
        tag: i32,
        scalar: i64,
        reference: Option<OwnedRooted<AnyRef>>,
        kind: i32,
        target: i32,
    },
    Syntax(OwnedRooted<AnyRef>),
}
impl EscapedReply {
    pub(super) fn entered(
        store: &mut impl AsContextMut<Data = WasmHostState>,
        output: [Val; 6],
    ) -> wasmtime::Result<Self> {
        let [Val::I32(tag), Val::I64(scalar), Val::AnyRef(reference), Val::I32(kind), Val::I32(target), Val::I32(2)] =
            output
        else {
            return Err(abi_error(
                "factory did not return actual Entered/Completion words",
            ));
        };
        scriptw_aot_wasm::check_gc_main_completion(kind, target).map_err(abi_error)?;
        GcHostValue::check(tag, scalar, reference.is_some()).map_err(abi_error)?;
        let reference = reference
            .map(|r| r.to_owned_rooted(&mut *store))
            .transpose()?;
        Ok(Self::Entered {
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
        signature: &NativeGlobalScriptSignature,
    ) -> wasmtime::Result<Self> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(message.len())
            .map_err(wasmtime::Error::new)?;
        values.extend(message.iter().map(|u| Val::I32(i32::from(*u))));
        let initialized_fields = [
            GcHostField::StringCodeUnits,
            GcHostField::StringRawKeyHash,
            GcHostField::StringEntryHint,
        ];
        if GcHostLayout::String.field_count() != Some(initialized_fields.len() as u32) {
            return Err(abi_error(
                "syntax String constructor lacks field initializers",
            ));
        }
        let pre = ArrayRefPre::new(&mut *store, signature.units.clone());
        let units = ArrayRef::new_fixed(&mut *store, &pre, &values)?;
        let mut fields = vec![Val::I32(0); signature.string.fields().len()];
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
        let [tag, scalar, reference, kind, target, status, syntax] = outputs else {
            return Err(abi_error("global compiler requires seven result slots"));
        };
        match self {
            Self::Entered {
                tag: t,
                scalar: s,
                reference: r,
                kind: k,
                target: p,
            } => {
                *tag = Val::I32(t);
                *scalar = Val::I64(s);
                *reference = Val::AnyRef(r.as_ref().map(|r| r.to_rooted(&mut *store)));
                *kind = Val::I32(k);
                *target = Val::I32(p);
                *status = Val::I32(2);
                *syntax = Val::AnyRef(None);
            }
            Self::Syntax(message) => {
                *tag = Val::I32(WasmRuntimeValueTag::Undefined.tag());
                *scalar = Val::I64(0);
                *reference = Val::AnyRef(None);
                *kind = Val::I32(CompletionKindIr::Normal.abi_code() as i32);
                *target = Val::I32(0);
                *status = Val::I32(1);
                *syntax = Val::AnyRef(Some(message.to_rooted(&mut *store)));
            }
        }
        Ok(())
    }
}
