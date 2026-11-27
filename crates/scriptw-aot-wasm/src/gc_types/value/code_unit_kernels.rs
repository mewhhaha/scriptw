//! Leaf loops over UTF-16 code units and bytes.
//!
//! A codec or a search that runs one of these loops inside its own large
//! body pays for that body's spilled locals on every unit. Each loop here is
//! the whole body of a registered runtime helper whose only operands are its
//! arrays and a few indices, so its state stays in registers. Scans return
//! the first index they did not handle, or `end`. Equality returns whether
//! the scan reached the end of the range. A loop writes only inside the range
//! its caller reserved, and the engine's array bounds checks guard every access.
use super::*;

/// The scratch unit of one kernel loop.
struct KernelUnit(I32Local);

impl KernelUnit {
    fn reserve(schema: &RuntimeSchema, function: &mut Function) -> Self {
        Self(schema.reserve_i32_local(function))
    }
    fn release(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.0, function);
    }
}

fn increment(local: I32Local, function: &mut Function) {
    local.load(function);
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::I32Add);
    local.store(function);
}

/// Opens `block { loop {` for a nonempty `start..end`; an empty range skips
/// the loop. Inside the loop, depth 1 leaves it.
fn open_scan(start: I32Local, end: I32Local, function: &mut Function) {
    function.instruction(&Instruction::Block(BlockType::Empty));
    start.load(function);
    end.load(function);
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::BrIf(0));
    function.instruction(&Instruction::Loop(BlockType::Empty));
}

/// Continues the loop while `start` is below `end`, with the one
/// conditional branch an iteration pays for the loop itself, closes both
/// frames and leaves `start` on the stack.
fn close_scan(start: I32Local, end: I32Local, function: &mut Function) {
    start.load(function);
    end.load(function);
    function.instruction(&Instruction::I32LtU);
    function.instruction(&Instruction::BrIf(0));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    start.load(function);
}

/// Copies `units[start..end]` into `bytes[at..]` while each unit is ASCII,
/// one byte a unit, and leaves the index of the first unit not copied.
pub(crate) fn emit_ascii_narrow_kernel(
    units: &GcLocal<CodeUnitArray>,
    start: I32Local,
    end: I32Local,
    bytes: &GcLocal<ByteArray>,
    at: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let unit = KernelUnit::reserve(schema, function);
    open_scan(start, end, function);
    schema
        .array_type::<CodeUnitArray>()
        .read(units, start, schema, function)
        .store(unit.0, function);
    unit.0.load(function);
    function.instruction(&Instruction::I32Const(0x80));
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::BrIf(1));
    schema.array_type::<ByteArray>().write(
        bytes,
        at,
        GcOperand::i32_local(unit.0),
        schema,
        function,
    );
    increment(start, function);
    increment(at, function);
    close_scan(start, end, function);
    unit.release(schema, function);
}

/// Copies `bytes[start..end]` into `units[at..]` while each byte is ASCII,
/// one unit a byte, and leaves the index of the first byte not copied.
/// `units` is a backing still private to its construction.
fn emit_ascii_widen_kernel(
    bytes: &GcLocal<ByteArray>,
    start: I32Local,
    end: I32Local,
    units: &GcLocal<CodeUnitArray>,
    at: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let unit = KernelUnit::reserve(schema, function);
    open_scan(start, end, function);
    schema
        .array_type::<ByteArray>()
        .read(bytes, start, schema, function)
        .store(unit.0, function);
    unit.0.load(function);
    function.instruction(&Instruction::I32Const(0x80));
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::BrIf(1));
    // `array.set` checks the reference itself.
    units.access(function);
    at.load(function);
    unit.0.load(function);
    schema.array_type::<CodeUnitArray>().emit_set(function);
    increment(start, function);
    increment(at, function);
    close_scan(start, end, function);
    unit.release(schema, function);
}

/// Leaves the first index in `units[start..end]` that holds `unit`, or `end`.
pub(crate) fn emit_code_unit_find_kernel(
    units: &GcLocal<CodeUnitArray>,
    start: I32Local,
    end: I32Local,
    unit: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    open_scan(start, end, function);
    let _ = schema
        .array_type::<CodeUnitArray>()
        .read(units, start, schema, function);
    unit.load(function);
    function.instruction(&Instruction::I32Eq);
    function.instruction(&Instruction::BrIf(1));
    increment(start, function);
    close_scan(start, end, function);
}

/// Copies `source[start..end]` into `target[at..]` while QuoteJSONString
/// copies each unit as itself: at least U+0020, neither '"' nor '\\', and not
/// a surrogate. Leaves the index of the first unit not copied. `target` is an
/// accumulator backing, which no String aliases.
fn emit_json_plain_run_kernel(
    source: &GcLocal<CodeUnitArray>,
    start: I32Local,
    end: I32Local,
    target: &GcLocal<CodeUnitArray>,
    at: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let unit = KernelUnit::reserve(schema, function);
    open_scan(start, end, function);
    schema
        .array_type::<CodeUnitArray>()
        .read(source, start, schema, function)
        .store(unit.0, function);
    // U+0020 through U+D7FF holds every common unit; the units outside it
    // are controls, surrogates and the plain units from U+E000.
    unit.0.load(function);
    function.instruction(&Instruction::I32Const(0x20));
    function.instruction(&Instruction::I32Sub);
    function.instruction(&Instruction::I32Const(0xd800 - 0x20));
    function.instruction(&Instruction::I32GeU);
    function.instruction(&Instruction::If(BlockType::Empty));
    unit.0.load(function);
    function.instruction(&Instruction::I32Const(0xe000));
    function.instruction(&Instruction::I32LtU);
    function.instruction(&Instruction::BrIf(2));
    function.instruction(&Instruction::End);
    for escaped in [0x22, 0x5c] {
        unit.0.load(function);
        function.instruction(&Instruction::I32Const(escaped));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::BrIf(1));
    }
    // `array.set` checks the reference itself.
    target.access(function);
    at.load(function);
    unit.0.load(function);
    schema.array_type::<CodeUnitArray>().emit_set(function);
    increment(start, function);
    increment(at, function);
    close_scan(start, end, function);
    unit.release(schema, function);
}

/// Leaves the first control unit, quote, or backslash in `source[start..end]`,
/// or `end`. Surrogates are ordinary code units when parsing a JSON string.
pub(crate) fn emit_json_parse_plain_run_kernel(
    source: &GcLocal<CodeUnitArray>,
    start: I32Local,
    end: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let unit = KernelUnit::reserve(schema, function);
    open_scan(start, end, function);
    schema
        .array_type::<CodeUnitArray>()
        .read(source, start, schema, function)
        .store(unit.0, function);
    unit.0.load(function);
    function.instruction(&Instruction::I32Const(0x20));
    function.instruction(&Instruction::I32LtU);
    function.instruction(&Instruction::BrIf(1));
    for escaped in [0x22, 0x5c] {
        unit.0.load(function);
        function.instruction(&Instruction::I32Const(escaped));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::BrIf(1));
    }
    increment(start, function);
    close_scan(start, end, function);
    unit.release(schema, function);
}

/// Leaves `1` if the first `length` units match and `0` at a mismatch.
pub(crate) fn emit_code_unit_equal_kernel(
    left: &GcLocal<CodeUnitArray>,
    right: &GcLocal<CodeUnitArray>,
    length: I32Local,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    let index = schema.reserve_i32_local(function);
    function.instruction(&Instruction::I32Const(0));
    index.store(function);
    open_scan(index, length, function);
    let _ = schema
        .array_type::<CodeUnitArray>()
        .read(left, index, schema, function);
    let _ = schema
        .array_type::<CodeUnitArray>()
        .read(right, index, schema, function);
    function.instruction(&Instruction::I32Ne);
    function.instruction(&Instruction::BrIf(1));
    increment(index, function);
    close_scan(index, length, function);
    length.load(function);
    function.instruction(&Instruction::I32Eq);
    schema.release_i32_local(index, function);
}

/// The two kernels that write code units, as helper bodies. Their callers are
/// the construction and accumulator owners below, which alone hold a backing
/// no String aliases.
pub(crate) fn emit_ascii_widen_helper_body(
    parameters: &crate::runtime_helpers::AsciiWidenParameters,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    emit_ascii_widen_kernel(
        &parameters.bytes,
        parameters.start,
        parameters.end,
        &parameters.units,
        parameters.at,
        schema,
        function,
    );
}

pub(crate) fn emit_json_plain_run_helper_body(
    parameters: &crate::runtime_helpers::JsonPlainRunParameters,
    schema: &RuntimeSchema,
    function: &mut Function,
) {
    emit_json_plain_run_kernel(
        &parameters.source,
        parameters.start,
        parameters.end,
        &parameters.target,
        parameters.at,
        schema,
        function,
    );
}

impl StringConstruction {
    /// Writes `bytes[start..end]` at `written` while each byte is ASCII and
    /// advances both indices past the bytes written. The caller's backing
    /// has room for one unit a remaining byte.
    pub(crate) fn widen_ascii_from(
        &self,
        written: I32Local,
        bytes: &GcLocal<ByteArray>,
        start: I32Local,
        end: I32Local,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let stopped = schema.reserve_i32_local(function);
        schema
            .call_helper(
                crate::runtime_helpers::AsciiWidenArguments::new(
                    bytes,
                    start,
                    end,
                    &self.units,
                    written,
                ),
                base,
                function,
            )
            .store(stopped, function);
        advance(written, start, stopped, function);
        schema.release_i32_local(stopped, function);
    }
}

impl StringAccumulatorCursor {
    /// Appends `source[start..end]` while QuoteJSONString copies each unit as
    /// itself and advances `start` past the units appended. Room for one unit
    /// a remaining source unit is already reserved.
    pub(crate) fn append_json_plain_run(
        &self,
        source: &GcLocal<CodeUnitArray>,
        start: I32Local,
        end: I32Local,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let stopped = schema.reserve_i32_local(function);
        schema
            .call_helper(
                crate::runtime_helpers::JsonPlainRunArguments::new(
                    source,
                    start,
                    end,
                    self.units(),
                    self.length(),
                ),
                base,
                function,
            )
            .store(stopped, function);
        advance(self.length(), start, stopped, function);
        schema.release_i32_local(stopped, function);
    }
}

/// `written += stopped - start; start = stopped`.
fn advance(written: I32Local, start: I32Local, stopped: I32Local, function: &mut Function) {
    written.load(function);
    stopped.load(function);
    start.load(function);
    function.instruction(&Instruction::I32Sub);
    function.instruction(&Instruction::I32Add);
    written.store(function);
    stopped.load(function);
    start.store(function);
}
