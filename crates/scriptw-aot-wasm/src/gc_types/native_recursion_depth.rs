//! Checked ownership of the finite depth of nested runtime-helper recursion.
//!
//! Source bodies have their own bound ([`super::SourceActivationDepth`]), but a
//! runtime helper can recurse without entering any source body: a Proxy whose
//! target is a Proxy forwards each internal method to the next one, and a
//! built-in can call a built-in (`Array.prototype.join` converting a nested
//! Array, a Proxy handler whose traps are `Reflect` functions). Such chains are
//! as deep as the program's data, so a guard on each recursive edge turns
//! exhaustion of the Wasm stack, which would trap and end the program, into a
//! catchable RangeError (an implementation resource limit).
//!
//! A guarded edge tests the shared scalar once on entry and throws the
//! RangeError when the bound is reached. Inside it, every call is a counted
//! call of the code sink (`Function::open_counted_calls`): the scalar is
//! raised just before the call and lowered just after it returns, so the
//! count stays balanced on every exit of the surrounding code (branches,
//! returns, completions routed out of a builtin), and a trap ends the
//! Store's program.

use super::*;

/// Simultaneously executing counted calls. One level of the deepest guarded
/// cycles measured here (a Proxy forwarding to a Proxy, JSON.stringify of a
/// nested Array, Array.prototype.join converting a nested Array) costs well
/// under 4 KiB of Wasm stack: 30,000 levels of each ran in the product's
/// 32 MiB Wasm stack before this bound existed, and one join level is two
/// counted calls. 4,096 keeps the guarded chain under half that stack,
/// beside the source bodies' own bound.
pub(crate) const MAX_NESTED_NATIVE_RECURSION: i32 = 4096;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct NativeRecursionDepthGlobal {
    index: u32,
}

impl NativeRecursionDepthGlobal {
    pub(super) fn declare(globals: &mut GlobalLedger) -> Self {
        let index = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
        Self { index }
    }
}

impl RuntimeSchema {
    /// Leaves 1 when one more guarded recursion would exceed the bound.
    pub(crate) fn emit_native_recursion_exhausted_i32(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.native_recursion_depth.index));
        function.instruction(&Instruction::I32Const(MAX_NESTED_NATIVE_RECURSION));
        function.instruction(&Instruction::I32GeU);
    }

    /// Emits `body` with every call it makes counted as one guarded
    /// recursion. The caller has already tested
    /// [`Self::emit_native_recursion_exhausted_i32`] and routed the
    /// exhausted case.
    pub(crate) fn emit_native_recursion_counted<E>(
        &self,
        function: &mut Function,
        body: impl FnOnce(&mut Function) -> Result<(), E>,
    ) -> Result<(), E> {
        let scope = function.open_counted_calls(self.native_recursion_depth.index);
        body(function)?;
        function.close_counted_calls(scope);
        Ok(())
    }
}
