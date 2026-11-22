//! Scriptw patch tests (see PATCHES.md): `uextend.i64` of an `i32` value whose
//! lowering already wrote a 32-bit result emits no `movl` of its own.

use crate::Context;
use crate::cursor::{Cursor, FuncCursor};
use crate::ir::condcodes::IntCC;
use crate::ir::{AbiParam, Function, InstBuilder, MemFlagsData, Signature, Value, types};
use crate::isa::{CallConv, lookup};
use crate::settings::{self, Configurable, Flags};
use alloc::string::String;
use alloc::vec::Vec;
use target_lexicon::triple;

/// Builds `fn(p: i64, base: i64, a: i32, b: i32) -> i64`. `make_index` makes
/// the `i32` value that is zero-extended and added to `base`; that sum is the
/// address of the `i32` load whose zero-extension is returned. The function
/// also stores the index to `p`, so that a load producing it has a second use
/// and cannot be sunk into the extension.
fn build(make_index: impl FnOnce(&mut FuncCursor, [Value; 4]) -> Value) -> Function {
    let mut sig = Signature::new(CallConv::SystemV);
    for ty in [types::I64, types::I64, types::I32, types::I32] {
        sig.params.push(AbiParam::new(ty));
    }
    sig.returns.push(AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(Default::default(), sig);
    let block = func.dfg.make_block();
    let params: Vec<Value> = [types::I64, types::I64, types::I32, types::I32]
        .into_iter()
        .map(|ty| func.dfg.append_block_param(block, ty))
        .collect();
    let mut pos = FuncCursor::new(&mut func);
    pos.insert_block(block);
    let [p, base, a, b] = [params[0], params[1], params[2], params[3]];
    let index = make_index(&mut pos, [p, base, a, b]);
    pos.ins().store(MemFlagsData::trusted(), index, p, 8);
    let wide = pos.ins().uextend(types::I64, index);
    let address = pos.ins().iadd(base, wide);
    let loaded = pos.ins().load(types::I32, MemFlagsData::trusted(), address, 4);
    let result = pos.ins().uextend(types::I64, loaded);
    pos.ins().return_(&[result]);
    func
}

/// The VCode text of `func` compiled for x86-64 with the given `opt_level`.
fn vcode(func: Function, opt_level: &str) -> String {
    let mut flags = settings::builder();
    flags.set("opt_level", opt_level).unwrap();
    let isa = lookup(triple!("x86_64"))
        .expect("x86-64 backend")
        .finish(Flags::new(flags))
        .expect("x86-64 ISA");
    let mut context = Context::for_function(func);
    context.set_disasm(true);
    context
        .compile(&*isa, &mut Default::default())
        .expect("compiles")
        .vcode
        .clone()
        .expect("VCode text was requested")
}

/// The register-to-register 32-bit moves in `vcode`, which here can only be
/// zero-extensions.
fn movl_register_copies(vcode: &str) -> Vec<&str> {
    vcode
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.strip_prefix("movl %")
                .is_some_and(|operands| !operands.contains('(') && !operands.contains('$'))
        })
        .collect()
}

fn assert_no_extension(name: &str, make_index: impl FnOnce(&mut FuncCursor, [Value; 4]) -> Value) {
    let func = build(make_index);
    for opt_level in ["none", "speed"] {
        let text = vcode(func.clone(), opt_level);
        assert_eq!(
            movl_register_copies(&text),
            Vec::<&str>::new(),
            "{name} (opt_level={opt_level}):\n{text}"
        );
    }
}

#[test]
fn i32_load_is_not_zero_extended_again() {
    assert_no_extension("load", |pos, [p, ..]| {
        pos.ins().load(types::I32, MemFlagsData::trusted(), p, 0)
    });
}

#[test]
fn i32_sshr_is_not_zero_extended_again() {
    assert_no_extension("sshr by a register", |pos, [_, _, a, b]| {
        pos.ins().sshr(a, b)
    });
    assert_no_extension("sshr by an immediate", |pos, [_, _, a, _]| {
        pos.ins().sshr_imm_u(a, 3)
    });
}

#[test]
fn i32_select_is_not_zero_extended_again() {
    // The comparison is not between the selected values, which the egraph
    // would turn into `smin`.
    assert_no_extension("select on a comparison", |pos, [p, base, a, b]| {
        let condition = pos.ins().icmp(IntCC::SignedLessThan, p, base);
        pos.ins().select(condition, a, b)
    });
    // `select` on two conditions (`fcmp eq`) uses two 32-bit `cmov`s.
    assert_no_extension("select on an ordered equality", |pos, [_, _, a, b]| {
        let x = pos.ins().fcvt_from_sint(types::F64, a);
        let y = pos.ins().fcvt_from_sint(types::F64, b);
        let condition = pos.ins().fcmp(crate::ir::condcodes::FloatCC::Equal, x, y);
        pos.ins().select(condition, a, b)
    });
}

/// An `i32` argument may hold anything in its upper half, so its
/// zero-extension stays.
#[test]
fn i32_argument_is_still_zero_extended() {
    let func = build(|_, [_, _, a, _]| a);
    for opt_level in ["none", "speed"] {
        let text = vcode(func.clone(), opt_level);
        assert_eq!(
            movl_register_copies(&text).len(),
            1,
            "opt_level={opt_level}:\n{text}"
        );
    }
}
