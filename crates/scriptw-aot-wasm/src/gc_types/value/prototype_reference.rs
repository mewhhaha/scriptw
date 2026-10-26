//! [[Prototype]] stores the actual object payload or null, without value boxing.
use super::*;

impl RuntimeSchema {
    /// Validate the object-or-null boundary once before publishing its typed edge.
    pub(crate) fn prototype_reference(
        &self,
        value: &ValueLocals,
        f: &mut Function,
    ) -> GcStackReference<JavaScriptObject, Nullable> {
        f.instruction(&Instruction::I32Const(0));
        for tag in [
            WasmRuntimeValueTag::Null,
            WasmRuntimeValueTag::Object,
            WasmRuntimeValueTag::Array,
            WasmRuntimeValueTag::Function,
            WasmRuntimeValueTag::Arguments,
        ] {
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(tag as i32));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32Or);
        }
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Eq);
        value.reference().load(f);
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        value.reference().load(f);
        f.instruction(&Instruction::RefCastNullable(
            self.reference_type::<JavaScriptObject>(GcNullability::Nullable)
                .heap_type,
        ));
        GcStackReference::new()
    }

    /// All tags derive from the same closed schema as object header projection.
    pub(crate) fn prototype_reference_into(
        &self,
        reference: &GcLocal<JavaScriptObject, Nullable>,
        out: &ValueLocals,
        f: &mut Function,
    ) {
        out.set_scalar(ScalarValue::Null, f);
        reference.load(self, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object as i32));
        out.tag().store(f);
        reference.load(self, f);
        f.instruction(&Instruction::LocalSet(out.reference.index));
        for layout in GcLayout::ALL {
            if layout.object_projection().is_none() {
                continue;
            }
            let tag = match layout.javascript_tag() {
                Some(WasmRuntimeValueTag::Object) => continue,
                Some(
                    tag @ (WasmRuntimeValueTag::Array
                    | WasmRuntimeValueTag::Function
                    | WasmRuntimeValueTag::Arguments),
                ) => tag,
                Some(
                    WasmRuntimeValueTag::Undefined
                    | WasmRuntimeValueTag::Null
                    | WasmRuntimeValueTag::Boolean
                    | WasmRuntimeValueTag::Number
                    | WasmRuntimeValueTag::String
                    | WasmRuntimeValueTag::BigInt
                    | WasmRuntimeValueTag::Symbol,
                )
                | None => unreachable!("object schema must declare an object ABI tag"),
            };
            reference.load(self, f);
            f.instruction(&Instruction::RefTestNonNull(
                self.types
                    .layouts
                    .reference(*layout, GcNullability::NonNullable)
                    .heap_type,
            ));
            f.instruction(&Instruction::If(BlockType::Empty));
            f.instruction(&Instruction::I32Const(tag as i32));
            out.tag().store(f);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);
    }
}
