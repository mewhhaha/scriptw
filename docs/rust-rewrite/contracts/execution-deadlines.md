# Execution deadlines

Compiled code carries no deadline checks. A deadline stops a Store from
outside, by making its compiled code fault. Wasmtime epoch interruption is
disabled for R and P; it cost 14–22% of retained RSC request time at every
function entry and loop header.

## Mechanism

`wasmtime::Store::interrupt_handle` returns a handle that any thread may use.
`InterruptHandle::interrupt` marks a request pending and removes access to the
Store's GC heap and to each linear memory registered through
`Store::interrupt_on_memory`. Engine registers the private linear memory.

Compiled code faults at its next access to that memory. The fault handler
restores access. When the faulting instruction is compiled code and a request
is pending, it consumes the request and unwinds with `Trap::Interrupt`, whether
or not the instruction is a registered trap site. Engine reports that trap as
`EngineExecutionFailure::Timeout`, as it did for an epoch deadline.

Host code inside the Store that reaches the memory first, such as the
collector, a host import or an embedder read, gets access back and continues.
A requester therefore repeats the request. A request still pending when the
host next enters compiled code is delivered at that entry.

A fault can land in the middle of an object's initialisation. That object's
reference lived only in the frames just unwound, and the copying collector
reaches objects from roots and never scans the active space, so nothing reads
it. The allocation pointer is outside the watched memory.

Each watched range is published in a fixed table of atomics before access is
removed, so a fault always finds it, and is cleared before the Store frees the
memory. Only memory that cannot relocate is watched: the product configuration
sets `gc_heap_may_move(false)` and `memory_may_move(false)`, and the linear
memory reservation already equals the Store limit.

The GC heap's first page is its null guard: compiled code lets an access
through a null reference fault there instead of testing the reference. A
request never publishes, removes or restores access to that page, so a fault
is told apart by its address: a null access traps on the null reference, as
the explicit test it replaces did, and a pending request is delivered at the
next watched access or entry.

## What compiled code must do

Code that can run without bound must reach watched memory. Two rules give
that:

- A source function stores one byte to private linear memory address
  `DEADLINE_POLL_ADDRESS` on entry, and a source loop stores it at every loop
  header. The address lies below `STATIC_DATA_OFFSET`, which nothing else
  reads or writes. This covers pure scalar loops and recursion. A function
  with an empty body has no store; it cannot loop or recurse.
- Runtime helpers and builtins carry no poll. Their loops are bounded by a heap
  value they read on each iteration, call into source code, or, for the regular
  expression compiler and matcher, run in the private linear memory.

Standalone modules have no linear memory and no polls; their admission accepts
no loop and no function.

A helper loop that touches neither the GC heap nor linear memory and makes no
call is not interruptible, so it must be bounded by construction. On ABI 57
the runtime module has 13,252 loops: 11,259 touch the GC heap, 1,407 more
reach linear memory, 13 more make a call, and 570 do none of these. Of the
570, 419 are fixed-width arithmetic in Temporal and 44 in Date; the rest
are decimal conversion, half-precision rounding and capacity doubling. A new
helper loop of that shape needs a bound or a poll. The retained RSC program
has 1,416 loops: 616 source loops carry the poll, and the other 800, generated
inside source functions, all touch the GC heap.

## Engine

`execution_deadline::ExecutionDeadline::arm` registers a Store's handle and
deadline with one process-wide thread. From the deadline until the guard is
dropped, that thread repeats the request every millisecond. Dropping the guard
removes the registration and then cancels the last request, so no request
follows the drop. The guard is declared after its Store and dropped before it.

The deadline covers compiled code only, as before. Host compilation, blocking
host imports and waits are bounded by their own checks of the execution
budget. After a deadline the Store's JavaScript state is mid-operation, as
after any trap.

Embedders that build their own Store own its handle and any deadline;
`configure_wasm_runtime` neither requests nor cancels an interrupt.

## Limits

Fault-based delivery needs Unix signal handling and has been exercised on
Linux x86-64. On other hosts only the entry check applies, and Engine rejects
a requested timeout there rather than accept one it cannot enforce.

Host code must copy watched memory in user space, as Engine does. A system
call given a pointer into protected memory fails with `EFAULT` instead of
faulting, so it would not get access back.

A debugger or profiler that stops on `SIGSEGV` sees one fault per delivered or
retried request. Nothing faults before a deadline.

## Verification

`vendor/wasmtime-47.0.0/tests/interrupt_handle.rs` covers a GC-heap loop, a
linear-memory loop, host code that reaches protected memory, a request made
while idle, cancellation, Store drop, concurrent Stores and a null access next
to a request. Engine's `aot_execution_deadlines` tests stop an empty loop, a
scalar loop, recursion and a generator loop through the poll; a sparse array
search through the GC heap; a backtracking regular expression through linear
memory; and a builtin that calls source code on every step. They also run a script within its budget
before and after a timeout. `wasm_backend_wasm_aot_deadline_bounds_infinite_loop`
and `scalar_chunks_keep_deadlines_between_bounded_groups` remain.

The loop counts above come from the Binaryen text of the runtime and program
modules: a loop counts as touching the GC heap when its body has a `struct.*`
or `array.*` instruction, and as reaching linear memory when it has a load, a
store, `memory.copy` or `memory.fill`.
