//! Reuse execution stacks without retaining deep compiler/emit stack pages.
//!
//! Each calling host thread lazily owns one pool with one 64MiB worker. Its TLS
//! drop requests worker termination; touched execution-stack pages can remain
//! resident for the pool's lifetime. The pool retains no Store or Realm.
//! `install` joins borrowed work before return. Reentrant entry uses a fresh
//! scoped stack: a different Store can reset its guest stack limit, so the
//! outer worker cannot promise enough remaining host stack. An external Rayon
//! caller yielding into another job also uses a scoped worker while its pool
//! is busy, instead of queueing behind the first invocation.
use super::{EngineWorkerStackGuard, ENGINE_WORKER_STACK_ACTIVE, ENGINE_WORKER_STACK_SIZE};

thread_local! {
    static EXECUTION_POOL: std::cell::OnceCell<rayon::ThreadPool> = const { std::cell::OnceCell::new() };
    static EXECUTION_POOL_IN_FLIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct CallerInstallGuard {
    thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl CallerInstallGuard {
    fn try_enter() -> Option<Self> {
        if EXECUTION_POOL_IN_FLIGHT.replace(true) {
            None
        } else {
            Some(Self {
                thread: std::marker::PhantomData,
            })
        }
    }
}

impl Drop for CallerInstallGuard {
    fn drop(&mut self) {
        EXECUTION_POOL_IN_FLIGHT.set(false);
    }
}

pub(super) fn run_on_execution_stack<T, F>(f: F) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    if ENGINE_WORKER_STACK_ACTIVE.get() {
        return super::run_on_sized_stack(f);
    }
    let Some(_caller) = CallerInstallGuard::try_enter() else {
        return super::run_on_sized_stack(f);
    };
    EXECUTION_POOL.with(|pool| {
        pool.get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .stack_size(ENGINE_WORKER_STACK_SIZE)
                .thread_name(|_| "scriptw-execution".to_owned())
                .build()
                .expect("scriptw-engine execution worker should spawn")
        })
        .install(|| {
            let _stack = EngineWorkerStackGuard::enter();
            f()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn execution_worker_reuses_top_level_stack_and_isolates_borrowed_reentry() {
        let caller = thread::current().id();
        let mut values = [0; 2];
        let worker = run_on_execution_stack(|| {
            assert!(ENGINE_WORKER_STACK_ACTIVE.get());
            let worker = thread::current().id();
            let [first, second] = &mut values;
            rayon::scope(|scope| {
                scope.spawn(|_| *first = 17);
                scope.spawn(|_| *second = 29);
            });
            run_on_execution_stack(|| {
                assert_ne!(thread::current().id(), worker);
                assert!(ENGINE_WORKER_STACK_ACTIVE.get());
                values[0] += 1;
                values[1] += 1;
            });
            assert!(ENGINE_WORKER_STACK_ACTIVE.get());
            worker
        });
        assert_ne!(worker, caller);
        assert_eq!(values, [18, 30]);
        assert_eq!(run_on_execution_stack(|| thread::current().id()), worker);
        assert!(!ENGINE_WORKER_STACK_ACTIVE.get());
    }

    #[test]
    fn execution_worker_restores_admission_after_panic_and_remains_usable() {
        let worker = run_on_execution_stack(|| thread::current().id());
        let panic = std::panic::catch_unwind(|| {
            run_on_execution_stack(|| panic!("exercise execution worker unwinding"));
        });
        assert!(panic.is_err());
        assert!(!ENGINE_WORKER_STACK_ACTIVE.get());
        assert!(!EXECUTION_POOL_IN_FLIGHT.get());
        EXECUTION_POOL.with(|pool| {
            let active = pool
                .get()
                .unwrap()
                .install(|| ENGINE_WORKER_STACK_ACTIVE.get());
            assert!(!active);
        });
        assert_eq!(run_on_execution_stack(|| thread::current().id()), worker);
    }

    #[test]
    fn separate_callers_have_concurrent_execution_workers() {
        use std::sync::mpsc;
        use std::time::Duration;

        let timeout = Duration::from_secs(10);
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut releases = Vec::new();
        let mut callers = Vec::new();
        for _ in 0..2 {
            let ready_tx = ready_tx.clone();
            let (release_tx, release_rx) = mpsc::channel();
            releases.push(release_tx);
            callers.push(thread::spawn(move || {
                let caller = thread::current().id();
                run_on_execution_stack(move || {
                    let worker = thread::current().id();
                    ready_tx.send((caller, worker)).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(30)).unwrap();
                    worker
                })
            }));
        }
        drop(ready_tx);
        let first = ready_rx.recv_timeout(timeout);
        let second = ready_rx.recv_timeout(timeout);
        // Always release both workers before assertions, even if an incorrect
        // shared pool prevented the second caller from entering concurrently.
        for release in releases {
            let _ = release.send(());
        }
        let workers: Vec<_> = callers.into_iter().map(|caller| caller.join()).collect();
        let first = first.expect("first caller should enter its worker");
        let second = second.expect("second caller must not wait for the first");
        assert_ne!(first.0, first.1);
        assert_ne!(second.0, second.1);
        assert_ne!(first.1, second.1);
        assert!(workers.into_iter().all(|worker| worker.is_ok()));
    }

    #[test]
    fn external_rayon_reentry_does_not_queue_behind_its_blocked_worker() {
        use std::sync::mpsc;
        use std::time::Duration;

        let timeout = Duration::from_secs(10);
        let external = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (first, second, in_flight) = external.install(|| {
            let mut second = None;
            let first = rayon::scope(|scope| {
                let second = &mut second;
                scope.spawn(move |_| {
                    let caller = thread::current().id();
                    let started = started_rx.recv_timeout(timeout).is_ok();
                    let in_flight = EXECUTION_POOL_IN_FLIGHT.get();
                    let (worker, released) = run_on_execution_stack(move || {
                        (thread::current().id(), release_tx.send(()).is_ok())
                    });
                    *second = Some((
                        caller,
                        worker,
                        started,
                        in_flight,
                        released,
                        EXECUTION_POOL_IN_FLIGHT.get(),
                    ));
                });
                let caller = thread::current().id();
                let (worker, started, released) = run_on_execution_stack(move || {
                    let started = started_tx.send(()).is_ok();
                    let released = release_rx.recv_timeout(timeout).is_ok();
                    (thread::current().id(), started, released)
                });
                (caller, worker, started, released)
            });
            (first, second.unwrap(), EXECUTION_POOL_IN_FLIGHT.get())
        });
        // Both release paths and the borrowed scope have completed before any
        // assertions, so a regression fails without leaving blocked workers.
        assert_eq!(first.0, second.0);
        assert_ne!(first.1, second.1);
        assert_ne!(first.0, first.1);
        assert_ne!(second.0, second.1);
        assert!(first.2 && first.3 && second.2 && second.3 && second.4 && second.5);
        assert!(!in_flight);
    }

    #[test]
    fn caller_exit_terminates_its_execution_worker() {
        use std::sync::mpsc;
        use std::time::Duration;

        struct ExitNotice(mpsc::Sender<()>);
        impl Drop for ExitNotice {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        thread_local! {
            static EXIT_NOTICE: std::cell::RefCell<Option<ExitNotice>> = const {
                std::cell::RefCell::new(None)
            };
        }

        let (exit_tx, exit_rx) = mpsc::channel();
        thread::spawn(move || {
            run_on_execution_stack(move || {
                EXIT_NOTICE.with(|notice| *notice.borrow_mut() = Some(ExitNotice(exit_tx)));
            });
        })
        .join()
        .unwrap();
        exit_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the execution worker must exit after its owning caller");
    }

    #[test]
    fn compiler_work_keeps_its_scoped_stack_inside_execution() {
        let worker = run_on_execution_stack(|| {
            let worker = thread::current().id();
            let compiler = crate::run_on_sized_stack(|| {
                assert!(ENGINE_WORKER_STACK_ACTIVE.get());
                thread::current().id()
            });
            assert_ne!(worker, compiler);
            assert!(ENGINE_WORKER_STACK_ACTIVE.get());
            worker
        });
        assert_eq!(run_on_execution_stack(|| thread::current().id()), worker);
    }
}
