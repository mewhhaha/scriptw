//! Worker completions carry owned data only. The owning Store retains every
//! resolver root and is the only place where Promise settlement can execute.

use super::{EngineError, ForeignValue};
use scriptw_ir::foreign::ForeignResultType;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use wasmtime::{EqRef, OwnedRooted};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ForeignRequestId(u64);

pub(super) enum ForeignAsyncSettlement {
    Fulfill(ForeignValue),
    Reject(ForeignValue),
    Cancelled,
}

struct WorkerSettlement {
    request: ForeignRequestId,
    settlement: ForeignAsyncSettlement,
}

struct PendingResolvers {
    resolve: OwnedRooted<EqRef>,
    reject: OwnedRooted<EqRef>,
}

pub(super) struct ReadyForeignSettlement {
    pub(super) resolve: OwnedRooted<EqRef>,
    pub(super) reject: OwnedRooted<EqRef>,
    pub(super) settlement: ForeignAsyncSettlement,
}

/// A single host operation's completion authority. Move it into an async task
/// or worker, then resolve or reject it with owned data. JavaScript runs only
/// when the embedder pumps the owning Store. Dropping this value cancels the
/// operation and schedules rejection at that same checkpoint.
///
/// This handle contains no Wasmtime references, Store borrows, or JS closures.
#[must_use = "dropping an unsettled host completion schedules cancellation"]
pub struct ForeignCompleter {
    request: ForeignRequestId,
    sender: Option<Sender<WorkerSettlement>>,
    expected: ForeignResultType,
}

impl std::fmt::Debug for ForeignCompleter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeignCompleter")
            .field("request", &self.request)
            .field("expected", &self.expected)
            .finish_non_exhaustive()
    }
}

impl ForeignCompleter {
    /// Fulfill with the declared result type. A type mismatch cancels this
    /// request and returns an error before any JavaScript is entered.
    pub fn resolve(self, value: ForeignValue) -> Result<(), EngineError> {
        let matches = match self.expected {
            ForeignResultType::Number => matches!(value, ForeignValue::Number(_)),
            ForeignResultType::StringUtf16 => matches!(value, ForeignValue::StringUtf16(_)),
            ForeignResultType::Bytes => matches!(value, ForeignValue::Bytes(_)),
            ForeignResultType::Void => matches!(value, ForeignValue::Void),
        };
        if !matches {
            return Err(EngineError::new(
                "asynchronous foreign fulfillment differs from the declared result type",
            ));
        }
        self.send(ForeignAsyncSettlement::Fulfill(value))
    }

    /// Reject with a JavaScript value. Rejection is independent of the declared
    /// fulfillment type and follows ordinary Promise reaction ordering.
    pub fn reject(self, value: ForeignValue) -> Result<(), EngineError> {
        self.send(ForeignAsyncSettlement::Reject(value))
    }

    /// Schedule cancellation explicitly. Dropping an unsettled handle has the
    /// same effect. The Store pump creates the catchable JavaScript error.
    pub fn cancel(self) -> Result<(), EngineError> {
        self.send(ForeignAsyncSettlement::Cancelled)
    }

    fn send(mut self, settlement: ForeignAsyncSettlement) -> Result<(), EngineError> {
        let sender = self
            .sender
            .take()
            .expect("a completion is consumed by its only settlement");
        sender
            .send(WorkerSettlement {
                request: self.request,
                settlement,
            })
            .map_err(|_| EngineError::new("asynchronous foreign operation's Store was dropped"))
    }
}

impl Drop for ForeignCompleter {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            // A closed receiver means its Store and every retained root have
            // already been released; cancellation then needs no further work.
            let _ = sender.send(WorkerSettlement {
                request: self.request,
                settlement: ForeignAsyncSettlement::Cancelled,
            });
        }
    }
}

pub(super) struct ForeignAsyncState {
    sender: Sender<WorkerSettlement>,
    receiver: Receiver<WorkerSettlement>,
    next_request: Option<ForeignRequestId>,
    pending: BTreeMap<ForeignRequestId, PendingResolvers>,
}

impl ForeignAsyncState {
    pub(super) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            next_request: Some(ForeignRequestId(1)),
            pending: BTreeMap::new(),
        }
    }

    /// Callers validate and root both resolver references in this Store before
    /// borrowing the state. Each nonclone completer can enqueue at most once,
    /// so queue storage is bounded by the outstanding request inventory.
    pub(super) fn register(
        &mut self,
        resolve: OwnedRooted<EqRef>,
        reject: OwnedRooted<EqRef>,
        expected: ForeignResultType,
    ) -> Result<ForeignCompleter, EngineError> {
        let request = self
            .next_request
            .ok_or_else(|| EngineError::new("asynchronous foreign request identity exhausted"))?;
        self.next_request = request.0.checked_add(1).map(ForeignRequestId);
        assert!(self
            .pending
            .insert(request, PendingResolvers { resolve, reject })
            .is_none());
        Ok(ForeignCompleter {
            request,
            sender: Some(self.sender.clone()),
            expected,
        })
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Detach a finite nonblocking FIFO batch. Worker threads never contend on
    /// Store locks, and jobs started by settlement belong to a later batch.
    pub(super) fn take_ready(&mut self) -> Vec<ReadyForeignSettlement> {
        let mut ready = Vec::new();
        for _ in 0..self.pending.len() {
            let Ok(message) = self.receiver.try_recv() else {
                break;
            };
            let pending = self
                .pending
                .remove(&message.request)
                .expect("each owned completer settles its registered request exactly once");
            ready.push(ReadyForeignSettlement {
                resolve: pending.resolve,
                reject: pending.reject,
                settlement: message.settlement,
            });
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completer(expected: ForeignResultType) -> (ForeignCompleter, Receiver<WorkerSettlement>) {
        let (sender, receiver) = mpsc::channel();
        (
            ForeignCompleter {
                request: ForeignRequestId(1),
                sender: Some(sender),
                expected,
            },
            receiver,
        )
    }

    #[test]
    fn worker_settlement_is_send_and_enqueues_only_owned_lossless_data_once() {
        let (completion, receiver) = completer(ForeignResultType::StringUtf16);
        std::thread::spawn(move || {
            completion.resolve(ForeignValue::StringUtf16(vec![0, 0xd800, 0xffff]))
        })
        .join()
        .unwrap()
        .unwrap();
        let settled = receiver.try_recv().unwrap();
        assert_eq!(settled.request, ForeignRequestId(1));
        let ForeignAsyncSettlement::Fulfill(ForeignValue::StringUtf16(units)) = settled.settlement
        else {
            panic!("worker must retain exact UTF-16 fulfillment");
        };
        assert_eq!(units, [0, 0xd800, 0xffff]);
        assert!(
            receiver.try_recv().is_err(),
            "settled Drop must not also cancel"
        );
    }

    #[test]
    fn abandoned_and_invalid_fulfillments_cancel_but_rejections_accept_any_value_type() {
        let (completion, receiver) = completer(ForeignResultType::Number);
        drop(completion);
        assert!(matches!(
            receiver.try_recv().unwrap().settlement,
            ForeignAsyncSettlement::Cancelled
        ));
        assert!(receiver.try_recv().is_err());

        let (completion, receiver) = completer(ForeignResultType::Number);
        assert!(completion.resolve(ForeignValue::Void).is_err());
        assert!(matches!(
            receiver.try_recv().unwrap().settlement,
            ForeignAsyncSettlement::Cancelled
        ));
        assert!(receiver.try_recv().is_err());

        let (completion, receiver) = completer(ForeignResultType::Number);
        completion
            .reject(ForeignValue::Bytes(vec![0, 255]))
            .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap().settlement,
            ForeignAsyncSettlement::Reject(ForeignValue::Bytes(_))
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn settlement_after_store_retirement_reports_closed_authority() {
        let (completion, receiver) = completer(ForeignResultType::Void);
        drop(receiver);
        assert!(completion
            .resolve(ForeignValue::Void)
            .unwrap_err()
            .to_string()
            .contains("Store was dropped"));
    }
}
