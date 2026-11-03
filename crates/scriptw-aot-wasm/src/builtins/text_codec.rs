//! Encoding Web APIs compile into the ordinary Wasm GC object model.
use super::super::*;
use super::uint8array_codecs::Uint8ArrayCodecAccess;
use crate::functions::OrdinaryDefaultPrototype;
use crate::gc_types::*;

mod decoder;
mod encoder;

impl FunctionBuilder<'_> {
    fn emit_text_codec_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_uint8_codec_type_error_if(message, output, exit, f)
    }

    fn emit_text_codec_option(
        &mut self,
        options: &ValueLocals,
        name: &str,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.compile_nullish_tagged_i32(options.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        pending.value().set_undefined(f);
        pending.set_kind(CompletionKind::Normal, f);
        f.instruction(&Instruction::Else);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(name, f)?, f);
        let key = crate::objects::PropertyKeyLocals::from_string(s, &text, f);
        self.emit_object_read(options, options, &key, pending, f)?;
        key.clear(f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
}
mod buffer_source;
