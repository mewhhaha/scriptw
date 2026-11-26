//! Loads of what a function keeps in its frame, once per call.
//!
//! Two kinds of stack slot hold a value that the function reads far more
//! often than the value can change:
//!
//! * **Stack-map slots.** `cranelift-frontend` gives every GC reference that
//!   is live across a call a slot: it stores the value there before a live
//!   safepoint, lists the slot in the stack map of each call the value is live
//!   across, keeps the original SSA value before the first call in its block,
//!   and reloads once per safepoint-separated segment inside each block. The
//!   collector moves objects, so a reference has to be read again after a
//!   call. It does not have to be read again across blocks or loops without a
//!   call.
//! * **Home slots** (`base_home`): the GC heap base and the `vmctx`, each
//!   stored once by the entry block and loaded at every use.
//!
//! Cranelift's own redundant-load elimination stops at every instruction
//! that can trap, and compiled JavaScript has a null check or a cast every
//! few instructions.
//!
//! [`Reloads::forward`] removes the loads that no call makes necessary. It
//! follows, through the whole function, what each such slot is known to
//! hold:
//!
//! * a store to the slot makes the stored value the slot's content;
//! * a load from the slot whose content is known yields that value and is
//!   removed; any other load stays, and its result is the content from there;
//! * a call ends what is known about every slot, except that a slot holding
//!   the null reference still holds it;
//! * where control-flow paths meet, a slot's content is known when every
//!   path brings the same value;
//! * the head of a loop takes the content as a parameter when the loop
//!   reads the slot before it calls and a path back to the head knows the
//!   content (see [`Reloads::hoist`]): the slot is then read on the way into
//!   the loop instead of in every iteration.
//!
//! A load that stays addresses its slot through a `stack_addr` of its own
//! (see [`unshare_address`]). A home that no load reads any more loses its
//! store and its slot.
//!
//! A call also ends what is known about a home, although a home never
//! changes: a value that stays known across a call is a value that is live
//! across it, which the register allocator then spills and reloads at every
//! use. Nor does a block other than a loop's head take a parameter where
//! paths bring different values, as SSA construction would give it: the
//! moves into those parameters cost more than the loads they save. That was
//! measured twice: on the GC heap base, which was first placed that way,
//! with a call every few blocks and so a parameter at nearly every join; and
//! for all slots of this pass with parameters only where every path onward
//! reads the slot before it calls, and a load only on the edges that do not
//! know its content. At a loop's head the load saved is one per iteration.
//!
//! # Why this is correct
//!
//! The pass handles only slots that are *plain*: every `stack_addr` of the
//! slot is used as nothing but the address of a `load` or `store` of the
//! whole slot, all of one type. Such a slot's bytes change only
//!
//! 1. at one of those stores, which the pass sees, and
//! 2. while a call runs, when the collector rewrites a slot that the call's
//!    stack map lists. It writes a relocated reference over a reference; it
//!    never changes a null one, so a slot that holds zero when a call starts
//!    holds zero when it returns.
//!
//! Nothing else can: the address is not available to any other instruction,
//! and code outside the function reaches the frame only through a call. A
//! trap or an exception leaves the function, or continues it at a `try_call`
//! successor, and `try_call` is a call.
//!
//! The pass computes, for the start of each reachable block, the slots whose
//! content is known, as a solution of the usual forward equations: the
//! entry block starts knowing nothing, any other block with the facts that
//! the ends of all its reachable predecessors agree on and the parameters
//! [`Reloads::hoist`] gave it, whose arguments are what the slot holds at
//! the end of each predecessor, and a block's end follows from its start by
//! the rules above. Take any such solution and a
//! load of slot `s` where it says `s` holds `v`, and follow any path of the
//! function from its entry to that load backwards. The fact "`s` holds `v`"
//! is in the solution at each point on the way until the instruction that
//! made it: not the entry, which knows nothing, not a call, which leaves no
//! value known, and not a store of another value, so a store of `v` to `s`,
//! a load of `s` whose result is `v`, or the start of a loop whose
//! parameter for `s` is `v`, with no call and no other store to `s` after it
//! on the path. The slot therefore still holds what that
//! instruction stored or read: the load yields `v`. Every path to the load
//! passes such an instruction, which `v`'s definition dominates, so `v`'s
//! definition dominates the load and using `v` is valid SSA. No call lies
//! between that instruction and the load, so `v` is not a reference that a
//! collection could have made stale, and no stack map needs to list it. For
//! a slot known to hold null the same argument reaches a store of a zero
//! constant with no other store after it, calls allowed.
//!
//! The stores to stack-map slots and the stack maps are untouched, so the
//! collector sees what it saw before. A home's store is removed only when
//! no load of the home is left in the function, reachable or not. If the
//! equations are not solved within [`MAX_ROUNDS`] rounds, or their answer
//! would hold more than [`MAX_KNOWN_PER_BLOCK`] entries per block on average
//! over the function's reachable blocks, the function is left as it was:
//! its loads all stay.

use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::dominator_tree::DominatorTree;
use cranelift_codegen::flowgraph::ControlFlowGraph;
use cranelift_codegen::ir::{self, InstBuilder};
use cranelift_entity::SecondaryMap;

/// Rounds over the function's blocks after which the pass gives up. A
/// function without loops needs two (the second confirms the first); each
/// level of loop nesting can add one.
const MAX_ROUNDS: usize = 12;

/// Scriptw: the most entries the answer may hold, on average per reachable
/// block of the function (the budget is this times their number, shared by
/// all blocks), before the pass gives up. What is known at a block's end can
/// grow with the
/// path to it, so deeply nested code (2,000 nested `else if` branches) held
/// a known set per block proportional to its depth, memory quadratic in the
/// function's size (half of a 1 GB native compilation of 500 such branches).
const MAX_KNOWN_PER_BLOCK: usize = 64;

/// What a slot is known to hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Content {
    /// The null reference: a zero that no collection changes.
    Null,
    /// This value, as long as no call has run.
    Value(ir::Value),
}

/// The slots whose content is known at one point, ordered by the number the
/// pass gives each handled slot.
type Known = Vec<(u32, Content)>;

/// What an instruction does to a handled slot.
#[derive(Clone, Copy)]
enum Event {
    Load {
        slot: u32,
        inst: ir::Inst,
    },
    Store {
        slot: u32,
        content: Content,
        inst: ir::Inst,
    },
    Call,
    /// The block's parameter `value` holds the slot's content (see
    /// [`Reloads::hoist`]).
    Enter {
        slot: u32,
        value: ir::Value,
    },
}

fn set(known: &mut Known, slot: u32, content: Content) {
    match known.binary_search_by_key(&slot, |entry| entry.0) {
        Ok(index) => known[index].1 = content,
        Err(index) => known.insert(index, (slot, content)),
    }
}

fn get(known: &Known, slot: u32) -> Option<Content> {
    known
        .binary_search_by_key(&slot, |entry| entry.0)
        .ok()
        .map(|index| known[index].1)
}

/// Keep in `known` what `other` agrees with.
fn meet(known: &mut Known, other: &Known) {
    let mut theirs = other.iter().peekable();
    known.retain(|entry| {
        while theirs.next_if(|other| other.0 < entry.0).is_some() {}
        theirs.peek().is_some_and(|other| *other == entry)
    });
}

/// The pass's view of one function.
pub(crate) struct Reloads {
    /// The events of all blocks, each block's in layout order.
    events: Vec<Event>,
    /// Each block's range of `events`.
    block_events: SecondaryMap<ir::Block, (u32, u32)>,
    /// The home slots among the handled slots, with their numbers.
    homes: Vec<(u32, ir::StackSlot)>,
    /// How each handled slot is read, by its number.
    accesses: Vec<Option<Access>>,
    /// The parameters that `hoist` gave loop headers, as events at their
    /// start.
    entered: SecondaryMap<ir::Block, Vec<Event>>,
    /// The loads that `hoist` put at the end of blocks.
    appended: SecondaryMap<ir::Block, Vec<Event>>,
}

/// How the function loads a handled slot.
#[derive(Clone, Copy)]
struct Access {
    slot: ir::StackSlot,
    /// The type of every value the slot holds.
    ty: ir::Type,
    flags: ir::MemFlags,
    pointer_type: ir::Type,
}

impl Reloads {
    /// Find the stack-map slots of `func` and what its instructions do to
    /// them and to `homes`, the function's home slots (see the module
    /// documentation). `None` when the function has no slot to handle.
    pub(crate) fn scan(func: &ir::Function, homes: &[ir::StackSlot]) -> Option<Self> {
        // The slots that stack maps list, and each `stack_addr`.
        let mut numbers: SecondaryMap<ir::StackSlot, u32> = SecondaryMap::with_default(NONE);
        let mut plain = Vec::new();
        for &home in homes {
            numbers[home] = plain.len() as u32;
            plain.push(true);
        }
        let mut addresses: SecondaryMap<ir::Value, Option<(ir::StackSlot, i32)>> =
            SecondaryMap::new();
        for block in func.layout.blocks() {
            for inst in func.layout.block_insts(block) {
                match func.dfg.insts[inst] {
                    ir::InstructionData::StackAddr {
                        opcode: ir::Opcode::StackAddr,
                        stack_slot,
                        offset,
                    } => {
                        let address = func.dfg.first_result(inst);
                        addresses[address] = Some((stack_slot, offset.into()));
                    }
                    ref data if data.opcode().is_call() => {
                        for entry in func.dfg.user_stack_map_entries(inst).unwrap_or(&[]) {
                            if numbers[entry.slot] == NONE {
                                numbers[entry.slot] = plain.len() as u32;
                                plain.push(true);
                            }
                            let whole = entry.offset == 0
                                && entry.ty.bytes() == func.sized_stack_slots[entry.slot].size;
                            plain[numbers[entry.slot] as usize] &= whole;
                        }
                    }
                    _ => {}
                }
            }
        }
        if plain.is_empty() {
            return None;
        }

        // The slot `value` addresses, when the pass handles that slot.
        let address_of = |value: ir::Value| {
            let value = func.dfg.resolve_aliases(value);
            let (slot, offset) = addresses[value]?;
            (numbers[slot] != NONE).then_some((slot, numbers[slot], offset))
        };
        let whole_slot = |slot: ir::StackSlot, ty: ir::Type, address: i32, access: i32| {
            address.checked_add(access) == Some(0)
                && ty.bytes() == func.sized_stack_slots[slot].size
        };
        // A plain slot holds values of one type.
        let mut types: Vec<Option<ir::Type>> = vec![None; plain.len()];
        let mut one_type = |number: u32, ty: ir::Type| {
            let known = types[number as usize].get_or_insert(ty);
            *known == ty
        };
        let mut accesses: Vec<Option<Access>> = vec![None; plain.len()];

        let mut events = Vec::new();
        let mut block_events: SecondaryMap<ir::Block, (u32, u32)> = SecondaryMap::new();
        for block in func.layout.blocks() {
            let start = events.len() as u32;
            for inst in func.layout.block_insts(block) {
                match func.dfg.insts[inst] {
                    ir::InstructionData::Load {
                        opcode: ir::Opcode::Load,
                        arg,
                        offset,
                        flags,
                    } => {
                        if let Some((slot, number, address)) = address_of(arg) {
                            let ty = func.dfg.value_type(func.dfg.first_result(inst));
                            if whole_slot(slot, ty, address, offset.into()) && one_type(number, ty)
                            {
                                events.push(Event::Load { slot: number, inst });
                                accesses[number as usize].get_or_insert(Access {
                                    slot,
                                    ty,
                                    flags,
                                    pointer_type: func
                                        .dfg
                                        .value_type(func.dfg.resolve_aliases(arg)),
                                });
                            } else {
                                plain[number as usize] = false;
                            }
                        }
                        continue;
                    }
                    ir::InstructionData::Store {
                        opcode: ir::Opcode::Store,
                        args: [value, address],
                        offset,
                        ..
                    } => {
                        if let Some((slot, number, address)) = address_of(address) {
                            let stored = func.dfg.resolve_aliases(value);
                            let ty = func.dfg.value_type(stored);
                            if whole_slot(slot, ty, address, offset.into()) && one_type(number, ty)
                            {
                                let content = if is_zero(func, stored) {
                                    Content::Null
                                } else {
                                    Content::Value(stored)
                                };
                                events.push(Event::Store {
                                    slot: number,
                                    content,
                                    inst,
                                });
                            } else {
                                plain[number as usize] = false;
                            }
                        }
                        // Storing a slot's address somewhere lets it escape.
                        if let Some((_, number, _)) = address_of(value) {
                            plain[number as usize] = false;
                        }
                        continue;
                    }
                    ref data => {
                        if data.opcode().is_call() {
                            events.push(Event::Call);
                        }
                        // An instruction that names a slot other than to take
                        // its address accesses it in a way this pass does not
                        // follow.
                        if data.opcode() != ir::Opcode::StackAddr {
                            if let Some(slot) = data.stack_slot() {
                                if numbers[slot] != NONE {
                                    plain[numbers[slot] as usize] = false;
                                }
                            }
                        }
                    }
                }
                // Any other instruction that takes a slot's address lets it
                // escape.
                super::super::operands::for_each(&func.dfg, inst, |value| {
                    if let Some((_, number, _)) = address_of(value) {
                        plain[number as usize] = false;
                    }
                });
            }
            block_events[block] = (start, events.len() as u32);
        }

        // Forget the slots that are not plain.
        if plain.iter().any(|plain| !plain) {
            let mut kept = Vec::with_capacity(events.len());
            for block in func.layout.blocks() {
                let (start, end) = block_events[block];
                let first = kept.len() as u32;
                kept.extend(
                    events[start as usize..end as usize].iter().copied().filter(
                        |event| match *event {
                            Event::Load { slot, .. } | Event::Store { slot, .. } => {
                                plain[slot as usize]
                            }
                            Event::Call | Event::Enter { .. } => true,
                        },
                    ),
                );
                block_events[block] = (first, kept.len() as u32);
            }
            events = kept;
        }
        if !events
            .iter()
            .any(|event| matches!(event, Event::Load { .. }))
        {
            return None;
        }
        let homes = homes
            .iter()
            .map(|&home| (numbers[home], home))
            .filter(|&(number, _)| plain[number as usize])
            .collect();
        Some(Self {
            events,
            block_events,
            homes,
            accesses,
            entered: SecondaryMap::new(),
            appended: SecondaryMap::new(),
        })
    }

    /// The events of `block` in the order they happen.
    fn events(&self, block: ir::Block) -> impl DoubleEndedIterator<Item = Event> + '_ {
        let (start, end) = self.block_events[block];
        self.entered[block]
            .iter()
            .chain(&self.events[start as usize..end as usize])
            .chain(&self.appended[block])
            .copied()
    }

    /// What is known at the start of `block`, from what is known at the end
    /// of its predecessors that `ends` has an answer for.
    fn at_start(
        &self,
        cfg: &ControlFlowGraph,
        ends: &SecondaryMap<ir::Block, Option<Known>>,
        entry: ir::Block,
        block: ir::Block,
    ) -> Known {
        if block == entry {
            return Known::new();
        }
        let mut known: Option<Known> = None;
        for predecessor in cfg.pred_iter(block) {
            let Some(end) = &ends[predecessor.block] else {
                continue;
            };
            match &mut known {
                None => known = Some(end.clone()),
                Some(known) => meet(known, end),
            }
            if known.as_ref().is_some_and(|known| known.is_empty()) {
                break;
            }
        }
        known.unwrap_or_default()
    }

    /// What is known after `block`'s events when `known` is before them.
    fn at_end(&self, func: &ir::Function, block: ir::Block, mut known: Known) -> Known {
        for event in self.events(block) {
            match event {
                Event::Load { slot, inst } => {
                    if get(&known, slot).is_none() {
                        set(
                            &mut known,
                            slot,
                            Content::Value(func.dfg.first_result(inst)),
                        );
                    }
                }
                Event::Store { slot, content, .. } => set(&mut known, slot, content),
                Event::Enter { slot, value } => set(&mut known, slot, Content::Value(value)),
                Event::Call => known.retain(|entry| entry.1 == Content::Null),
            }
        }
        known
    }

    /// Solve for what is known at the end of each of `blocks`, the function's
    /// reachable blocks, each after its dominators. A block whose end has no
    /// answer yet does not constrain its successors; the loop ends when a
    /// whole round changes nothing, and by then every reachable block has an
    /// answer that follows from its predecessors' answers. `None` when that
    /// takes more than [`MAX_ROUNDS`] rounds or the answer outgrows
    /// [`MAX_KNOWN_PER_BLOCK`] entries per block on average.
    fn solve(
        &self,
        func: &ir::Function,
        cfg: &ControlFlowGraph,
        entry: ir::Block,
        blocks: &[ir::Block],
    ) -> Option<SecondaryMap<ir::Block, Option<Known>>> {
        let mut ends: SecondaryMap<ir::Block, Option<Known>> = SecondaryMap::new();
        // A block's end follows from its predecessors' ends alone. Each
        // evaluation of a block takes the next tick of `clock`, and a block
        // whose end changes records that tick; a block that was evaluated
        // after the last change of each of its predecessors would compute the
        // end it has, so it is not evaluated again. A block is its own
        // predecessor when it loops to itself, and then the change it made
        // at its own tick counts. Rounds, changes and the answer are those of
        // evaluating every block in every round.
        let mut clock: u32 = 0;
        let mut evaluated: SecondaryMap<ir::Block, u32> = SecondaryMap::new();
        let mut changed_at: SecondaryMap<ir::Block, u32> = SecondaryMap::new();
        let budget = MAX_KNOWN_PER_BLOCK.saturating_mul(blocks.len());
        let mut entries = 0usize;
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for &block in blocks {
                let seen = evaluated[block];
                if seen != 0
                    && !cfg
                        .pred_iter(block)
                        .any(|predecessor| changed_at[predecessor.block] >= seen)
                {
                    continue;
                }
                clock += 1;
                evaluated[block] = clock;
                let start = self.at_start(cfg, &ends, entry, block);
                let end = self.at_end(func, block, start);
                if ends[block].as_ref() != Some(&end) {
                    entries = entries - ends[block].as_ref().map_or(0, Vec::len) + end.len();
                    if entries > budget {
                        return None;
                    }
                    ends[block] = Some(end);
                    changed_at[block] = clock;
                    changed = true;
                }
            }
            if !changed {
                return Some(ends);
            }
        }
        None
    }

    /// Read before a loop what the loop would read in every iteration.
    /// Returns whether anything changed.
    ///
    /// Inside a loop that makes no call, a slot's content is known from its
    /// first load to the loop's end. It is not known at the loop's head
    /// unless it was known before the loop, because there the path into the
    /// loop meets the path around it; so the first load stays and runs in
    /// every iteration. This gives the head a parameter for the content
    /// instead: the path into the loop reads the slot at its end, and the
    /// paths around the loop pass what they know.
    ///
    /// `ends` is [`Reloads::solve`]'s answer for `func` as it is. The loads
    /// and parameters this adds are events for the next answer; `self` then
    /// describes the changed function.
    ///
    /// A block gets a parameter for a slot `s` when
    ///
    /// * it is the head of a loop: it dominates one of its predecessors;
    /// * some path from its start loads `s` before any call or store to `s`
    ///   (found by one backward pass that takes a block not yet seen to want
    ///   nothing, so it finds less than there is, never more);
    /// * its predecessors are all reachable and do not all agree on the
    ///   content of `s`, and at least one predecessor inside the loop knows
    ///   it at its end: an iteration that ends there read the slot, or
    ///   would have, and reads nothing now;
    /// * each predecessor that does not know it ends in a `jump`, so a load
    ///   before that `jump` is on the edge and on no other path.
    ///
    /// The argument an edge passes is what the slot holds at the end of the
    /// predecessor: the value `ends` names, a zero for the null reference, or
    /// the result of the new load. Nothing between the end of a predecessor
    /// and the block's first event for `s` writes the slot or calls, so the
    /// parameter is the slot's content from the block's start to that event,
    /// which is what [`Event::Enter`] states. A load from the frame cannot
    /// trap, so reading on the way into a loop that then leaves before it
    /// would have read costs that one load and changes nothing else.
    fn hoist(
        &mut self,
        func: &mut ir::Function,
        cfg: &ControlFlowGraph,
        domtree: &DominatorTree,
        entry: ir::Block,
        blocks: &[ir::Block],
        ends: &SecondaryMap<ir::Block, Option<Known>>,
    ) -> bool {
        // Loop heads, with their predecessors.
        let mut heads = Vec::new();
        for &block in blocks {
            if block != entry
                && cfg
                    .pred_iter(block)
                    .any(|predecessor| domtree.block_dominates(block, predecessor.block))
            {
                heads.push(block);
            }
        }
        if heads.is_empty() {
            return false;
        }

        // The slots that some path from a block's start loads before it
        // calls or stores to them, ordered by number.
        let mut wanted: SecondaryMap<ir::Block, Vec<u32>> = SecondaryMap::new();
        for &block in blocks.iter().rev() {
            let mut slots: Vec<u32> = Vec::new();
            for successor in cfg.succ_iter(block) {
                for &slot in &wanted[successor] {
                    if let Err(index) = slots.binary_search(&slot) {
                        slots.insert(index, slot);
                    }
                }
            }
            for event in self.events(block).rev() {
                match event {
                    Event::Load { slot, .. } => {
                        if let Err(index) = slots.binary_search(&slot) {
                            slots.insert(index, slot);
                        }
                    }
                    Event::Store { slot, .. } | Event::Enter { slot, .. } => {
                        if let Ok(index) = slots.binary_search(&slot) {
                            slots.remove(index);
                        }
                    }
                    Event::Call => slots.clear(),
                }
            }
            wanted[block] = slots;
        }

        let mut hoisted = false;
        let mut predecessors = Vec::new();
        let mut contents = Vec::new();
        for block in heads {
            if wanted[block].is_empty() {
                continue;
            }
            predecessors.clear();
            predecessors.extend(cfg.pred_iter(block));
            if predecessors
                .iter()
                .any(|predecessor| ends[predecessor.block].is_none())
            {
                continue;
            }
            let start = self.at_start(cfg, ends, entry, block);
            for &slot in &wanted[block] {
                let Some(access) = self.accesses[slot as usize] else {
                    continue;
                };
                if get(&start, slot).is_some() {
                    continue;
                }
                contents.clear();
                contents.extend(predecessors.iter().map(|predecessor| {
                    get(
                        ends[predecessor.block]
                            .as_ref()
                            .expect("checked above to be reachable"),
                        slot,
                    )
                }));
                let mut edges = predecessors.iter().zip(&contents);
                let known_around = edges.clone().any(|(predecessor, content)| {
                    content.is_some() && domtree.block_dominates(block, predecessor.block)
                });
                let loads_on_edges = edges.all(|(predecessor, content)| {
                    content.is_some()
                        || func.dfg.insts[predecessor.inst].opcode() == ir::Opcode::Jump
                });
                if !known_around || !loads_on_edges {
                    continue;
                }

                let parameter = func.dfg.append_block_param(block, access.ty);
                for (predecessor, content) in predecessors.iter().zip(&contents) {
                    let mut pos = FuncCursor::new(func).at_inst(predecessor.inst);
                    let argument = match *content {
                        Some(Content::Value(value)) => value,
                        Some(Content::Null) => pos.ins().iconst(access.ty, 0),
                        None => {
                            let flags = pos.func.dfg.mem_flags[access.flags];
                            let address = pos.ins().stack_addr(access.pointer_type, access.slot, 0);
                            let loaded = pos.ins().load(access.ty, flags, address, 0);
                            let inst = pos.func.dfg.value_def(loaded).unwrap_inst();
                            self.appended[predecessor.block].push(Event::Load { slot, inst });
                            loaded
                        }
                    };
                    let dfg = &mut func.dfg;
                    for destination in dfg.insts[predecessor.inst]
                        .branch_destination_mut(&mut dfg.jump_tables, &mut dfg.exception_tables)
                    {
                        if destination.block(&dfg.value_lists) == block {
                            destination.append_argument(argument, &mut dfg.value_lists);
                        }
                    }
                }
                self.entered[block].push(Event::Enter {
                    slot,
                    value: parameter,
                });
                hoisted = true;
            }
        }
        hoisted
    }
}

const NONE: u32 = u32::MAX;

fn is_zero(func: &ir::Function, value: ir::Value) -> bool {
    func.dfg.value_def(value).inst().is_some_and(|inst| {
        matches!(
            func.dfg.insts[inst],
            ir::InstructionData::UnaryImm {
                opcode: ir::Opcode::Iconst,
                imm,
            } if imm.bits() == 0
        )
    })
}

impl Reloads {
    /// Remove the loads from the handled slots of `func` that no call makes
    /// necessary (see the module documentation), and return how many.
    ///
    /// `self` must be [`Reloads::scan`] of `func` as it is now, and `cfg`
    /// and `domtree` current for it. The pass changes no block and no branch,
    /// so they stay current.
    pub(crate) fn forward(
        self,
        func: &mut ir::Function,
        cfg: &ControlFlowGraph,
        domtree: &DominatorTree,
    ) -> usize {
        let mut reloads = self;
        let Some(entry) = func.layout.entry_block() else {
            return 0;
        };
        // Reachable blocks, each after its dominators.
        let blocks: Vec<ir::Block> = domtree.cfg_rpo().copied().collect();
        let Some(mut ends) = reloads.solve(func, cfg, entry, &blocks) else {
            return 0;
        };
        if reloads.hoist(func, cfg, domtree, entry, &blocks, &ends) {
            // Should this not be solved, the function has parameters and
            // loads it does not need, which is harmless.
            match reloads.solve(func, cfg, entry, &blocks) {
                Some(again) => ends = again,
                None => return 0,
            }
        }

        // Rewrite. The solution names only results of loads that stay.
        let mut removed = 0;
        let mut kept = 0usize;
        for &block in &blocks {
            let mut known = reloads.at_start(cfg, &ends, entry, block);
            for event in reloads.events(block) {
                match event {
                    Event::Load { slot, inst } => match get(&known, slot) {
                        Some(Content::Value(value)) => {
                            let result = func.dfg.first_result(inst);
                            func.dfg.clear_results(inst);
                            func.dfg.change_to_alias(result, value);
                            func.layout.remove_inst(inst);
                            removed += 1;
                        }
                        Some(Content::Null) => {
                            let ty = func.dfg.value_type(func.dfg.first_result(inst));
                            func.replace(inst).iconst(ty, 0);
                            removed += 1;
                        }
                        None => {
                            set(
                                &mut known,
                                slot,
                                Content::Value(func.dfg.first_result(inst)),
                            );
                            kept += 1;
                            unshare_address(func, inst, kept);
                        }
                    },
                    Event::Store { slot, content, .. } => set(&mut known, slot, content),
                    Event::Enter { slot, value } => set(&mut known, slot, Content::Value(value)),
                    Event::Call => known.retain(|entry| entry.1 == Content::Null),
                }
            }
        }

        // A home that nothing reads any more is not written either.
        for &(number, home) in &reloads.homes {
            let read = reloads
                .events
                .iter()
                .chain(reloads.appended.values().flatten())
                .any(|event| {
                    matches!(*event, Event::Load { slot, inst }
                        if slot == number && func.layout.inst_block(inst).is_some())
                });
            if read {
                continue;
            }
            for event in &reloads.events {
                if let Event::Store { slot, inst, .. } = *event {
                    if slot == number {
                        func.layout.remove_inst(inst);
                    }
                }
            }
            func.sized_stack_slots[home].size = 0;
        }
        removed
    }
}

/// Give the load `inst` of a whole stack slot a `stack_addr` of its own, at
/// a distance past the slot that no other load uses, with the opposite load
/// offset.
///
/// The frontend addresses every access of a slot through `stack_addr slot,
/// 0`, which Cranelift then makes one value per slot. The x64 backend folds
/// that value into the address of a store or a load, but not when it merges
/// a load into the instruction that uses it: then the address is computed
/// into a register where the shared value was first needed, usually next to
/// the store at the reference's definition, and kept in a callee-saved
/// register or a spill slot from there to each merged load. An address of
/// the load's own is computed next to it instead.
fn unshare_address(func: &mut ir::Function, inst: ir::Inst, ordinal: usize) {
    let ir::InstructionData::Load { arg, .. } = func.dfg.insts[inst] else {
        unreachable!("only loads are recorded as load events")
    };
    let address = func.dfg.resolve_aliases(arg);
    let definition = func
        .dfg
        .value_def(address)
        .inst()
        .expect("a slot's address is a `stack_addr` result");
    let ir::InstructionData::StackAddr {
        stack_slot, offset, ..
    } = func.dfg.insts[definition]
    else {
        unreachable!("a slot's address is a `stack_addr` result")
    };
    let pointer_type = func.dfg.value_type(address);
    let bytes = func.sized_stack_slots[stack_slot].size as usize;
    let Some(distance) = ordinal
        .checked_mul(bytes.max(1))
        .and_then(|distance| i32::try_from(distance).ok())
    else {
        // Out of distances: this load shares the slot's own address.
        return;
    };
    let base: i32 = offset.into();
    let Some(shifted) = base.checked_add(distance) else {
        return;
    };
    let own =
        FuncCursor::new(func)
            .at_inst(inst)
            .ins()
            .stack_addr(pointer_type, stack_slot, shifted);
    let ir::InstructionData::Load { arg, offset, .. } = &mut func.dfg.insts[inst] else {
        unreachable!()
    };
    *arg = own;
    // The whole-slot access had `base + offset == 0`.
    *offset = (-shifted).into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{
        AbiParam, ExceptionTableData, Function, Signature, UserFuncName, types::I32, types::I64,
    };
    use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
    use cranelift_codegen::settings;
    use cranelift_codegen::verifier::verify_function;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

    const FRONTEND: TargetFrontendConfig = TargetFrontendConfig {
        default_call_conv: CallConv::SystemV,
        pointer_width: target_lexicon::PointerWidth::U64,
        page_size_align_log2: 12,
    };

    /// A function `(i32 reference, i32 reference, i64 condition) -> i32`
    /// under construction, whose parameters and marked values the frontend
    /// keeps in stack maps.
    struct Harness {
        func: Function,
        context: FunctionBuilderContext,
    }

    struct Body<'a> {
        builder: FunctionBuilder<'a>,
        callee: ir::FuncRef,
        signature: ir::SigRef,
        first: ir::Value,
        second: ir::Value,
        condition: ir::Value,
    }

    impl Harness {
        fn new() -> Self {
            let mut signature = Signature::new(CallConv::SystemV);
            signature.params.push(AbiParam::new(I32));
            signature.params.push(AbiParam::new(I32));
            signature.params.push(AbiParam::new(I64));
            signature.returns.push(AbiParam::new(I32));
            Self {
                func: Function::with_name_signature(UserFuncName::testcase("reloads"), signature),
                context: FunctionBuilderContext::new(),
            }
        }

        fn body(&mut self) -> Body<'_> {
            let mut builder = FunctionBuilder::new(&mut self.func, &mut self.context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let [first, second, condition] = *builder.block_params(entry) else {
                unreachable!()
            };
            builder.declare_value_needs_stack_map(first);
            builder.declare_value_needs_stack_map(second);
            let signature = builder
                .func
                .import_signature(Signature::new(CallConv::SystemV));
            let name = builder
                .func
                .declare_imported_user_function(ir::UserExternalName::new(0, 0));
            let callee = builder.func.import_function(ir::ExtFuncData {
                name: ir::ExternalName::user(name),
                signature,
                colocated: true,
                patchable: false,
            });
            Body {
                builder,
                callee,
                signature,
                first,
                second,
                condition,
            }
        }

        /// Run the pass, verify the function and return how many loads it
        /// removed.
        fn forward(&mut self) -> usize {
            self.forward_with(&[])
        }

        /// The same for a function with these home slots.
        fn forward_with(&mut self, homes: &[ir::StackSlot]) -> usize {
            let cfg = ControlFlowGraph::with_function(&self.func);
            let domtree = DominatorTree::with_function(&self.func, &cfg);
            let removed = match Reloads::scan(&self.func, homes) {
                Some(reloads) => reloads.forward(&mut self.func, &cfg, &domtree),
                None => 0,
            };
            let flags = settings::Flags::new(settings::builder());
            if let Err(errors) = verify_function(&self.func, &flags) {
                panic!("{errors}\n{}", self.func.display());
            }
            removed
        }

        fn resolve(&self, value: ir::Value) -> ir::Value {
            self.func.dfg.resolve_aliases(value)
        }

        /// The slot `value` is loaded from, when it is the result of a load
        /// from a stack slot.
        fn reload(&self, value: ir::Value) -> Option<ir::StackSlot> {
            let dfg = &self.func.dfg;
            let inst = dfg.value_def(self.resolve(value)).inst()?;
            let ir::InstructionData::Load { arg, offset, .. } = dfg.insts[inst] else {
                return None;
            };
            let address = dfg.value_def(dfg.resolve_aliases(arg)).inst()?;
            let ir::InstructionData::StackAddr {
                stack_slot,
                offset: distance,
                ..
            } = dfg.insts[address]
            else {
                return None;
            };
            assert_eq!(i32::from(offset) + i32::from(distance), 0);
            Some(stack_slot)
        }

        /// The loads from stack slots that are left in the layout.
        fn reloads(&self) -> usize {
            let func = &self.func;
            func.layout
                .blocks()
                .flat_map(|block| func.layout.block_insts(block))
                .filter(|&inst| {
                    func.dfg.insts[inst].opcode() == ir::Opcode::Load
                        && self.reload(func.dfg.first_result(inst)).is_some()
                })
                .count()
        }

        /// The arguments `from`'s terminator passes to `to`, once resolved.
        fn arguments(&self, from: ir::Block, to: ir::Block) -> Vec<ir::Value> {
            let dfg = &self.func.dfg;
            let terminator = self.func.layout.last_inst(from).unwrap();
            dfg.insts[terminator]
                .branch_destination(&dfg.jump_tables, &dfg.exception_tables)
                .iter()
                .filter(|destination| destination.block(&dfg.value_lists) == to)
                .flat_map(|destination| destination.args(&dfg.value_lists))
                .map(|argument| match argument {
                    ir::BlockArg::Value(value) => self.resolve(value),
                    other => panic!("unexpected block argument {other:?}"),
                })
                .collect()
        }

        /// The operands of the instruction that defines `value`.
        fn operands(&self, value: ir::Value) -> Vec<ir::Value> {
            let dfg = &self.func.dfg;
            let inst = dfg.value_def(value).unwrap_inst();
            dfg.inst_args(inst)
                .iter()
                .map(|&operand| self.resolve(operand))
                .collect()
        }
    }

    impl Body<'_> {
        fn call(&mut self) {
            self.builder.ins().call(self.callee, &[]);
        }

        /// An instruction that uses `value` and can trap.
        fn check(&mut self, value: ir::Value) {
            self.builder
                .ins()
                .trapz(value, ir::TrapCode::unwrap_user(1));
        }

        fn sum(&mut self, left: ir::Value, right: ir::Value) -> ir::Value {
            self.builder.ins().iadd(left, right)
        }

        fn block(&mut self) -> ir::Block {
            self.builder.create_block()
        }

        fn switch_to(&mut self, block: ir::Block) {
            self.builder.switch_to_block(block);
        }

        fn finish(mut self, result: ir::Value) {
            self.builder.ins().return_(&[result]);
            self.builder.seal_all_blocks();
            self.builder.finalize(FRONTEND);
        }
    }

    #[test]
    fn a_function_without_calls_has_nothing_to_forward() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let sum = body.sum(body.first, body.second);
        body.finish(sum);
        assert_eq!(harness.forward(), 0);
        assert_eq!(harness.reloads(), 0);
    }

    #[test]
    fn a_reference_is_loaded_once_after_a_call() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        // Before the first call the value itself is at hand.
        let before = body.sum(first, first);
        body.check(first);
        body.call();
        let after = body.sum(first, first);
        // A trapping instruction is no call.
        body.check(first);
        let later = body.sum(first, after);
        // No use follows these calls before the next one.
        body.call();
        body.call();
        let last = body.sum(first, later);
        let result = body.sum(before, last);
        body.finish(result);
        // The frontend keeps the original SSA value before the first call and
        // reloads once per safepoint-separated segment inside the block.
        assert_eq!(harness.reloads(), 2);
        assert_eq!(harness.forward(), 0);
        assert_eq!(harness.reloads(), 2);
        assert_eq!(harness.operands(before), [first, first]);
        let [reloaded, same] = harness.operands(after)[..] else {
            unreachable!()
        };
        assert!(harness.reload(reloaded).is_some());
        assert_eq!(same, reloaded);
        assert_eq!(harness.operands(later)[0], reloaded);
        let again = harness.operands(last)[0];
        assert!(harness.reload(again).is_some());
        assert_ne!(again, reloaded);
    }

    #[test]
    fn each_load_that_stays_has_an_address_of_its_own() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        body.call();
        let one = body.sum(first, first);
        body.call();
        let two = body.sum(first, one);
        body.finish(two);
        harness.forward();
        let func = &harness.func;
        let mut addresses = Vec::new();
        for block in func.layout.blocks() {
            for inst in func.layout.block_insts(block) {
                if let ir::InstructionData::Load { arg, .. } = func.dfg.insts[inst] {
                    addresses.push(func.dfg.value_def(arg).unwrap_inst());
                }
            }
        }
        assert_eq!(addresses.len(), 2);
        let distances: Vec<i32> = addresses
            .iter()
            .map(|&inst| match func.dfg.insts[inst] {
                ir::InstructionData::StackAddr { offset, .. } => offset.into(),
                _ => unreachable!(),
            })
            .collect();
        assert_ne!(distances[0], distances[1]);
        assert!(distances.iter().all(|&distance| distance != 0));
    }

    #[test]
    fn a_join_keeps_what_every_path_brings() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let (first, second) = (body.first, body.second);
        let then = body.block();
        let otherwise = body.block();
        let join = body.block();
        body.call();
        let before = body.sum(first, second);
        body.builder
            .ins()
            .brif(body.condition, then, &[], otherwise, &[]);

        body.switch_to(then);
        let in_then = body.sum(first, first);
        body.check(in_then);
        body.builder.ins().jump(join, &[]);

        body.switch_to(otherwise);
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let after = body.sum(first, second);
        let result = body.sum(before, after);
        body.finish(result);
        assert_eq!(harness.forward(), 3);
        assert_eq!(harness.reloads(), 2);
        assert_eq!(harness.operands(after), harness.operands(before));
        assert_eq!(harness.operands(in_then)[0], harness.operands(before)[0]);
    }

    #[test]
    fn a_join_below_a_call_on_one_path_loads_again() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let slow = body.block();
        let join = body.block();
        body.call();
        let before = body.sum(first, first);
        body.builder
            .ins()
            .brif(body.condition, slow, &[], join, &[]);

        body.switch_to(slow);
        body.call();
        let in_slow = body.sum(first, first);
        body.check(in_slow);
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let after = body.sum(first, first);
        let result = body.sum(before, after);
        body.finish(result);
        assert_eq!(harness.forward(), 0);
        let loaded = [before, in_slow, after].map(|sum| {
            let [left, right] = harness.operands(sum)[..] else {
                unreachable!()
            };
            assert!(harness.reload(left).is_some());
            assert_eq!(left, right);
            left
        });
        // The paths bring different values, so the join reads the slot.
        assert_ne!(loaded[0], loaded[1]);
        assert_ne!(loaded[2], loaded[0]);
        assert_ne!(loaded[2], loaded[1]);
        assert_eq!(
            harness
                .func
                .layout
                .inst_block(harness.func.dfg.value_def(loaded[2]).unwrap_inst()),
            Some(join)
        );
    }

    #[test]
    fn a_loop_without_a_call_loads_nothing() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let header = body.block();
        let exit = body.block();
        body.call();
        let before = body.sum(first, first);
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.sum(first, before);
        body.check(in_loop);
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.sum(first, before);
        body.finish(after);
        assert_eq!(harness.forward(), 2);
        assert_eq!(harness.reloads(), 1);
        let loaded = harness.operands(before)[0];
        assert_eq!(harness.operands(in_loop)[0], loaded);
        assert_eq!(harness.operands(after)[0], loaded);
    }

    #[test]
    fn a_loop_that_calls_reads_once_in_each_iteration() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let entry = body.builder.current_block().unwrap();
        let header = body.block();
        let exit = body.block();
        let before = body.sum(first, first);
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        // What the back edge brings was read after the loop's call; the
        // loop's head takes it, or the value from before the loop, as a
        // parameter.
        let at_top = body.sum(first, first);
        body.check(at_top);
        body.call();
        let after_call = body.sum(first, first);
        body.check(after_call);
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.sum(first, before);
        body.finish(after);
        harness.forward();
        assert_eq!(harness.operands(before), [first, first]);
        let bottom = harness.operands(after_call)[0];
        assert!(harness.reload(bottom).is_some());
        let &[parameter] = harness.func.dfg.block_params(header) else {
            panic!("the loop's head takes one parameter")
        };
        assert_eq!(harness.operands(at_top), [parameter, parameter]);
        assert_eq!(harness.arguments(entry, header), [first]);
        assert_eq!(harness.arguments(header, header), [bottom]);
        // The exit is entered only from the loop's end.
        assert_eq!(harness.operands(after)[0], bottom);
        assert_eq!(harness.reloads(), 1);
    }

    #[test]
    fn a_loop_reads_before_it_is_entered_what_every_iteration_uses() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let entry = body.builder.current_block().unwrap();
        let header = body.block();
        let exit = body.block();
        // Nothing is known when the loop is entered.
        body.call();
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.sum(first, first);
        body.check(in_loop);
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.sum(first, in_loop);
        body.finish(after);
        harness.forward();
        let &[parameter] = harness.func.dfg.block_params(header) else {
            panic!("the loop's head takes one parameter")
        };
        assert_eq!(harness.operands(in_loop), [parameter, parameter]);
        // The one load is at the end of the path into the loop; the loop
        // passes its own parameter around.
        let &[loaded] = &harness.arguments(entry, header)[..] else {
            unreachable!()
        };
        assert!(harness.reload(loaded).is_some());
        assert_eq!(
            harness
                .func
                .layout
                .inst_block(harness.func.dfg.value_def(loaded).unwrap_inst()),
            Some(entry)
        );
        assert_eq!(harness.arguments(header, header), [parameter]);
        assert_eq!(harness.operands(after)[0], parameter);
        assert_eq!(harness.reloads(), 1);
    }

    #[test]
    fn a_loop_that_may_leave_before_it_reads_still_reads_once() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let entry = body.builder.current_block().unwrap();
        let header = body.block();
        let turn = body.block();
        let exit = body.block();
        body.call();
        body.builder.ins().jump(header, &[]);

        // The head tests whether to go on before anything is read.
        body.switch_to(header);
        body.builder
            .ins()
            .brif(body.condition, turn, &[], exit, &[]);

        body.switch_to(turn);
        let in_loop = body.sum(first, first);
        body.check(in_loop);
        body.builder.ins().jump(header, &[]);

        body.switch_to(exit);
        let result = body.builder.ins().iconst(I32, 0);
        body.finish(result);
        harness.forward();
        let &[parameter] = harness.func.dfg.block_params(header) else {
            panic!("the loop's head takes one parameter")
        };
        assert_eq!(harness.operands(in_loop), [parameter, parameter]);
        let &[loaded] = &harness.arguments(entry, header)[..] else {
            unreachable!()
        };
        assert!(harness.reload(loaded).is_some());
        assert_eq!(harness.arguments(turn, header), [parameter]);
        assert_eq!(harness.reloads(), 1);
    }

    #[test]
    fn a_loop_that_calls_before_it_reads_keeps_its_load() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let header = body.block();
        let exit = body.block();
        let before = body.sum(first, first);
        body.builder.ins().jump(header, &[]);

        // No path from the head reads before the call.
        body.switch_to(header);
        body.call();
        let in_loop = body.sum(first, before);
        body.check(in_loop);
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.sum(first, before);
        body.finish(after);
        harness.forward();
        assert!(harness.func.dfg.block_params(header).is_empty());
        let loaded = harness.operands(in_loop)[0];
        assert!(harness.reload(loaded).is_some());
        assert_eq!(harness.operands(after)[0], loaded);
        assert_eq!(harness.reloads(), 1);
    }

    #[test]
    fn the_successors_of_a_call_that_ends_its_block_load() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let returned = body.block();
        let caught = body.block();
        let before = body.sum(first, first);
        let normal = body.builder.func.dfg.block_call(returned, &[]);
        let handler = body.builder.func.dfg.block_call(caught, &[]);
        let table = body
            .builder
            .func
            .dfg
            .exception_tables
            .push(ExceptionTableData::new(
                body.signature,
                normal,
                [ir::ExceptionTableItem::Default(handler)],
            ));
        body.builder.ins().try_call(body.callee, &[], table);

        body.switch_to(returned);
        let in_returned = body.sum(first, before);
        body.builder.ins().return_(&[in_returned]);

        body.switch_to(caught);
        let in_caught = body.sum(first, first);
        body.finish(in_caught);
        harness.forward();
        assert_eq!(harness.operands(before), [first, first]);
        let returned = harness.operands(in_returned)[0];
        let caught = harness.operands(in_caught)[0];
        assert!(harness.reload(returned).is_some());
        assert!(harness.reload(caught).is_some());
        assert_ne!(returned, caught);
        assert_eq!(harness.operands(in_caught)[1], caught);
    }

    #[test]
    fn a_null_reference_is_never_loaded() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let null = body.builder.ins().iconst(I32, 0);
        body.builder.declare_value_needs_stack_map(null);
        let header = body.block();
        let exit = body.block();
        body.call();
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.sum(null, null);
        body.check(in_loop);
        body.call();
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.sum(null, body.first);
        body.finish(after);
        harness.forward();
        // Only the parameter is read.
        assert_eq!(harness.reloads(), 1);
        for value in harness
            .operands(in_loop)
            .into_iter()
            .chain([harness.operands(after)[0]])
        {
            let inst = harness.func.dfg.value_def(value).unwrap_inst();
            assert!(matches!(
                harness.func.dfg.insts[inst],
                ir::InstructionData::UnaryImm { opcode: ir::Opcode::Iconst, imm } if imm.bits() == 0
            ));
        }
        assert!(harness.reload(harness.operands(after)[1]).is_some());
    }

    #[test]
    fn a_slot_used_again_holds_the_later_value() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let (first, second) = (body.first, body.second);
        body.call();
        // The last use of the first reference: its slot is free below.
        let early = body.sum(first, second);
        let fresh = body.builder.ins().iadd_imm_s(early, 8);
        body.builder.declare_value_needs_stack_map(fresh);
        let near = body.sum(fresh, second);
        body.call();
        let late = body.sum(fresh, second);
        let result = body.sum(near, late);
        body.finish(result);
        harness.forward();
        let fresh = harness.resolve(fresh);
        assert_eq!(harness.operands(near)[0], fresh);
        // After the call the slot is read, whichever slot the frontend chose
        // for the value, and never taken for the first reference.
        let reloaded = harness.operands(late)[0];
        assert!(harness.reload(reloaded).is_some());
        assert_ne!(reloaded, harness.operands(early)[0]);
        assert_eq!(harness.operands(near)[1], harness.operands(early)[1]);
    }

    #[test]
    fn a_slot_whose_address_escapes_is_left_alone() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        let next = body.block();
        body.call();
        let one = body.sum(first, first);
        body.check(one);
        body.builder.ins().jump(next, &[]);

        body.switch_to(next);
        let two = body.sum(first, one);
        body.finish(two);
        // Hand the slot's address to an instruction that is no load or store.
        let func = &mut harness.func;
        let slot = func
            .sized_stack_slots
            .keys()
            .next()
            .expect("the reference has a slot");
        let entry = func.layout.entry_block().unwrap();
        let last = func.layout.last_inst(entry).unwrap();
        let mut pos = FuncCursor::new(func).at_inst(last);
        let address = pos.ins().stack_addr(I64, slot, 0);
        pos.ins().iadd_imm_s(address, 1);
        let before = harness.reloads();
        assert_eq!(before, 2);
        assert_eq!(harness.forward(), 0);
        assert_eq!(harness.reloads(), before);
    }

    #[test]
    fn a_slot_read_in_part_is_left_alone() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        body.call();
        let one = body.sum(first, first);
        body.finish(one);
        let func = &mut harness.func;
        let slot = func
            .sized_stack_slots
            .keys()
            .next()
            .expect("the reference has a slot");
        let entry = func.layout.entry_block().unwrap();
        let last = func.layout.last_inst(entry).unwrap();
        let mut pos = FuncCursor::new(func).at_inst(last);
        let address = pos.ins().stack_addr(I64, slot, 0);
        pos.ins()
            .load(ir::types::I8, ir::MemFlagsData::trusted(), address, 0);
        let before = harness.reloads();
        assert_eq!(harness.forward(), 0);
        assert_eq!(harness.reloads(), before);
    }

    #[test]
    fn a_slot_read_as_another_type_is_left_alone() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.first;
        body.call();
        let one = body.sum(first, first);
        body.finish(one);
        let func = &mut harness.func;
        let slot = func
            .sized_stack_slots
            .keys()
            .next()
            .expect("the reference has a slot");
        let entry = func.layout.entry_block().unwrap();
        let last = func.layout.last_inst(entry).unwrap();
        let mut pos = FuncCursor::new(func).at_inst(last);
        let address = pos.ins().stack_addr(I64, slot, 0);
        pos.ins()
            .load(ir::types::F32, ir::MemFlagsData::trusted(), address, 0);
        let before = harness.reloads();
        assert_eq!(harness.forward(), 0);
        assert_eq!(harness.reloads(), before);
    }

    /// A callee `(i64)` of `builder`'s function.
    fn callee_with_argument(builder: &mut FunctionBuilder<'_>) -> ir::FuncRef {
        let mut signature = Signature::new(CallConv::SystemV);
        signature.params.push(AbiParam::new(I64));
        let signature = builder.func.import_signature(signature);
        let name = builder
            .func
            .declare_imported_user_function(ir::UserExternalName::new(0, 1));
        builder.func.import_function(ir::ExtFuncData {
            name: ir::ExternalName::user(name),
            signature,
            colocated: true,
            patchable: false,
        })
    }

    fn home_region(func: &mut Function) -> ir::AliasRegion {
        func.dfg.alias_regions.insert(ir::AliasRegionData {
            user_id: 1,
            description: "home".into(),
        })
    }

    fn stores_to(func: &Function, slot: ir::StackSlot) -> usize {
        func.layout
            .blocks()
            .flat_map(|block| func.layout.block_insts(block))
            .filter(|&inst| match func.dfg.insts[inst] {
                ir::InstructionData::Store { args, .. } => {
                    let address = func.dfg.value_def(args[1]).unwrap_inst();
                    func.dfg.insts[address].stack_slot() == Some(slot)
                }
                _ => false,
            })
            .count()
    }

    #[test]
    fn a_home_is_read_once_after_a_call() {
        use super::super::base_home::FrameHome;
        let mut harness = Harness::new();
        let mut body = harness.body();
        let condition = body.condition;
        let region = home_region(body.builder.func);
        let home = FrameHome::store_at_entry(&mut body.builder.cursor(), region, I64, condition);
        let slow = body.block();
        let join = body.block();
        let before = [(); 2].map(|()| home.load(&mut body.builder.cursor()));
        body.call();
        let after = [(); 2].map(|()| home.load(&mut body.builder.cursor()));
        body.builder.ins().brif(condition, slow, &[], join, &[]);

        body.switch_to(slow);
        body.call();
        body.builder.ins().jump(join, &[]);

        // One path called since the last load: the join reads the home.
        body.switch_to(join);
        let joined = [(); 2].map(|()| home.load(&mut body.builder.cursor()));
        let mut sum = before[0];
        for value in [before[1], after[0], after[1], joined[0], joined[1]] {
            sum = body.builder.ins().iadd(sum, value);
        }
        let result = body.builder.ins().ireduce(I32, sum);
        body.finish(result);
        assert_eq!(harness.forward_with(&[home.slot()]), 4);
        // Up to the first call the stored value itself is at hand.
        assert_eq!(before.map(|value| harness.resolve(value)), [condition; 2]);
        assert_eq!(harness.reload(after[0]), Some(home.slot()));
        assert_eq!(harness.resolve(after[1]), harness.resolve(after[0]));
        assert_eq!(harness.reload(joined[0]), Some(home.slot()));
        assert_ne!(harness.resolve(joined[0]), harness.resolve(after[0]));
        assert_eq!(harness.resolve(joined[1]), harness.resolve(joined[0]));
        assert_eq!(stores_to(&harness.func, home.slot()), 1);
    }

    #[test]
    fn a_home_that_nothing_reads_is_not_written() {
        use super::super::base_home::FrameHome;
        let mut harness = Harness::new();
        let mut body = harness.body();
        let condition = body.condition;
        let region = home_region(body.builder.func);
        let home = FrameHome::store_at_entry(&mut body.builder.cursor(), region, I64, condition);
        // Every use precedes the function's only call.
        let first = home.load(&mut body.builder.cursor());
        let second = home.load(&mut body.builder.cursor());
        let sum = body.builder.ins().iadd(first, second);
        body.call();
        let result = body.builder.ins().ireduce(I32, sum);
        body.finish(result);
        assert_eq!(harness.forward_with(&[home.slot()]), 2);
        assert_eq!(harness.resolve(first), condition);
        assert_eq!(harness.resolve(second), condition);
        assert_eq!(stores_to(&harness.func, home.slot()), 0);
        assert_eq!(harness.func.sized_stack_slots[home.slot()].size, 0);
    }

    #[test]
    fn a_parameter_without_a_call_to_split_at_gets_no_home() {
        use super::super::base_home::FrameHome;
        let mut harness = Harness::new();
        let mut body = harness.body();
        let condition = body.condition;
        let sum = body.builder.ins().iadd(condition, condition);
        let result = body.builder.ins().ireduce(I32, sum);
        body.finish(result);
        let region = home_region(&mut harness.func);
        let before = harness.func.display().to_string();
        assert!(FrameHome::of_parameter(&mut harness.func, region, condition).is_none());
        assert_eq!(harness.func.display().to_string(), before);
    }

    #[test]
    fn a_parameter_s_uses_read_its_home_once_after_a_call() {
        use super::super::base_home::FrameHome;
        let mut harness = Harness::new();
        let mut body = harness.body();
        // The translator passes the `vmctx` itself to every call and reads
        // through it in between.
        let parameter = body.condition;
        let callee = callee_with_argument(&mut body.builder);
        let mut sums = Vec::new();
        let mut calls = Vec::new();
        sums.push(body.builder.ins().iadd(parameter, parameter));
        calls.push(body.builder.ins().call(callee, &[parameter]));
        sums.push(body.builder.ins().iadd(parameter, parameter));
        body.check(body.first);
        sums.push(body.builder.ins().iadd(parameter, sums[1]));
        calls.push(body.builder.ins().call(callee, &[parameter]));
        calls.push(body.builder.ins().call(callee, &[parameter]));
        let result = body.builder.ins().ireduce(I32, sums[2]);
        body.finish(result);
        let region = home_region(&mut harness.func);
        let home = FrameHome::of_parameter(&mut harness.func, region, parameter).expect("a home");
        harness.forward_with(&[home.slot()]);
        let arguments = |inst: ir::Inst| -> Vec<ir::Value> {
            harness
                .func
                .dfg
                .inst_args(inst)
                .iter()
                .map(|&value| harness.resolve(value))
                .collect()
        };
        // Up to the first call the parameter itself is used.
        assert_eq!(harness.operands(sums[0]), [parameter, parameter]);
        assert_eq!(arguments(calls[0]), [parameter]);
        // One load after the first call serves every use up to the second.
        let after = harness.operands(sums[1])[0];
        assert_eq!(harness.reload(after), Some(home.slot()));
        assert_eq!(harness.operands(sums[1]), [after, after]);
        assert_eq!(harness.operands(sums[2])[0], after);
        assert_eq!(arguments(calls[1]), [after]);
        // The third call's argument is read after the second call.
        let last = arguments(calls[2])[0];
        assert_eq!(harness.reload(last), Some(home.slot()));
        assert_ne!(last, after);
        assert_eq!(stores_to(&harness.func, home.slot()), 1);
    }
}
