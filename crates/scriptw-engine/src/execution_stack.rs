//! Synchronous caller-thread guarded execution stacks for the bounded Engine entry stack.
//!
//! Idle `GuardedStack`s are cached per OS thread up to a small process-wide cap
//! (`MAX_CACHED_EXECUTION_STACKS`) so repeated callers avoid OS thread spawns
//! without unbounded virtual stack retention across large caller thread pools.
//! Reentrant Engine entry retains the existing scoped 64 MiB thread path so
//! nested calls receive an independent guarded stack.

use super::{EngineWorkerStackGuard, ENGINE_WORKER_STACK_ACTIVE, ENGINE_WORKER_STACK_SIZE};
use std::{
    cell::RefCell,
    panic::{self, AssertUnwindSafe},
    sync::atomic::{AtomicUsize, Ordering},
};

const MAX_CACHED_EXECUTION_STACKS: usize = 2;

/// Reserve frame headroom for the stack-switch trampoline and entry wrapper so
/// the callee inside `run_on_execution_stack` always receives at least the full
/// `ENGINE_WORKER_STACK_SIZE` usable stack.
const FIBER_FRAME_HEADROOM: usize = 256 * 1024;

static CACHED_EXECUTION_STACK_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
mod native_stack {
    use rustix::mm::{mmap_anonymous, mprotect, munmap, MapFlags, MprotectFlags, ProtFlags};
    use std::ffi::c_void;
    use std::io;
    use std::ptr::{self, NonNull};

    pub(super) struct GuardedStack {
        base: NonNull<u8>,
        total_len: usize,
    }

    // SAFETY: A `GuardedStack` owns an exclusive anonymous `mmap` region not currently
    // executing on any thread when transferred or dropped.
    unsafe impl Send for GuardedStack {}

    impl GuardedStack {
        pub(super) fn new(usable_size: usize) -> io::Result<Self> {
            let page_size = rustix::param::page_size().max(4096);
            let guard_size = page_size;
            let usable_aligned = usable_size
                .checked_add(page_size - 1)
                .map(|v| v & !(page_size - 1))
                .ok_or_else(|| io::Error::other("execution stack size overflow"))?;
            let total_len = guard_size
                .checked_add(usable_aligned)
                .ok_or_else(|| io::Error::other("execution stack total size overflow"))?;

            // SAFETY: Anonymous private mapping with null address hint.
            let raw: *mut c_void = unsafe {
                mmap_anonymous(
                    ptr::null_mut(),
                    total_len,
                    ProtFlags::READ | ProtFlags::WRITE,
                    MapFlags::PRIVATE,
                )
            }?;
            let base = NonNull::new(raw.cast::<u8>())
                .ok_or_else(|| io::Error::other("mmap_anonymous returned null"))?;

            // Place a PROT_NONE guard page at the bottom of the downward-growing stack.
            // SAFETY: `[raw, raw + guard_size)` is the first page of our fresh mapping.
            if let Err(err) = unsafe { mprotect(raw, guard_size, MprotectFlags::empty()) } {
                // SAFETY: Unmap the entire mapping on failure.
                let _ = unsafe { munmap(raw, total_len) };
                return Err(err.into());
            }

            Ok(Self { base, total_len })
        }

        pub(super) fn top(&self) -> Option<*mut u8> {
            // SAFETY: `total_len` is the exact mapping length, so `base + total_len`
            // is the page-aligned (16-byte-aligned) one-past-the-end stack top.
            Some(unsafe { self.base.as_ptr().add(self.total_len) })
        }
    }

    impl Drop for GuardedStack {
        fn drop(&mut self) {
            // SAFETY: `self.base` and `self.total_len` came from `mmap_anonymous`.
            let _ = unsafe { munmap(self.base.as_ptr().cast::<c_void>(), self.total_len) };
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn switch_and_call(
        _stack_top: *mut u8,
        _entry: extern "C" fn(*mut u8),
        _arg: *mut u8,
    ) {
        core::arch::naked_asm!(
            "push rbp",
            "mov rbp, rsp",
            "and rdi, -16",
            "mov rsp, rdi",
            "mov rdi, rdx",
            "call rsi",
            "mov rsp, rbp",
            "pop rbp",
            "ret",
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[unsafe(naked)]
    pub(super) unsafe extern "C" fn switch_and_call(
        _stack_top: *mut u8,
        _entry: extern "C" fn(*mut u8),
        _arg: *mut u8,
    ) {
        core::arch::naked_asm!(
            "stp x29, x30, [sp, #-16]!",
            "mov x29, sp",
            "and x0, x0, #-16",
            "mov sp, x0",
            "mov x0, x2",
            "blr x1",
            "mov sp, x29",
            "ldp x29, x30, [sp], #16",
            "ret",
        );
    }
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
use native_stack::{switch_and_call, GuardedStack};

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
struct CachedExecutionStack(Option<GuardedStack>);

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
impl CachedExecutionStack {
    fn into_inner(mut self) -> GuardedStack {
        // The Drop impl decrements the process-wide cached count when this
        // wrapper leaves TLS, either when taken for execution or on thread exit.
        self.0.take().expect("cached GuardedStack present")
    }
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
impl Drop for CachedExecutionStack {
    fn drop(&mut self) {
        CACHED_EXECUTION_STACK_COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
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

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
fn take_execution_stack() -> GuardedStack {
    if let Some(cached) = EXECUTION_STACK.with(|slot| slot.borrow_mut().take()) {
        return cached.into_inner();
    }
    GuardedStack::new(ENGINE_WORKER_STACK_SIZE + FIBER_FRAME_HEADROOM)
        .unwrap_or_else(|error| panic!("scriptw execution stack allocation failed: {error}"))
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
fn try_reserve_cached_slot() -> bool {
    CACHED_EXECUTION_STACK_COUNT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_CACHED_EXECUTION_STACKS).then_some(count + 1)
        })
        .is_ok()
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
fn cache_execution_stack(stack: GuardedStack) {
    if !try_reserve_cached_slot() {
        drop(stack);
        return;
    }
    let cached = CachedExecutionStack(Some(stack));
    EXECUTION_STACK.with(|slot| {
        let previous = slot.replace(Some(cached));
        assert!(
            previous.is_none(),
            "execution stack cache occupied during call"
        );
    });
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
struct StackCallState<T, F> {
    func: Option<F>,
    result: Option<std::thread::Result<T>>,
    #[cfg(test)]
    stack_top: Option<usize>,
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
extern "C" fn execution_stack_trampoline<T, F>(raw_state: *mut u8)
where
    F: FnOnce() -> T,
{
    // SAFETY: `raw_state` points to the caller's live `StackCallState<T, F>`.
    // `catch_unwind` ensures no Rust panic ever unwinds across `switch_and_call`.
    let state = unsafe { &mut *(raw_state as *mut StackCallState<T, F>) };
    let func = state.func.take().expect("execution stack closure present");
    #[cfg(test)]
    let stack_top = state.stack_top;
    let result = panic::catch_unwind(AssertUnwindSafe(move || {
        #[cfg(test)]
        let _active_stack_top = ActiveStackTopGuard::enter(stack_top);
        let _stack = EngineWorkerStackGuard::enter();
        func()
    }));
    state.result = Some(result);
}

#[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
fn run_on_cached_stack<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    let stack = take_execution_stack();
    let stack_top_ptr = stack.top().expect("guarded stack top");
    let mut state = StackCallState {
        func: Some(f),
        result: None,
        #[cfg(test)]
        stack_top: Some(stack_top_ptr as usize),
    };
    // SAFETY: `stack_top_ptr` is the 16-byte-aligned top of our exclusively owned
    // guarded stack; `execution_stack_trampoline` catches all panics before returning.
    unsafe {
        switch_and_call(
            stack_top_ptr,
            execution_stack_trampoline::<T, F>,
            (&mut state as *mut StackCallState<T, F>).cast::<u8>(),
        );
    }
    cache_execution_stack(stack);
    match state
        .result
        .take()
        .expect("execution stack trampoline did not store a result")
    {
        Ok(value) => value,
        Err(payload) => panic::resume_unwind(payload),
    }
}

pub(super) fn run_on_execution_stack<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    // A running caller-thread stack cannot be reused recursively on the same OS thread.
    // Reentrant Store/host calls keep the existing scoped thread path, which
    // provides a fresh guarded stack and preserves borrowed closure lifetimes.
    if ENGINE_WORKER_STACK_ACTIVE.get() {
        return super::run_on_sized_stack(f);
    }
    #[cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        run_on_cached_stack(f)
    }
    #[cfg(not(all(unix, any(target_arch = "x86_64", target_arch = "aarch64"))))]
    {
        super::run_on_sized_stack(f)
    }
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
                "wave retained {tls_cached} TLS execution stacks (expected 1..={MAX_CACHED_EXECUTION_STACKS})"
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
