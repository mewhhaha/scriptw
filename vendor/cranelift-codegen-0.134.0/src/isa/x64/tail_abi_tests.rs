use crate::Context;
use crate::cursor::{Cursor, FuncCursor};
use crate::ir::{
    AbiParam, ArgumentPurpose, Function, GlobalValueData, InstBuilder, MemFlagsData, Signature,
    StackSlotData, StackSlotKind, types,
};
use crate::isa::{CallConv, lookup};
use crate::settings::{self, Configurable, Flags};
use alloc::vec::Vec;
use target_lexicon::triple;

#[test]
fn indirect_tail_call_keeps_eight_integer_arguments_through_stack_checks_and_frame_shrink() {
    let mut flags = settings::builder();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_probestack", "true").unwrap();
    flags.set("probestack_strategy", "outline").unwrap();
    let isa = lookup(triple!("x86_64"))
        .unwrap()
        .finish(Flags::new(flags))
        .unwrap();

    let mut signature = Signature::new(CallConv::Tail);
    signature
        .params
        .push(AbiParam::special(types::I64, ArgumentPurpose::VMContext));
    signature
        .params
        .extend((0..7).map(|_| AbiParam::new(types::I64)));
    let callee_signature = signature.clone();
    // Take the target pointer in a stack argument so the tail call drops
    // one aligned argument area even though it forwards all eight registers.
    signature.params.push(AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(Default::default(), signature);
    let callee_signature = func.import_signature(callee_signature);
    let vmctx = func.create_global_value(GlobalValueData::VMContext);
    let mem_flags = func.dfg.mem_flags.insert(MemFlagsData::trusted()).unwrap();
    func.stack_limit = Some(func.create_global_value(GlobalValueData::Load {
        base: vmctx,
        offset: 24.into(),
        global_type: types::I64,
        flags: mem_flags,
    }));

    // Force the stack probe loop so it must use an actual scratch register.
    let slot =
        func.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 5 * 4096, 0));
    let entry = func.dfg.make_block();
    let params: Vec<_> = (0..9)
        .map(|_| func.dfg.append_block_param(entry, types::I64))
        .collect();
    let mut cursor = FuncCursor::new(&mut func);
    cursor.insert_block(entry);
    cursor.ins().stack_store(types::I64, params[7], slot, 0);
    cursor
        .ins()
        .return_call_indirect(callee_signature, params[8], &params[..8]);

    let mut context = Context::for_function(func);
    context.set_disasm(true);
    let code = context.compile(&*isa, &mut Default::default()).unwrap();
    let vcode = code.vcode.as_deref().unwrap();
    let prologue = vcode.split_once("block0:").unwrap().0;
    assert!(prologue.contains("0x18(%rdi), %r11"), "{vcode}");
    assert!(
        !prologue.contains("%rax") && !prologue.contains("%r10"),
        "{vcode}"
    );
    assert!(
        prologue
            .lines()
            .any(|line| line.contains("stack_probe_loop") && line.contains("%r11")),
        "{vcode}"
    );
    assert!(code.buffer.relocs().is_empty(), "{vcode}");
    let call = vcode
        .lines()
        .find(|line| line.contains("return_call_unknown"))
        .unwrap();
    assert!(call.contains("return_call_unknown %r11 (0)"), "{vcode}");
    assert!(call.contains("=%rax") && call.contains("=%r10"), "{vcode}");

    let mut call_sites = code.buffer.call_sites();
    let tail_site = call_sites.next().unwrap();
    assert!(call_sites.next().is_none());
    assert!(tail_site.exception_handlers.is_empty());
    assert!(code.buffer.user_stack_maps().is_empty());
    // The call-site offset follows popq 8(%rsp), addq $8,%rsp, jmpq *%r11.
    // The three instructions move the return PC by 16 bytes without a GPR.
    assert!(
        code.code_buffer()[..tail_site.ret_addr as usize].ends_with(&[
            0x8f, 0x44, 0x24, 0x08, 0x48, 0x83, 0xc4, 0x08, 0x41, 0xff, 0xe3,
        ])
    );
}
