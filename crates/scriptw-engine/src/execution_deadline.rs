//! Execution deadlines enforced by interrupting the Store.
//!
//! Compiled code carries no deadline checks. One process-wide thread asks
//! Wasmtime to interrupt each Store whose deadline has passed: Wasmtime removes
//! access to the Store's GC heap and private linear memory, and compiled code
//! stops with `Trap::Interrupt` at its next access to either. Host code that
//! reaches that memory first gets access back and continues, so the request is
//! repeated until the guard for that execution is dropped.

use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use wasmtime::InterruptHandle;

/// Period of repeated requests once a deadline has passed.
const REPEAT: Duration = Duration::from_millis(1);

struct Watch {
    id: u64,
    deadline: Instant,
    handle: InterruptHandle,
}

#[derive(Default)]
struct Watches {
    next_id: u64,
    armed: Vec<Watch>,
}

struct Shared {
    watches: Mutex<Watches>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Watches> {
        self.watches
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<&'static Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let shared: &'static Shared = Box::leak(Box::new(Shared {
            watches: Mutex::new(Watches::default()),
            changed: Condvar::new(),
        }));
        std::thread::Builder::new()
            .name("scriptw-execution-deadline".to_string())
            .spawn(move || watch(shared))
            .expect("scriptw execution deadline thread should spawn");
        shared
    })
}

fn watch(shared: &'static Shared) -> ! {
    let mut watches = shared.lock();
    loop {
        let now = Instant::now();
        let mut next = None;
        for watch in &watches.armed {
            // Requests are made under the lock, so none follows the removal
            // of a watch by its guard.
            let wake = if watch.deadline <= now {
                watch.handle.interrupt();
                now + REPEAT
            } else {
                watch.deadline
            };
            next = Some(next.map_or(wake, |next: Instant| next.min(wake)));
        }
        watches = match next {
            Some(wake) => {
                let wait = wake.saturating_duration_since(Instant::now());
                shared
                    .changed
                    .wait_timeout(watches, wait)
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0
            }
            None => shared
                .changed
                .wait(watches)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

/// Interrupts one Store's compiled code from `deadline` until dropped.
///
/// Declare it after the Store it guards, so that it is dropped first.
pub(crate) struct ExecutionDeadline {
    id: u64,
    handle: InterruptHandle,
}

impl ExecutionDeadline {
    pub(crate) fn arm(handle: InterruptHandle, deadline: Instant) -> Self {
        let shared = shared();
        let mut watches = shared.lock();
        let id = watches.next_id;
        watches.next_id += 1;
        watches.armed.push(Watch {
            id,
            deadline,
            handle: handle.clone(),
        });
        drop(watches);
        shared.changed.notify_one();
        Self { id, handle }
    }
}

impl Drop for ExecutionDeadline {
    fn drop(&mut self) {
        shared().lock().armed.retain(|watch| watch.id != self.id);
        // No request follows the removal, so this withdraws the last one.
        self.handle.cancel();
    }
}
