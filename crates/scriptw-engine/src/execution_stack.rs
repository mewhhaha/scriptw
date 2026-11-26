//! Synchronous caller-thread Wasmtime fibers for the bounded Engine entry stack.
//!
//! Idle `FiberStack`s are cached per OS thread up to a small process-wide cap
//! (`MAX_CACHED_EXECUTION_STACKS`) so repeated callers avoid OS thread spawns
//! without unbounded virtual stack retention across large caller thread pools.
//! A nested public Engine entry cannot resume another Wasmtime Fiber on the
//! same thread, so reentry retains the existing scoped 64 MiB thread path and
//! receives an independent guarded stack.
use super::{EngineWorkerStackGuard, ENGINE_WORKER_STACK_ACTIVE, ENGINE_WORKER_STACK_SIZE};
use std::{
    cell::RefCell,
    panic::{self, AssertUnwindSafe},
    sync::atomic::{AtomicUsize, Ordering},
};
use wasmtime_fiber::{Fiber, FiberStack};

const MAX_CACHED_EXECUTION_STACKS: usize = 2;

/// Reserve frame headroom for the Wasmtime fiber trampoline and entry wrapper so
/// the callee inside `run_on_execution_stack` always receives at least the full
/// `ENGINE_WORKER_STACK_SIZE` usable stack.
const FIBER_FRAME_HEADROOM: usize = 256 * 1024;

static CACHED_EXECUTION_STACK_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CachedExecutionStack(Option<FiberStack>);

impl CachedExecutionStack {
    fn into_inner(mut self) -> FiberStack {
        // The Drop impl decrements the process-wide cached count when this
        // wrapper leaves TLS, either when taken for execution or on thread exit.
        self.0.take().expect("cached FiberStack present")
    }
}

impl Drop for CachedExecutionStack {
    fn drop(&mut self) {
        CACHED_EXECUTION_STACK_COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}

thread_local! {
    static EXECUTION_STACK: RefCell<Option<CachedExecutionStack>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static ACTIVE_EXECUTION_STACK_TOP: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
struct ActiveStackTopGuard(Option<usize>);

#[cfg(test)]
impl ActiveStackTopGuard {
    fn enter(top: Option<usize>) -> Self {
        let previous = ACTIVE_EXECUTION_STACK_TOP.with(|slot| slot.replace(top));
        Self(previous)
    }
}

#[cfg(test)]
impl Drop for ActiveStackTopGuard {
    fn drop(&mut self) {
        ACTIVE_EXECUTION_STACK_TOP.with(|slot| slot.set(self.0));
    }
}

fn take_execution_stack() -> FiberStack {
    if let Some(cached) = EXECUTION_STACK.with(|slot| slot.borrow_mut().take()) {
        return cached.into_inner();
    }
    FiberStack::new(ENGINE_WORKER_STACK_SIZE + FIBER_FRAME_HEADROOM, false)
        .unwrap_or_else(|error| panic!("scriptw execution FiberStack allocation failed: {error}"))
}

fn try_reserve_cached_slot() -> bool {
    CACHED_EXECUTION_STACK_COUNT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_CACHED_EXECUTION_STACKS).then_some(count + 1)
        })
        .is_ok()
}

fn cache_execution_stack(stack: FiberStack) {
    if !try_reserve_cached_slot() {
        drop(stack);
        return;
    }
    let cached = CachedExecutionStack(Some(stack));
    EXECUTION_STACK.with(|slot| {
        let previous = slot.replace(Some(cached));
        assert!(
            previous.is_none(),
            "execution FiberStack cache occupied during call"
        );
    });
}

fn run_on_cached_fiber<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    let stack = take_execution_stack();
    #[cfg(test)]
    let stack_top = stack.top().map(|top| top as usize);
    let fiber = match Fiber::new(stack, move |(), _suspend| {
        #[cfg(test)]
        let _active_stack_top = ActiveStackTopGuard::enter(stack_top);
        let _stack = EngineWorkerStackGuard::enter();
        f()
    }) {
        Ok(fiber) => fiber,
        Err((error, stack)) => {
            cache_execution_stack(stack);
            panic!("scriptw execution Fiber creation failed: {error}");
        }
    };

    // The Wasmtime fiber catches panics on its stack and propagates them from
    // resume. Catch only to extract and return the completed guarded stack to
    // this thread's cache, then resume the original panic unchanged.
    let result = panic::catch_unwind(AssertUnwindSafe(|| fiber.resume(())));
    let stack = fiber.into_stack();
    cache_execution_stack(stack);
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(())) => unreachable!("execution fiber has no yield path"),
        Err(payload) => panic::resume_unwind(payload),
    }
}

pub(super) fn run_on_execution_stack<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    // A running Fiber cannot be resumed recursively on the same OS thread.
    // Reentrant Store/host calls keep the existing scoped thread path, which
    // provides a fresh guarded stack and preserves borrowed closure lifetimes.
    if ENGINE_WORKER_STACK_ACTIVE.get() {
        return super::run_on_sized_stack(f);
    }
    run_on_cached_fiber(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, thread};

    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn cached_fiber_uses_caller_thread_and_reuses_guarded_stack() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(ENGINE_WORKER_STACK_SIZE, 64 * 1024 * 1024);
        assert_eq!(crate::WASM_MAX_STACK_SIZE, 32 * 1024 * 1024);
        let caller = thread::current().id();
        let first = run_on_execution_stack(|| {
            assert!(ENGINE_WORKER_STACK_ACTIVE.get());
            thread::current().id()
        });
        let first_stack = EXECUTION_STACK.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|cached| cached.0.as_ref())
                .unwrap()
                .top()
        });
        let second = run_on_execution_stack(|| thread::current().id());
        let second_stack = EXECUTION_STACK.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|cached| cached.0.as_ref())
                .unwrap()
                .top()
        });
        assert_eq!(first, caller);
        assert_eq!(second, caller);
        assert_eq!(first_stack, second_stack);
        assert!(!ENGINE_WORKER_STACK_ACTIVE.get());
        EXECUTION_STACK.with(|slot| slot.borrow_mut().take());
    }

    #[test]
    fn concurrent_callers_keep_distinct_thread_local_stacks() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut callers = Vec::new();
        for _ in 0..2 {
            let barrier = barrier.clone();
            callers.push(thread::spawn(move || {
                let caller = thread::current().id();
                run_on_execution_stack(|| {
                    let executor = thread::current().id();
                    let top = ACTIVE_EXECUTION_STACK_TOP.with(|slot| slot.get());
                    barrier.wait();
                    (caller, executor, top)
                })
            }));
        }
        let first = callers.remove(0).join().unwrap();
        let second = callers.remove(0).join().unwrap();
        assert_eq!(first.0, first.1);
        assert_eq!(second.0, second.1);
        assert_ne!(first.1, second.1);
        assert!(first.2.is_some());
        assert!(second.2.is_some());
        assert_ne!(first.2, second.2);
    }

    #[test]
    fn reentry_uses_a_fresh_thread_stack_and_restores_outer_fiber() {
        let caller = thread::current().id();
        let outer = run_on_execution_stack(|| {
            let outer = thread::current().id();
            let nested = run_on_execution_stack(|| {
                assert!(ENGINE_WORKER_STACK_ACTIVE.get());
                thread::current().id()
            });
            assert_ne!(nested, outer);
            assert_eq!(thread::current().id(), outer);
            outer
        });
        assert_eq!(outer, caller);
        assert!(!ENGINE_WORKER_STACK_ACTIVE.get());
    }

    #[test]
    fn panic_returns_fiber_stack_to_cache_and_preserves_worker_admission() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let panic = panic::catch_unwind(|| {
            run_on_execution_stack(|| panic!("fiber unwind control"));
        });
        assert!(panic.is_err());
        assert!(!ENGINE_WORKER_STACK_ACTIVE.get());
        assert!(EXECUTION_STACK.with(|slot| slot.borrow().is_some()));
        assert_eq!(run_on_execution_stack(|| 29), 29);
        EXECUTION_STACK.with(|slot| slot.borrow_mut().take());
    }

    #[test]
    fn wasmtime_stack_overflow_traps_and_same_store_call_recovers() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let engine = crate::shared_wasm_engine().unwrap();
        let module = wasmtime::Module::new(
            &engine,
            // Three i32-result functions: recursive non-tail call + one,
            // exported overflow wrapper, and an exported constant control.
            // Binary input keeps the test independent of Wasmtime's WAT feature.
            &[
                0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 4, 3, 0, 0, 0, 7, 17, 2, 8,
                111, 118, 101, 114, 102, 108, 111, 119, 0, 1, 2, 111, 107, 0, 2, 10, 19, 3, 7, 0,
                16, 0, 65, 1, 106, 11, 4, 0, 16, 0, 11, 4, 0, 65, 29, 11,
            ],
        )
        .unwrap();
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let overflow = instance
            .get_typed_func::<(), i32>(&mut store, "overflow")
            .unwrap();
        let safe = instance
            .get_typed_func::<(), i32>(&mut store, "ok")
            .unwrap();

        let trap = run_on_execution_stack(|| overflow.call(&mut store, ())).unwrap_err();
        assert_eq!(
            trap.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::StackOverflow)
        );
        assert_eq!(
            run_on_execution_stack(|| safe.call(&mut store, ())).unwrap(),
            29
        );
        assert!(EXECUTION_STACK.with(|slot| slot.borrow().is_some()));
        EXECUTION_STACK.with(|slot| slot.borrow_mut().take());
    }

    #[test]
    fn external_rayon_reentry_does_not_block_the_fiber_caller() {
        let external = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let caller = thread::spawn(move || {
            let result = external.install(|| {
                run_on_execution_stack(|| {
                    let worker = thread::current().id();
                    let nested = run_on_execution_stack(|| thread::current().id());
                    (worker, nested)
                })
            });
            done_tx.send(result).unwrap();
        });
        let (outer, nested) = done_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("Rayon reentry must complete without queueing behind itself");
        caller.join().unwrap();
        assert_ne!(outer, nested);
    }

    #[test]
    fn idle_fiber_stack_cache_is_bounded_across_caller_threads() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXECUTION_STACK.with(|slot| slot.borrow_mut().take());
        let workers = 6;
        for _wave in 0..2 {
            let done = std::sync::Arc::new(std::sync::Barrier::new(workers + 1));
            let release = std::sync::Arc::new(std::sync::Barrier::new(workers + 1));
            let occupied = std::sync::Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                let done = done.clone();
                let release = release.clone();
                let occupied = occupied.clone();
                handles.push(thread::spawn(move || {
                    let value = run_on_execution_stack(|| 7);
                    assert_eq!(value, 7);
                    if EXECUTION_STACK.with(|slot| slot.borrow().is_some()) {
                        occupied.fetch_add(1, Ordering::AcqRel);
                    }
                    done.wait();
                    release.wait();
                }));
            }
            done.wait();
            let tls_cached = occupied.load(Ordering::Acquire);
            assert!(
                tls_cached >= 1 && tls_cached <= MAX_CACHED_EXECUTION_STACKS,
                "wave retained {tls_cached} TLS FiberStacks (expected 1..={MAX_CACHED_EXECUTION_STACKS})"
            );
            assert!(
                CACHED_EXECUTION_STACK_COUNT.load(Ordering::Acquire) <= MAX_CACHED_EXECUTION_STACKS
            );
            release.wait();
            for handle in handles {
                handle.join().unwrap();
            }
        }
    }
}
