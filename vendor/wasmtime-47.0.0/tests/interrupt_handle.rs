// A separate test file because it installs the process-wide signal handlers.

#![cfg(target_os = "linux")]
#![cfg(not(miri))]
#![cfg(has_native_signals)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use wasmtime::*;

const WAT: &str = r#"
(module
  (type $cell (struct (field (mut i32))))
  (import "" "touch" (func $touch (param (ref $cell)) (result i32)))
  (memory (export "memory") 1)
  (global $cell (mut (ref null $cell)) (ref.null $cell))

  (func (export "new_cell") (result (ref $cell))
    (global.set $cell (struct.new $cell (i32.const 7)))
    (ref.as_non_null (global.get $cell)))

  ;; Runs until interrupted; every iteration stores to the GC heap.
  (func (export "spin_heap")
    (local $cell (ref null $cell))
    (local.set $cell (struct.new $cell (i32.const 0)))
    (loop $again
      (struct.set $cell 0 (local.get $cell)
        (i32.add (struct.get $cell 0 (local.get $cell)) (i32.const 1)))
      (br $again)))

  ;; Runs until interrupted; touches only linear memory.
  (func (export "spin_memory")
    (loop $again
      (i32.store (i32.const 16) (i32.add (i32.load (i32.const 16)) (i32.const 1)))
      (br $again)))

  ;; Runs until interrupted; the host reads the GC heap on every iteration.
  (func (export "spin_host")
    (local $cell (ref null $cell))
    (local.set $cell (struct.new $cell (i32.const 5)))
    (loop $again
      (drop (call $touch (ref.as_non_null (local.get $cell))))
      (struct.set $cell 0 (local.get $cell) (i32.const 5))
      (br $again)))

  (func (export "sum") (param i32) (result i32)
    (local $cell (ref null $cell))
    (local.set $cell (struct.new $cell (local.get 0)))
    (i32.add (struct.get $cell 0 (local.get $cell)) (i32.load (i32.const 16))))
)
"#;

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.collector(Collector::Copying);
    // The product's shape: a reserved, unmoving GC heap behind guard pages.
    config.gc_heap_reservation(1 << 32);
    config.gc_heap_guard_size(32 << 20);
    config.gc_heap_may_move(false);
    config.memory_reservation(1 << 30);
    config.memory_may_move(false);
    Engine::new(&config).unwrap()
}

struct Fixture {
    store: Store<Arc<AtomicU32>>,
    instance: Instance,
}

fn fixture() -> Fixture {
    let engine = engine();
    let module = Module::new(&engine, WAT).unwrap();
    let mut store = Store::new(&engine, Arc::new(AtomicU32::new(0)));
    let touch_ty = module.imports().next().unwrap().ty().unwrap_func().clone();
    let touch = Func::new(&mut store, touch_ty, |mut caller, params, results| {
        // Reads the struct through the host API, which reaches the GC heap.
        let cell = params[0].unwrap_any_ref().unwrap().unwrap_struct(&caller)?;
        let value = cell.field(&mut caller, 0)?.unwrap_i32();
        caller.data().fetch_add(1, Ordering::SeqCst);
        results[0] = Val::I32(value);
        Ok(())
    });
    let instance = Instance::new(&mut store, &module, &[touch.into()]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    store.interrupt_on_memory(&memory).unwrap();
    Fixture { store, instance }
}

/// Requests an interrupt after `delay` and repeats it until told to stop.
fn requester(handle: InterruptHandle, delay: Duration) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let thread = std::thread::spawn({
        let stop = stop.clone();
        move || {
            std::thread::sleep(delay);
            while !stop.load(Ordering::SeqCst) {
                handle.interrupt();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    (stop, thread)
}

fn assert_interrupted(name: &str) -> Fixture {
    let mut fixture = fixture();
    let spin = fixture
        .instance
        .get_typed_func::<(), ()>(&mut fixture.store, name)
        .unwrap();
    let handle = fixture.store.interrupt_handle();
    let (stop, thread) = requester(handle.clone(), Duration::from_millis(30));
    let started = Instant::now();
    let error = spin.call(&mut fixture.store, ()).unwrap_err();
    stop.store(true, Ordering::SeqCst);
    thread.join().unwrap();
    handle.cancel();
    assert_eq!(error.downcast_ref::<Trap>(), Some(&Trap::Interrupt), "{name}: {error:?}");
    assert!(started.elapsed() >= Duration::from_millis(30), "{name} stopped early");
    assert!(started.elapsed() < Duration::from_secs(20), "{name} stopped late");
    fixture
}

fn sum(fixture: &mut Fixture, value: i32) -> Result<i32> {
    let sum = fixture
        .instance
        .get_typed_func::<i32, i32>(&mut fixture.store, "sum")
        .unwrap();
    sum.call(&mut fixture.store, value)
}

#[test]
fn compiled_code_stops_at_its_next_gc_heap_access() {
    let mut fixture = assert_interrupted("spin_heap");
    // The store stays usable once the request is withdrawn.
    assert_eq!(sum(&mut fixture, 3).unwrap(), 3);
}

#[test]
fn compiled_code_stops_at_a_watched_linear_memory_access() {
    let mut fixture = assert_interrupted("spin_memory");
    let before = sum(&mut fixture, 0).unwrap();
    assert!(before > 0, "the loop ran before it was stopped");
}

#[test]
fn host_code_keeps_running_and_the_caller_is_stopped() {
    let fixture = assert_interrupted("spin_host");
    assert!(fixture.store.data().load(Ordering::SeqCst) > 0);
}

#[test]
fn a_request_made_while_idle_is_delivered_at_entry_and_host_reads_survive() {
    let mut fixture = fixture();
    let new_cell = fixture
        .instance
        .get_typed_func::<(), Rooted<StructRef>>(&mut fixture.store, "new_cell")
        .unwrap();
    let cell = new_cell.call(&mut fixture.store, ()).unwrap();
    let handle = fixture.store.interrupt_handle();
    handle.interrupt();
    assert!(handle.is_pending());
    // Host code outside any activation reads the inaccessible heap.
    assert_eq!(cell.field(&mut fixture.store, 0).unwrap().unwrap_i32(), 7);
    let error = sum(&mut fixture, 1).unwrap_err();
    assert_eq!(error.downcast_ref::<Trap>(), Some(&Trap::Interrupt));
    assert!(!handle.is_pending());
    assert_eq!(sum(&mut fixture, 1).unwrap(), 1);
}

#[test]
fn a_cancelled_request_is_not_delivered() {
    let mut fixture = fixture();
    let handle = fixture.store.interrupt_handle();
    handle.interrupt();
    handle.cancel();
    assert!(!handle.is_pending());
    assert_eq!(sum(&mut fixture, 4).unwrap(), 4);
}

#[test]
fn a_dropped_store_ignores_its_handles() {
    let fixture = fixture();
    let handle = fixture.store.interrupt_handle();
    handle.interrupt();
    drop(fixture);
    handle.interrupt();
    handle.cancel();
    // A later store may reuse the same addresses and must be unaffected.
    let mut next = self::fixture();
    assert_eq!(sum(&mut next, 9).unwrap(), 9);
}

#[test]
fn many_stores_are_interrupted_independently() {
    let threads: Vec<_> = (0..8)
        .map(|index| {
            std::thread::spawn(move || {
                let name = ["spin_heap", "spin_memory", "spin_host"][index % 3];
                let mut fixture = assert_interrupted(name);
                assert_eq!(sum(&mut fixture, 2).unwrap() >= 2, true);
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

/// A deadline and a null check share the fault path but not their addresses:
/// a request never publishes the GC heap's null guard. Compiled code that
/// reaches a null access before its next watched access traps on the null
/// reference, exactly as an explicit check (which accesses no memory) did, and
/// the request is delivered at the next entry. Code that reaches watched memory
/// first stops for the deadline.
#[test]
fn a_null_access_and_a_request_are_told_apart_by_address() {
    let engine = engine();
    let module = Module::new(
        &engine,
        r#"
        (module
          (type $cell (struct (field (mut i32))))
          (import "" "request" (func $request))
          (global $cell (ref $cell) (struct.new $cell (i32.const 7)))
          (func (export "null_after_request") (result i32)
            (call $request)
            (struct.get $cell 0 (ref.null $cell)))
          (func (export "heap_then_null_after_request") (result i32)
            (call $request)
            (drop (struct.get $cell 0 (global.get $cell)))
            (struct.get $cell 0 (ref.null $cell)))
          (func (export "null") (result i32)
            (struct.get $cell 0 (ref.null $cell)))
          (func (export "read") (result i32)
            (struct.get $cell 0 (global.get $cell))))
        "#,
    )
    .unwrap();
    let mut store = Store::new(&engine, ());
    let handle = store.interrupt_handle();
    let request = Func::wrap(&mut store, {
        let handle = handle.clone();
        move || handle.interrupt()
    });
    let instance = Instance::new(&mut store, &module, &[request.into()]).unwrap();
    let call = |store: &mut Store<()>, name: &str| {
        instance
            .get_typed_func::<(), i32>(&mut *store, name)
            .unwrap()
            .call(&mut *store, ())
    };
    let trap = |result: Result<i32>| *result.unwrap_err().downcast_ref::<Trap>().unwrap();

    assert_eq!(trap(call(&mut store, "null_after_request")), Trap::NullReference);
    assert!(handle.is_pending());
    assert_eq!(trap(call(&mut store, "read")), Trap::Interrupt);

    assert_eq!(trap(call(&mut store, "heap_then_null_after_request")), Trap::Interrupt);
    assert!(!handle.is_pending());

    handle.interrupt();
    handle.cancel();
    assert_eq!(trap(call(&mut store, "null")), Trap::NullReference);
    assert_eq!(call(&mut store, "read").unwrap(), 7);
}
