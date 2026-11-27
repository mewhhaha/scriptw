//! Checked SIMD lowering of complete, structurally matched packed array loops.

use std::iter::Peekable;

use cranelift_codegen::ir::{self, Endianness, InstBuilder, condcodes::IntCC, types::*};
use target_lexicon::{Aarch64Architecture, Architecture};
use wasmparser::{
    BlockType, FuncValidator, FunctionBody, Operator, OperatorsIterator, WasmModuleResources,
};
use wasmtime_environ::TripleExt;

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum KernelKind {
    AsciiNarrow,
    AsciiWiden,
    CodeUnitFind,
    JsonPlainRun,
    JsonParsePlainRun,
    CodeUnitEqual,
}

impl KernelKind {
    fn is_ascii_copy(self) -> bool {
        matches!(self, Self::AsciiNarrow | Self::AsciiWiden)
    }

    fn is_copy(self) -> bool {
        self.is_ascii_copy() || self == Self::JsonPlainRun
    }
}

#[derive(Clone, Copy)]
struct PackedArray {
    ty: TypeIndex,
    nullable: bool,
    elem_size: i64,
    base_size: i64,
}

impl PackedArray {
    fn match_operand(
        env: &mut FuncEnvironment<'_>,
        reference: WasmRefType,
        type_index: u32,
    ) -> Option<Self> {
        let WasmHeapType::ConcreteArray(reference_type) = reference.heap_type else {
            return None;
        };
        let ty = TypeIndex::from_u32(type_index);
        let interned = env.module.types.get(ty)?.as_module_type_index()?;
        if Some(interned) != reference_type.as_module_type_index() {
            return None;
        }
        let composite = &env.types[interned].composite_type;
        let WasmCompositeInnerType::Array(array) = &composite.inner else {
            return None;
        };
        if composite.shared {
            return None;
        }
        let elem_size = match array.0.element_type {
            WasmStorageType::I8 => 1,
            WasmStorageType::I16 => 2,
            _ => return None,
        };
        let layout = env.array_layout(interned).ok()?;
        if layout.elems_are_gc_refs || i64::from(layout.elem_size) != elem_size {
            return None;
        }
        Some(Self {
            ty,
            nullable: reference.nullable,
            elem_size,
            base_size: i64::from(layout.base_size),
        })
    }
}

struct PackedKernel {
    kind: KernelKind,
    source: PackedArray,
    other: Option<PackedArray>,
}

struct Shape<'a>(Peekable<OperatorsIterator<'a>>);

impl<'a> Shape<'a> {
    fn next(&mut self) -> Option<Operator<'a>> {
        self.0.next()?.ok()
    }

    fn expect(&mut self, expected: Operator<'a>) -> Option<()> {
        (self.next()? == expected).then_some(())
    }

    fn expect_all(&mut self, expected: &[Operator<'a>]) -> Option<()> {
        for op in expected {
            self.expect(op.clone())?;
        }
        Some(())
    }

    fn reference(&mut self, local_index: u32, reference: WasmRefType) -> Option<()> {
        self.expect(Operator::LocalGet { local_index })?;
        if !reference.nullable && matches!(self.0.peek(), Some(Ok(Operator::RefAsNonNull))) {
            self.next()?;
        }
        Some(())
    }

    fn array_get(&mut self, local: u32, reference: WasmRefType, index: u32) -> Option<u32> {
        self.reference(local, reference)?;
        self.expect(Operator::LocalGet { local_index: index })?;
        let Operator::ArrayGetU { array_type_index } = self.next()? else {
            return None;
        };
        Some(array_type_index)
    }

    fn store_and_load(&mut self) -> Option<u32> {
        use Operator::*;
        match self.next()? {
            // The code sink fuses an immediate set/get of one local.
            LocalTee { local_index } => Some(local_index),
            LocalSet { local_index } => {
                self.expect(LocalGet { local_index })?;
                Some(local_index)
            }
            _ => None,
        }
    }

    fn increment_value(&mut self, local_index: u32) -> Option<()> {
        use Operator::*;
        self.expect_all(&[LocalGet { local_index }, I32Const { value: 1 }, I32Add])
    }

    fn reject(&mut self, constant: i32, comparison: Operator<'a>) -> Option<()> {
        self.expect_all(&[
            Operator::I32Const { value: constant },
            comparison,
            Operator::BrIf { relative_depth: 1 },
        ])
    }

    // Optional clears occur after the sole result is on the stack. Validation
    // still checks their types, including the heap type of each ref.null.
    fn trailing_clears(&mut self) -> Option<()> {
        use Operator::*;
        while matches!(
            self.0.peek(),
            Some(Ok(I32Const { value: 0 }
                | I64Const { value: 0 }
                | RefNull { .. }))
        ) {
            self.next()?;
            if !matches!(self.next()?, LocalSet { .. }) {
                return None;
            }
        }
        Some(())
    }
}

impl PackedKernel {
    fn recognize(env: &mut FuncEnvironment<'_>, body: &FunctionBody<'_>) -> Option<Self> {
        use KernelKind::*;
        use Operator::*;
        use WasmValType::{I32, Ref};

        if env.wasm_func_ty.results() != [I32] {
            return None;
        }
        let params = env.wasm_func_ty.params();
        let (source_ref, other_ref, equal) = match *params {
            [Ref(source), I32, I32] | [Ref(source), I32, I32, I32] => (source, None, false),
            [Ref(source), I32, I32, Ref(target), I32] => (source, Some(target), false),
            [Ref(left), Ref(right), I32] => (left, Some(right), true),
            _ => return None,
        };
        if !matches!(source_ref.heap_type, WasmHeapType::ConcreteArray(_))
            || other_ref.is_some_and(|reference| {
                !matches!(reference.heap_type, WasmHeapType::ConcreteArray(_))
            })
        {
            return None;
        }
        let end = 2;
        let param_count = u32::try_from(params.len()).ok()?;
        let mut shape = Shape(body.get_operators_reader().ok()?.into_iter().peekable());
        let index = if equal {
            shape.expect(I32Const { value: 0 })?;
            let LocalSet { local_index } = shape.next()? else {
                return None;
            };
            local_index
        } else {
            1
        };
        shape.expect_all(&[
            Block {
                blockty: BlockType::Empty,
            },
            LocalGet { local_index: index },
            LocalGet { local_index: end },
            I32GeU,
            BrIf { relative_depth: 0 },
            Loop {
                blockty: BlockType::Empty,
            },
        ])?;
        let source_type = shape.array_get(0, source_ref, index)?;

        let mut other_type = None;
        let mut scratch = None;
        let kind = if let Some(other_ref) = other_ref {
            if equal {
                other_type = Some(shape.array_get(1, other_ref, index)?);
                shape.expect_all(&[I32Ne, BrIf { relative_depth: 1 }])?;
                scratch = Some(index);
                CodeUnitEqual
            } else {
                let unit = shape.store_and_load()?;
                scratch = Some(unit);
                let kind = if matches!(shape.0.peek(), Some(Ok(I32Const { value: 0x80 }))) {
                    shape.reject(0x80, I32GeU)?;
                    // The packed storage types below distinguish these two.
                    AsciiNarrow
                } else {
                    // For u16 this unsigned subtraction selects controls and
                    // units at or above the surrogate range. The nested
                    // comparison keeps U+E000 through U+FFFF.
                    shape.expect_all(&[
                        I32Const { value: 0x20 },
                        I32Sub,
                        I32Const {
                            value: 0xd800 - 0x20,
                        },
                        I32GeU,
                        If {
                            blockty: BlockType::Empty,
                        },
                        LocalGet { local_index: unit },
                        I32Const { value: 0xe000 },
                        I32LtU,
                        BrIf { relative_depth: 2 },
                        End,
                    ])?;
                    shape.expect(LocalGet { local_index: unit })?;
                    shape.reject(0x22, I32Eq)?;
                    shape.expect(LocalGet { local_index: unit })?;
                    shape.reject(0x5c, I32Eq)?;
                    JsonPlainRun
                };
                shape.reference(3, other_ref)?;
                shape.expect_all(&[LocalGet { local_index: 4 }, LocalGet { local_index: unit }])?;
                let ArraySet { array_type_index } = shape.next()? else {
                    return None;
                };
                other_type = Some(array_type_index);
                kind
            }
        } else if param_count == 4 {
            shape.expect_all(&[
                LocalGet { local_index: 3 },
                I32Eq,
                BrIf { relative_depth: 1 },
            ])?;
            CodeUnitFind
        } else {
            let unit = shape.store_and_load()?;
            scratch = Some(unit);
            shape.reject(0x20, I32LtU)?;
            shape.expect(LocalGet { local_index: unit })?;
            shape.reject(0x22, I32Eq)?;
            shape.expect(LocalGet { local_index: unit })?;
            shape.reject(0x5c, I32Eq)?;
            JsonParsePlainRun
        };
        shape.increment_value(index)?;
        if kind.is_copy() {
            shape.expect(LocalSet { local_index: index })?;
            shape.increment_value(4)?;
            shape.expect(LocalSet { local_index: 4 })?;
            shape.expect(LocalGet { local_index: index })?;
        } else if shape.store_and_load()? != index {
            return None;
        }
        shape.expect_all(&[
            LocalGet { local_index: end },
            I32LtU,
            BrIf { relative_depth: 0 },
            End,
            End,
            LocalGet { local_index: index },
        ])?;
        if equal {
            shape.expect_all(&[LocalGet { local_index: end }, I32Eq])?;
        }
        shape.trailing_clears()?;
        shape.expect(End)?;
        if shape.0.next().is_some() {
            return None;
        }
        if let Some(scratch) = scratch {
            let mut remaining = scratch.checked_sub(param_count)?;
            let mut found = false;
            for decl in body.get_locals_reader().ok()? {
                let (count, ty) = decl.ok()?;
                if remaining < count {
                    found = ty == wasmparser::ValType::I32;
                    break;
                }
                remaining -= count;
            }
            if !found {
                return None;
            }
        }

        let source = PackedArray::match_operand(env, source_ref, source_type)?;
        let other = match (other_ref, other_type) {
            (Some(reference), Some(ty)) => Some(PackedArray::match_operand(env, reference, ty)?),
            (None, None) => None,
            _ => return None,
        };
        let kind = match (kind, source.elem_size, other.map(|array| array.elem_size)) {
            (AsciiNarrow, 1, Some(2)) => AsciiWiden,
            (AsciiNarrow, 2, Some(1)) => AsciiNarrow,
            (JsonPlainRun | CodeUnitEqual, 2, Some(2))
            | (CodeUnitFind | JsonParsePlainRun, 2, None) => kind,
            _ => return None,
        };
        Some(Self {
            kind,
            source,
            other,
        })
    }
}

impl FuncEnvironment<'_> {
    pub(crate) fn try_translate_packed_kernel(
        &mut self,
        validator: &mut FuncValidator<impl WasmModuleResources>,
        body: &FunctionBody<'_>,
        builder: &mut FunctionBuilder<'_>,
    ) -> WasmResult<bool> {
        let triple = self.isa.triple();
        // SSE2 is baseline for x86-64. Cranelift's little-endian AArch64
        // backend uses baseline Advanced SIMD (NEON).
        if triple.is_pulley()
            || !matches!(
                triple.architecture,
                Architecture::X86_64 | Architecture::Aarch64(Aarch64Architecture::Aarch64)
            )
            || self.pointer_type() != I64
            || self.tunables.collector != Some(Collector::Copying)
            || self.tunables.consume_fuel
            || self.tunables.epoch_interruption
            || self.tunables.debug_guest
            || self.tunables.debug_native
            || self.state_slot.is_some()
            || self.compiler.wmemcheck
            || cfg!(gc_zeal)
        {
            return Ok(false);
        }

        // Probe the shape without consuming validator state. Any unmatched body
        // keeps the usual interleaved validation and scalar translation.
        let Some(kernel) = PackedKernel::recognize(self, body) else {
            return Ok(false);
        };
        validator.validate(body)?;
        self.before_translate_function(builder)?;
        kernel.lower(self, builder)?;
        self.after_translate_function(builder)?;
        Ok(true)
    }
}

/// The Wasm positions at the next scalar or vector iteration.
#[derive(Clone, Copy)]
struct Position {
    start: ir::Value,
    at: ir::Value,
}

impl Position {
    fn args(self) -> [ir::BlockArg; 2] {
        [self.start.into(), self.at.into()]
    }

    fn advance(self, builder: &mut FunctionBuilder<'_>, count: i64) -> Self {
        Self {
            start: builder.ins().iadd_imm_s(self.start, count),
            at: builder.ins().iadd_imm_s(self.at, count),
        }
    }
}

#[derive(Clone, Copy)]
struct Span {
    array: PackedArray,
    reference: ir::Value,
}

impl Span {
    fn len(
        self,
        env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
    ) -> WasmResult<ir::Value> {
        // The vector dispatch has established non-null on this path. Do not
        // record a non-null proof that the scalar path could inherit.
        translate_array_len(env, builder, self.reference, true)
    }

    fn checked_start(
        self,
        env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        index: ir::Value,
        count: ir::Value,
    ) -> WasmResult<ir::Value> {
        // The span passed the Wasm length checks in the dispatch. Widen before
        // scaling and adding the 32-bit reference, then check the native
        // address and the whole byte range against the live GC heap.
        let reference = builder.ins().uextend(I64, self.reference);
        let index = builder.ins().uextend(I64, index);
        let offset = builder.ins().imul_imm_s(index, self.array.elem_size);
        let offset = builder.ins().iadd_imm_s(offset, self.array.base_size);
        let offset = builder.ins().iadd(reference, offset);
        let base = env.get_gc_heap_base(builder)?;
        let addr = env.uadd_overflow_trap(builder, base, offset, TRAP_GC_HEAP_CORRUPT);
        let count = builder.ins().uextend(I64, count);
        let bytes = builder.ins().imul_imm_s(count, self.array.elem_size);
        env.emit_defensive_array_bounds_check(builder, addr, bytes)?;
        Ok(addr)
    }

    fn get(
        self,
        env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
        index: ir::Value,
    ) -> WasmResult<ir::Value> {
        translate_array_get(
            env,
            builder,
            self.array.ty,
            self.reference,
            !self.array.nullable,
            index,
            Some(Extension::Zero),
        )
    }
}

impl PackedKernel {
    fn lower(
        &self,
        env: &mut FuncEnvironment<'_>,
        builder: &mut FunctionBuilder<'_>,
    ) -> WasmResult<()> {
        use KernelKind::*;

        let entry = builder.func.layout.entry_block().unwrap();
        let params = builder.block_params(entry);
        let source = Span {
            array: self.source,
            reference: params[2],
        };
        let other = self.other.map(|array| Span {
            array,
            reference: params[if self.kind == CodeUnitEqual { 3 } else { 5 }],
        });
        let arg1 = params[3];
        let end = params[4];
        let needle = (self.kind == CodeUnitFind).then(|| params[5]);
        let at = self.kind.is_copy().then(|| params[6]);

        let zero = builder.ins().iconst(I32, 0);
        let equality = self.kind == CodeUnitEqual;
        let initial = Position {
            start: if equality { zero } else { arg1 },
            at: at.unwrap_or(zero),
        };
        let scalar = builder.create_block();
        let tail = Position {
            start: builder.append_block_param(scalar, I32),
            at: builder.append_block_param(scalar, I32),
        };
        let done = builder.create_block();
        let result = builder.append_block_param(done, I32);
        let exhausted = if equality {
            builder.ins().iconst(I32, 1)
        } else {
            initial.start
        };

        let dispatch = builder.create_block();
        let empty = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, initial.start, end);
        builder
            .ins()
            .brif(empty, done, &[exhausted.into()], dispatch, &[]);
        builder.switch_to_block(dispatch);

        // Falling back before reading either length defers all nullable
        // accesses to their original order, including a skipped destination
        // write after a rejecting source element.
        for span in std::iter::once(source).chain(other) {
            if span.array.nullable {
                let non_null = builder.create_block();
                builder
                    .ins()
                    .brif(span.reference, non_null, &[], scalar, &initial.args());
                builder.switch_to_block(non_null);
            }
        }
        if self.kind == JsonPlainRun {
            let distinct = builder.create_block();
            let alias =
                builder
                    .ins()
                    .icmp(IntCC::Equal, source.reference, other.unwrap().reference);
            builder
                .ins()
                .brif(alias, scalar, &initial.args(), distinct, &[]);
            builder.switch_to_block(distinct);
        }
        if let Some(needle) = needle {
            let packed = builder.create_block();
            let too_wide = builder
                .ins()
                .icmp_imm_u(IntCC::UnsignedGreaterThan, needle, 0xffff);
            builder
                .ins()
                .brif(too_wide, scalar, &initial.args(), packed, &[]);
            builder.switch_to_block(packed);
        }

        let remaining = builder.ins().isub(end, initial.start);
        let len = source.len(env, builder)?;
        let available = available_from(builder, len, initial.start);
        let mut count = builder.ins().umin(remaining, available);
        if let Some(other) = other {
            let len = other.len(env, builder)?;
            let available = available_from(builder, len, initial.at);
            count = builder.ins().umin(count, available);
        }
        let vector = builder.create_block();
        let enough = builder
            .ins()
            .icmp_imm_u(IntCC::UnsignedGreaterThanOrEqual, count, 8);
        builder
            .ins()
            .brif(enough, vector, &[], scalar, &initial.args());
        builder.switch_to_block(vector);
        let src = source.checked_start(env, builder, initial.start, count)?;
        let dst = if let Some(other) = other {
            other.checked_start(env, builder, initial.at, count)?
        } else {
            builder.ins().iconst(I64, 0)
        };
        let gc_region = env.alias_regions.gc_heap_region(builder.func);
        let flags = little_endian()
            .with_notrap()
            .with_alias_region(Some(gc_region));

        // No calls or polls can occur between the bounds checks and any of
        // these raw accesses. Terminal trap paths never rejoin the loop.
        VectorLoop {
            kind: self.kind,
            src_size: source.array.elem_size,
            dst_size: other.map_or(0, |span| span.array.elem_size),
            flags,
            scalar,
            done,
        }
        .emit(
            builder,
            VectorPosition {
                src,
                dst,
                position: initial,
                remaining: count,
            },
            needle,
            zero,
        );

        builder.switch_to_block(scalar);
        let read = builder.create_block();
        let finished = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, tail.start, end);
        let exhausted = if equality { exhausted } else { tail.start };
        builder
            .ins()
            .brif(finished, done, &[exhausted.into()], read, &[]);
        builder.switch_to_block(read);
        let unit = source.get(env, builder, tail.start)?;
        let stop = match self.kind {
            AsciiNarrow | AsciiWiden => {
                builder
                    .ins()
                    .icmp_imm_u(IntCC::UnsignedGreaterThanOrEqual, unit, 0x80)
            }
            CodeUnitFind => builder.ins().icmp(IntCC::Equal, unit, needle.unwrap()),
            JsonPlainRun | JsonParsePlainRun => {
                let control = builder.ins().icmp_imm_u(IntCC::UnsignedLessThan, unit, 0x20);
                let quote = builder.ins().icmp_imm_u(IntCC::Equal, unit, 0x22);
                let slash = builder.ins().icmp_imm_u(IntCC::Equal, unit, 0x5c);
                let punctuation = builder.ins().bor(quote, slash);
                let bad = builder.ins().bor(control, punctuation);
                if self.kind == JsonPlainRun {
                    let range = builder.ins().band_imm_u(unit, 0xf800);
                    let surrogate = builder.ins().icmp_imm_u(IntCC::Equal, range, 0xd800);
                    builder.ins().bor(bad, surrogate)
                } else {
                    bad
                }
            }
            CodeUnitEqual => {
                let right = other.unwrap().get(env, builder, tail.start)?;
                builder.ins().icmp(IntCC::NotEqual, unit, right)
            }
        };
        let next = builder.create_block();
        let rejected = if equality { zero } else { tail.start };
        builder
            .ins()
            .brif(stop, done, &[rejected.into()], next, &[]);
        builder.switch_to_block(next);
        if self.kind.is_copy() {
            let target = other.unwrap();
            translate_array_set(
                env,
                builder,
                target.array.ty,
                target.reference,
                !target.array.nullable,
                tail.at,
                unit,
                None,
            )?;
        }
        let next = tail.advance(builder, 1);
        builder.ins().jump(scalar, &next.args());

        builder.switch_to_block(done);
        env.handle_before_return(&[result], builder);
        builder.ins().return_(&[result]);
        builder.seal_all_blocks();
        Ok(())
    }
}

fn available_from(
    builder: &mut FunctionBuilder<'_>,
    len: ir::Value,
    start: ir::Value,
) -> ir::Value {
    let end = builder.ins().umax(len, start);
    builder.ins().isub(end, start)
}

fn little_endian() -> ir::MemFlagsData {
    ir::MemFlagsData::new().with_endianness(Endianness::Little)
}

fn splat_i16(builder: &mut FunctionBuilder<'_>, value: u16) -> ir::Value {
    let data = value.to_le_bytes().repeat(8);
    let constant = builder.func.dfg.constants.insert(data.into());
    builder.ins().vconst(I16X8, constant)
}

fn code_unit_mask(builder: &mut FunctionBuilder<'_>, vector: ir::Value) -> ir::Value {
    let bytes = builder.ins().bitcast(I8X16, little_endian(), vector);
    builder.ins().vhigh_bits(I32, bytes)
}

#[derive(Clone, Copy)]
struct VectorPosition {
    src: ir::Value,
    dst: ir::Value,
    position: Position,
    remaining: ir::Value,
}

impl VectorPosition {
    fn block_params(builder: &mut FunctionBuilder<'_>, block: ir::Block) -> Self {
        Self {
            src: builder.append_block_param(block, I64),
            dst: builder.append_block_param(block, I64),
            position: Position {
                start: builder.append_block_param(block, I32),
                at: builder.append_block_param(block, I32),
            },
            remaining: builder.append_block_param(block, I32),
        }
    }

    fn args(self) -> [ir::BlockArg; 5] {
        [
            self.src.into(),
            self.dst.into(),
            self.position.start.into(),
            self.position.at.into(),
            self.remaining.into(),
        ]
    }
}

struct VectorLoop {
    kind: KernelKind,
    src_size: i64,
    dst_size: i64,
    flags: ir::MemFlagsData,
    scalar: ir::Block,
    done: ir::Block,
}

impl VectorLoop {
    fn emit(
        &self,
        builder: &mut FunctionBuilder<'_>,
        initial: VectorPosition,
        needle: Option<ir::Value>,
        zero: ir::Value,
    ) {
        use KernelKind::*;

        let needle = needle.map(|needle| {
            let unit = builder.ins().ireduce(I16, needle);
            builder.ins().splat(I16X8, unit)
        });
        let zero_vec = splat_i16(builder, 0);
        let width = if self.kind.is_ascii_copy() { 16 } else { 8 };
        let header = builder.create_block();
        let current = VectorPosition::block_params(builder, header);
        let chunk = builder.create_block();
        let ascii_tail = if self.kind.is_ascii_copy() {
            Some(builder.create_block())
        } else {
            None
        };
        builder.ins().jump(header, &initial.args());

        builder.switch_to_block(header);
        let enough = builder.ins().icmp_imm_u(
            IntCC::UnsignedGreaterThanOrEqual,
            current.remaining,
            width,
        );
        let scalar_args = current.position.args();
        let (tail, tail_args) = match ascii_tail {
            Some(block) => (block, &[][..]),
            None => (self.scalar, &scalar_args[..]),
        };
        builder.ins().brif(enough, chunk, &[], tail, tail_args);
        builder.switch_to_block(chunk);
        let pass = builder.create_block();
        match self.kind {
            AsciiNarrow => {
                let lo = builder.ins().load(I16X8, self.flags, current.src, 0);
                let hi = builder.ins().load(I16X8, self.flags, current.src, 16);
                let units = builder.ins().bor(lo, hi);
                self.test_ascii_units(builder, units, pass, tail, tail_args);
                builder.switch_to_block(pass);
                let bytes = builder.ins().unarrow(lo, hi);
                builder.ins().store(self.flags, bytes, current.dst, 0);
            }
            AsciiWiden => {
                let bytes = builder.ins().load(I8X16, self.flags, current.src, 0);
                let mask = builder.ins().vhigh_bits(I32, bytes);
                builder.ins().brif(mask, tail, tail_args, pass, &[]);
                builder.switch_to_block(pass);
                let lo = builder.ins().uwiden_low(bytes);
                let hi = builder.ins().uwiden_high(bytes);
                builder.ins().store(self.flags, lo, current.dst, 0);
                builder.ins().store(self.flags, hi, current.dst, 16);
            }
            CodeUnitEqual => {
                let left = builder.ins().load(I8X16, self.flags, current.src, 0);
                let right = builder.ins().load(I8X16, self.flags, current.dst, 0);
                let eq = builder.ins().icmp(IntCC::Equal, left, right);
                let mask = builder.ins().vhigh_bits(I32, eq);
                let mismatch = builder.ins().icmp_imm_u(IntCC::NotEqual, mask, 0xffff);
                builder
                    .ins()
                    .brif(mismatch, self.done, &[zero.into()], pass, &[]);
                builder.switch_to_block(pass);
            }
            JsonPlainRun | JsonParsePlainRun | CodeUnitFind => {
                let units = builder.ins().load(I16X8, self.flags, current.src, 0);
                let stop = if self.kind == CodeUnitFind {
                    builder.ins().icmp(IntCC::Equal, units, needle.unwrap())
                } else {
                    self.json_stop_lanes(builder, units, zero_vec)
                };
                let mask = code_unit_mask(builder, stop);
                if self.kind == JsonPlainRun {
                    builder.ins().brif(mask, tail, tail_args, pass, &[]);
                    builder.switch_to_block(pass);
                    builder.ins().store(self.flags, units, current.dst, 0);
                } else {
                    let found = builder.create_block();
                    builder.ins().brif(mask, found, &[], pass, &[]);
                    builder.switch_to_block(found);
                    let first_byte = builder.ins().ctz(mask);
                    let lane = builder.ins().ushr_imm_u(first_byte, 1);
                    let index = builder.ins().iadd(current.position.start, lane);
                    builder.ins().jump(self.done, &[index.into()]);
                    builder.switch_to_block(pass);
                }
            }
        }
        let next = VectorPosition {
            src: builder.ins().iadd_imm_s(current.src, width * self.src_size),
            dst: builder.ins().iadd_imm_s(current.dst, width * self.dst_size),
            position: current.position.advance(builder, width),
            remaining: builder.ins().iadd_imm_s(current.remaining, -width),
        };
        builder.ins().jump(header, &next.args());

        if let Some(ascii_tail) = ascii_tail {
            builder.switch_to_block(ascii_tail);
            self.emit_ascii_tail(builder, current, zero_vec);
        }
    }

    fn test_ascii_units(
        &self,
        builder: &mut FunctionBuilder<'_>,
        units: ir::Value,
        pass: ir::Block,
        tail: ir::Block,
        tail_args: &[ir::BlockArg],
    ) {
        let high = splat_i16(builder, 0xff80);
        let high = builder.ins().band(units, high);
        let bad = builder.ins().vany_true(high);
        builder.ins().brif(bad, tail, tail_args, pass, &[]);
    }

    fn json_stop_lanes(
        &self,
        builder: &mut FunctionBuilder<'_>,
        units: ir::Value,
        zero: ir::Value,
    ) -> ir::Value {
        let high_mask = splat_i16(builder, 0xffe0);
        let high = builder.ins().band(units, high_mask);
        let control = builder.ins().icmp(IntCC::Equal, high, zero);
        let quote = splat_i16(builder, 0x22);
        let quote = builder.ins().icmp(IntCC::Equal, units, quote);
        let slash = splat_i16(builder, 0x5c);
        let slash = builder.ins().icmp(IntCC::Equal, units, slash);
        let punctuation = builder.ins().bor(quote, slash);
        let bad = builder.ins().bor(control, punctuation);
        if self.kind == KernelKind::JsonPlainRun {
            let mask = splat_i16(builder, 0xf800);
            let range = builder.ins().band(units, mask);
            let surrogate = splat_i16(builder, 0xd800);
            let surrogate = builder.ins().icmp(IntCC::Equal, range, surrogate);
            builder.ins().bor(bad, surrogate)
        } else {
            bad
        }
    }

    fn emit_ascii_tail(
        &self,
        builder: &mut FunctionBuilder<'_>,
        current: VectorPosition,
        zero: ir::Value,
    ) {
        let chunk = builder.create_block();
        let pass = builder.create_block();
        let scalar_args = current.position.args();
        let enough =
            builder
                .ins()
                .icmp_imm_u(IntCC::UnsignedGreaterThanOrEqual, current.remaining, 8);
        builder
            .ins()
            .brif(enough, chunk, &[], self.scalar, &scalar_args);
        builder.switch_to_block(chunk);

        if self.kind == KernelKind::AsciiNarrow {
            let units = builder.ins().load(I16X8, self.flags, current.src, 0);
            self.test_ascii_units(builder, units, pass, self.scalar, &scalar_args);
            builder.switch_to_block(pass);
            let bytes = builder.ins().unarrow(units, zero);
            let lanes = builder.ins().bitcast(I64X2, little_endian(), bytes);
            let low = builder.ins().extractlane(lanes, 0);
            builder.ins().store(self.flags, low, current.dst, 0);
        } else {
            let bytes = builder.ins().load(I64, self.flags, current.src, 0);
            let mask = builder
                .ins()
                .band_imm_s(bytes, 0x8080_8080_8080_8080u64 as i64);
            builder
                .ins()
                .brif(mask, self.scalar, &scalar_args, pass, &[]);
            builder.switch_to_block(pass);
            let lanes = builder.ins().scalar_to_vector(I64X2, bytes);
            let lanes = builder.ins().bitcast(I8X16, little_endian(), lanes);
            let units = builder.ins().uwiden_low(lanes);
            builder.ins().store(self.flags, units, current.dst, 0);
        }
        let next = current.position.advance(builder, 8);
        builder.ins().jump(self.scalar, &next.args());
    }
}
