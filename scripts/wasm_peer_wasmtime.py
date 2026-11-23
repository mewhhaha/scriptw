"""Benchmark harness only: run one Porffor-compiled Wasm program (https://porffor.dev) under Wasmtime.

usage: PYTHON -I wasm_peer_wasmtime.py PROGRAM.wasm EXPECTED_STDOUT SAMPLES WARMUPS

PYTHON is an isolated interpreter with the pinned wasmtime-py package; the
benchmark driver checks its version. The Engine mirrors the shared Scriptw
host's native tier (Cranelift SpeedAndSize, backtracking register allocation,
an explicit x86_64-unknown-linux-gnu target without host CPU feature
inference, serial compilation) and its fresh-Store allocation (a one-slot
pool that keeps 16 MiB of linear memory resident between Stores, 1 GiB Store
memory cap). The program is compiled once per process before any entry.

Each entry gets a fresh Store and Instance before the timer; the timer covers
only the exported main call. Porffor's character output import is a Wasm
function of a small adapter module in the same Store, so an entry makes no
host call for its output; numeric printing and clocks call back into Python
and are counted. `empty_call_ns` records this embedding's call floor: the same
timer around an exported Wasm function that returns at once.
"""
import ctypes
import hashlib
from importlib import metadata
import json
import math
import sys
import time

import wasmtime
from wasmtime import _ffi as ffi

ADAPTER_WAT = r'''
(module
  (import "host" "print" (func $host_print (param f64)))
  (memory (export "out") 4)
  (global $length (export "length") (mut i32) (i32.const 0))
  (global $numeric (export "numeric") (mut i32) (i32.const 0))
  (func (export "printChar") (param f64)
    (if (i32.ge_u (global.get $length) (i32.const 262144)) (then unreachable))
    (i32.store16 (global.get $length) (i32.trunc_sat_f64_u (local.get 0)))
    (global.set $length (i32.add (global.get $length) (i32.const 2))))
  (func (export "print") (param f64)
    (global.set $numeric (i32.add (global.get $numeric) (i32.const 1)))
    (call $host_print (local.get 0)))
  (func (export "empty")))
'''
STORE_MEMORY_BYTES = 1024**3
KEEP_RESIDENT_BYTES = 16 * 1024**2
MEMORY_SLOTS = 3
CALL = ffi.dll.wasmtime_func_call_unchecked
CALL.restype = ctypes.POINTER(ffi.wasmtime_error_t)


def js_number(value):
    """Number::toString for the values a numeric print import receives."""
    if math.isnan(value):
        return 'NaN'
    if math.isinf(value):
        return 'Infinity' if value > 0 else '-Infinity'
    if value == int(value) and abs(value) < 1e21:
        return str(int(value))
    text = repr(value)
    mantissa, _, exponent = text.partition('e')
    if not exponent:
        return text
    sign = '-' if exponent.startswith('-') else '+'
    return f'{mantissa}e{sign}{exponent.lstrip("+-").lstrip("0") or "0"}'


def engine():
    config = wasmtime.Config()
    # Porffor's Wasm throws through an exception tag; Wasmtime allocates exception
    # objects on a Store's GC heap, so exceptions need GC support.
    config.wasm_exceptions = True
    config.wasm_gc = True
    config.cranelift_opt_level = 'speed_and_size'
    ffi.wasmtime_config_cranelift_regalloc_algorithm_set(config.ptr(), 0)  # backtracking
    config.target = 'x86_64-unknown-linux-gnu'
    config.parallel_compilation = False
    pool = ffi.wasmtime_pooling_allocation_config_new()
    # One Store at a time: the program's linear memory, the adapter's memory and
    # the GC heap, two core instances; every slot it returns stays warm.
    ffi.wasmtime_pooling_allocation_config_total_memories_set(pool, MEMORY_SLOTS)
    ffi.wasmtime_pooling_allocation_config_max_memories_per_module_set(pool, 1)
    ffi.wasmtime_pooling_allocation_config_total_gc_heaps_set(pool, 1)
    ffi.wasmtime_pooling_allocation_config_total_core_instances_set(pool, 2)
    ffi.wasmtime_pooling_allocation_config_total_tables_set(pool, 2)
    ffi.wasmtime_pooling_allocation_config_max_tables_per_module_set(pool, 1)
    ffi.wasmtime_pooling_allocation_config_max_memory_size_set(pool, STORE_MEMORY_BYTES)
    ffi.wasmtime_pooling_allocation_config_max_unused_warm_slots_set(pool, MEMORY_SLOTS)
    ffi.wasmtime_pooling_allocation_config_linear_memory_keep_resident_set(pool, KEEP_RESIDENT_BYTES)
    ffi.wasmtime_pooling_allocation_strategy_set(config.ptr(), pool)
    ffi.wasmtime_pooling_allocation_config_delete(pool)
    return wasmtime.Engine(config)


def main(argv):
    program, expected, samples, warmups = argv[1], argv[2], int(argv[3]), int(argv[4])
    if not (3 <= samples <= 100 and 1 <= warmups <= 1000):
        raise SystemExit('samples must be 3..100 and warmups 1..1000')
    data = open(program, 'rb').read()
    report = dict(schema='scriptw-wasm-peer-wasmtime-v1', engine='wasmtime-py', wasmtime_py=metadata.version('wasmtime'), wasm_sha256=hashlib.sha256(data).hexdigest(),
                  wasm_bytes=len(data), samples=samples, warmups=warmups,
                  configuration=dict(cranelift_opt_level='SpeedAndSize', register_allocation='backtracking',
                                     target='x86_64-unknown-linux-gnu', parallel_compilation=False,
                                     wasm_exceptions=True, wasm_gc=True,
                                     allocation='pooling: one Store; 3 warm memory slots keep 16 MiB resident each',
                                     store_memory_limit_bytes=STORE_MEMORY_BYTES),
                  observations=[])
    started = time.perf_counter_ns()
    engine_ = engine()
    report['engine_ns'] = time.perf_counter_ns() - started
    try:
        started = time.perf_counter_ns()
        module = wasmtime.Module(engine_, data)
        report['module_compilation_ns'] = time.perf_counter_ns() - started
    except wasmtime.WasmtimeError as error:
        report.update(status='module-rejected', error=str(error).splitlines()[0])
        print(json.dumps(report))
        return
    adapter = wasmtime.Module(engine_, wasmtime.wat2wasm(ADAPTER_WAT))
    names = [(entry.module, entry.name) for entry in module.imports]
    report['imports'] = [f'{module_name}.{name}' for module_name, name in names]
    exports = {entry.name for entry in module.exports}
    if any(module_name != '' or name not in 'abcd' for module_name, name in names) or 'm' not in exports:
        report['status'] = 'unsupported-interface'
        print(json.dumps(report))
        return
    report['status'] = 'complete'
    origin_ns = time.perf_counter_ns()
    origin_ms = time.time() * 1000.0
    unary = wasmtime.FuncType([wasmtime.ValType.f64()], [])
    clock = wasmtime.FuncType([], [wasmtime.ValType.f64()])
    for index in range(warmups + samples):
        numeric_text = []
        calls = dict(clock=0)
        store = wasmtime.Store(engine_)
        store.set_limits(memory_size=STORE_MEMORY_BYTES)
        started = time.perf_counter_ns()
        host_print = wasmtime.Func(store, unary, lambda value: numeric_text.append(js_number(value)))
        helper = wasmtime.Instance(store, adapter, [host_print])
        helper_exports = helper.exports(store)

        def now():
            calls['clock'] += 1
            return (time.perf_counter_ns() - origin_ns) / 1e6

        def time_origin():
            calls['clock'] += 1
            return origin_ms
        provided = dict(a=helper_exports['print'], b=helper_exports['printChar'],
                        c=wasmtime.Func(store, clock, now), d=wasmtime.Func(store, clock, time_origin))
        instance = wasmtime.Instance(store, module, [provided[name] for _, name in names])
        entry = instance.exports(store)['m']
        slots = max(len(entry.type(store).params), len(entry.type(store).results), 1)
        context, values = store._context(), (ffi.wasmtime_val_raw_t * slots)()
        trap = ctypes.POINTER(ffi.wasm_trap_t)()
        instantiation_ns = time.perf_counter_ns() - started
        # The unchecked ctypes call is the cheapest entry this embedding offers;
        # the same call of an empty function measures what remains of it.
        empty, empty_values = helper_exports['empty'], (ffi.wasmtime_val_raw_t * 1)()
        begin = time.perf_counter_ns()
        failure = CALL(context, ctypes.byref(empty._func), empty_values, 1, ctypes.byref(trap))
        empty_call_ns = time.perf_counter_ns() - begin
        if failure or trap:
            raise SystemExit('the empty adapter call failed')
        begin = time.perf_counter_ns()
        failure = CALL(context, ctypes.byref(entry._func), values, slots, ctypes.byref(trap))
        execution_ns = time.perf_counter_ns() - begin
        completion, error = 'normal', ''
        if trap or failure:
            text = str(wasmtime.Trap._from_ptr(trap) if trap else wasmtime.WasmtimeError._from_ptr(failure))
            # An uncaught Porffor exception leaves Wasm as a thrown exception, not a trap.
            completion = 'throw' if 'thrown Wasm exception' in text else 'trap'
            error = [line.strip() for line in text.splitlines() if line.strip()][-1]
        numeric = helper_exports['numeric'].value(store)
        length = helper_exports['length'].value(store)
        units = helper_exports['out'].read(store, 0, length)
        # Character output and numeric output interleave only if both occur; the
        # driver's output check rejects any entry that mixes them.
        stdout = bytes(units).decode('utf-16-le', 'surrogatepass') if not numeric else ''.join(numeric_text)
        report['observations'].append(dict(
            index=index, warmup=index < warmups, execution_ns=execution_ns, instantiation_ns=instantiation_ns,
            empty_call_ns=empty_call_ns, stdout=stdout, numeric_print_calls=numeric, clock_calls=calls['clock'],
            mixed_output=bool(numeric and length), matched=completion == 'normal' and stdout == expected and not (numeric and length),
            completion=completion, error=error))
        del instance, helper, store
        if completion != 'normal':
            break
    print(json.dumps(report))


if __name__ == '__main__':
    main(sys.argv)
