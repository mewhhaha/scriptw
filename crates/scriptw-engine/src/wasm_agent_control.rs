//! One parent execution budget for native agent waits and worker deadlines.

use super::{EngineError, EngineExecutionFailure};
use std::sync::{mpsc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

enum ExecutionDeadline {
    Unbounded,
    Bounded { started: Instant, budget: Duration },
}

pub(super) enum AgentReceiveError {
    Disconnected,
    Execution(EngineError),
}

pub(super) struct WasmAgentExecutionControl {
    deadline: OnceLock<ExecutionDeadline>,
    finished: Mutex<bool>,
    changed: Condvar,
}

impl WasmAgentExecutionControl {
    pub(super) fn new() -> Self {
        Self {
            deadline: OnceLock::new(),
            finished: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    pub(super) fn arm(&self, started: Instant, timeout_ms: Option<u64>) {
        let deadline = match timeout_ms {
            Some(milliseconds) => ExecutionDeadline::Bounded {
                started,
                budget: Duration::from_millis(milliseconds),
            },
            None => ExecutionDeadline::Unbounded,
        };
        assert!(
            self.deadline.set(deadline).is_ok(),
            "parent budget is armed once"
        );
    }

    fn timeout() -> EngineError {
        EngineError::from_execution_failure(
            EngineExecutionFailure::Timeout,
            "timeout exceeded while waiting for a Test262 agent",
        )
    }

    pub(super) fn remaining(&self) -> Result<Option<Duration>, EngineError> {
        match self.deadline.get() {
            Some(ExecutionDeadline::Unbounded) => Ok(None),
            Some(ExecutionDeadline::Bounded { started, budget }) => budget
                .checked_sub(started.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .map(Some)
                .ok_or_else(Self::timeout),
            None => Err(EngineError::from_execution_failure(
                EngineExecutionFailure::Trap,
                "Test262 agent used before the parent execution budget was armed",
            )),
        }
    }

    pub(super) fn receive<T>(&self, receiver: &mpsc::Receiver<T>) -> Result<T, AgentReceiveError> {
        match self.remaining().map_err(AgentReceiveError::Execution)? {
            None => receiver.recv().map_err(|_| AgentReceiveError::Disconnected),
            Some(remaining) => receiver
                .recv_timeout(remaining)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Disconnected => AgentReceiveError::Disconnected,
                    mpsc::RecvTimeoutError::Timeout => {
                        AgentReceiveError::Execution(Self::timeout())
                    }
                }),
        }
    }

    pub(super) fn sleep(&self, milliseconds: f64) -> Result<(), EngineError> {
        if !milliseconds.is_finite() || milliseconds <= 0.0 {
            return Ok(());
        }
        // Very large finite delays are still interruptible by the parent.
        let delay = Duration::try_from_secs_f64(milliseconds / 1_000.0).unwrap_or(Duration::MAX);
        let started = Instant::now();
        let mut finished = self
            .finished
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if *finished {
                return Ok(());
            }
            let remaining_delay = delay.saturating_sub(started.elapsed());
            if remaining_delay.is_zero() {
                return Ok(());
            }
            let wait = self
                .remaining()?
                .map_or(remaining_delay, |remaining| remaining.min(remaining_delay));
            let (next, _) = self
                .changed
                .wait_timeout(finished, wait.min(Duration::from_secs(86_400)))
                .unwrap_or_else(|error| error.into_inner());
            finished = next;
        }
    }

    pub(super) fn finish(&self) {
        *self
            .finished
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WasmExecutionFailureKind;
    use std::sync::Arc;

    #[test]
    fn expired_parent_budget_never_becomes_a_fresh_receive_budget() {
        let control = WasmAgentExecutionControl::new();
        control.arm(Instant::now() - Duration::from_secs(1), Some(100));
        let (_sender, receiver) = mpsc::channel::<()>();
        for _ in 0..2 {
            match control.receive(&receiver) {
                Err(AgentReceiveError::Execution(error)) => assert_eq!(
                    error.wasm_execution_failure_kind(),
                    Some(WasmExecutionFailureKind::Timeout)
                ),
                Ok(()) | Err(AgentReceiveError::Disconnected) => {
                    panic!("the same parent deadline remains expired")
                }
            }
        }
    }

    #[test]
    fn a_connected_recipient_that_does_not_retrieve_times_out() {
        let control = WasmAgentExecutionControl::new();
        control.arm(Instant::now(), Some(30));
        let (_sender, receiver) = mpsc::channel::<()>();
        match control.receive(&receiver) {
            Err(AgentReceiveError::Execution(error)) => assert_eq!(
                error.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::Timeout)
            ),
            Ok(()) | Err(AgentReceiveError::Disconnected) => {
                panic!("a live pending recipient must time out")
            }
        }
    }

    #[test]
    fn finish_wakes_a_sleeping_worker_without_waiting_for_its_delay() {
        let control = Arc::new(WasmAgentExecutionControl::new());
        control.arm(Instant::now(), None);
        let sleeping = Arc::clone(&control);
        let (returned, observed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            returned
                .send(sleeping.sleep(60_000.0))
                .expect("sleep owner observes completion");
        });
        control.finish();
        observed
            .recv_timeout(Duration::from_secs(5))
            .expect("finish wakes native sleep")
            .unwrap();
        worker.join().expect("sleeping worker joins");
    }

    #[test]
    fn a_long_native_sleep_uses_the_parent_budget() {
        let control = WasmAgentExecutionControl::new();
        control.arm(Instant::now(), Some(30));
        assert_eq!(
            control
                .sleep(60_000.0)
                .expect_err("sleep cannot extend the parent budget")
                .wasm_execution_failure_kind(),
            Some(WasmExecutionFailureKind::Timeout)
        );
    }
}
