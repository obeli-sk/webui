//! Status of every watched unfinished execution over one `status/batch?follow=true` stream
//! per tab, instead of a poll loop per execution. Browsers allow about 6 connections per
//! origin, so streams per execution would not scale.

use crate::rest::sse::SseStream;
use concepts::{ExecutionId, storage::PendingState};
use futures::{
    FutureExt as _,
    channel::{mpsc, oneshot},
    future::{AbortHandle, abortable},
};
use gloo::{net::http::Request, timers::future::TimeoutFuture};
use hashbrown::{HashMap, HashSet};
use log::{debug, warn};
use rest_types::executions::ExecutionWithStateSer;
use serde::Deserialize;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Server limit of executions per batch.
const CHUNK: usize = 200;
const ADDED_DELAY_MS: u32 = 10;
/// Removals wait so that a quick re-add (page re-render) does not reconnect.
const REMOVED_DELAY_MS: u32 = 2_000;
const RECONNECT_MS: (u32, u32) = (1_000, 30_000);
/// Upper bound of `changed`, in case an update is never seen.
const CHANGED_TIMEOUT_MS: u32 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StatusUpdate {
    #[serde(flatten)]
    pub status: ExecutionWithStateSer,
    pub max_version: u32,
}

impl StatusUpdate {
    fn is_finished(&self) -> bool {
        matches!(self.status.pending_state, PendingState::Finished(_))
    }
}

type Listener = (u64, mpsc::UnboundedSender<Rc<StatusUpdate>>);

#[derive(Default)]
struct Inner {
    interest: HashMap<ExecutionId, usize>,
    listeners: HashMap<ExecutionId, Vec<Listener>>,
    waiters: HashMap<ExecutionId, Vec<oneshot::Sender<()>>>,
    latest: HashMap<ExecutionId, Rc<StatusUpdate>>,
    /// Bumped on every change of an execution's status or max version.
    seq: HashMap<ExecutionId, u64>,
    /// Finished or missing, not streamed again.
    done: HashSet<ExecutionId>,
    streaming: HashSet<ExecutionId>,
    streams: Vec<AbortHandle>,
    scheduled_at: Option<f64>,
    schedule_generation: u64,
    reconnect_ms: u32,
    next_listener: u64,
}

#[derive(Clone, Default)]
pub struct StatusHub(Rc<RefCell<Inner>>);

impl PartialEq for StatusHub {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Keeps an execution followed until dropped.
pub struct StatusWatch {
    hub: Weak<RefCell<Inner>>,
    execution_id: ExecutionId,
    listener: Option<u64>,
}

impl Drop for StatusWatch {
    fn drop(&mut self) {
        let Some(inner) = self.hub.upgrade() else {
            return;
        };
        let hub = StatusHub(inner);
        {
            let mut inner = hub.0.borrow_mut();
            if let Some(key) = self.listener
                && let Some(listeners) = inner.listeners.get_mut(&self.execution_id)
            {
                listeners.retain(|(listener, _)| *listener != key);
                if listeners.is_empty() {
                    inner.listeners.remove(&self.execution_id);
                }
            }
            if let Some(count) = inner.interest.get_mut(&self.execution_id) {
                *count -= 1;
                if *count == 0 {
                    inner.interest.remove(&self.execution_id);
                }
            }
        }
        hub.schedule(REMOVED_DELAY_MS);
    }
}

impl StatusHub {
    pub fn watch(&self, execution_id: &ExecutionId) -> StatusWatch {
        *self
            .0
            .borrow_mut()
            .interest
            .entry(execution_id.clone())
            .or_default() += 1;
        self.schedule(ADDED_DELAY_MS);
        StatusWatch {
            hub: Rc::downgrade(&self.0),
            execution_id: execution_id.clone(),
            listener: None,
        }
    }

    /// Watches the execution and receives its updates, starting with the latest known one.
    /// The receiver closes if the execution does not exist.
    pub fn subscribe(
        &self,
        execution_id: &ExecutionId,
    ) -> (StatusWatch, mpsc::UnboundedReceiver<Rc<StatusUpdate>>) {
        let mut watch = self.watch(execution_id);
        let (sender, receiver) = mpsc::unbounded();
        let mut inner = self.0.borrow_mut();
        let latest = inner.latest.get(execution_id).cloned();
        let done = inner.done.contains(execution_id);
        // Only a streamed or finished status is current.
        if (done || inner.streaming.contains(execution_id))
            && let Some(latest) = &latest
        {
            let _ = sender.unbounded_send(latest.clone());
        }
        if !done || latest.is_some() {
            let key = inner.next_listener;
            inner.next_listener += 1;
            inner
                .listeners
                .entry(execution_id.clone())
                .or_default()
                .push((key, sender));
            watch.listener = Some(key);
        }
        (watch, receiver)
    }

    /// Change counter of the execution, to be passed to `changed`.
    pub fn seq(&self, execution_id: &ExecutionId) -> u64 {
        self.0
            .borrow()
            .seq
            .get(execution_id)
            .copied()
            .unwrap_or_default()
    }

    /// Resolves once the execution changed after `since`, or is finished or missing.
    /// The execution must be watched to see changes.
    pub async fn changed(&self, execution_id: &ExecutionId, since: u64) {
        let receiver = {
            let mut inner = self.0.borrow_mut();
            if inner.seq.get(execution_id).copied().unwrap_or_default() > since
                || inner.done.contains(execution_id)
            {
                return;
            }
            let (sender, receiver) = oneshot::channel();
            inner
                .waiters
                .entry(execution_id.clone())
                .or_default()
                .push(sender);
            receiver
        };
        futures::select! {
            _ = receiver.fuse() => {},
            () = TimeoutFuture::new(CHANGED_TIMEOUT_MS).fuse() => {},
        }
    }

    /// Reconciles after `delay_ms` unless an earlier reconcile is already scheduled.
    fn schedule(&self, delay_ms: u32) {
        let at = js_sys::Date::now() + f64::from(delay_ms);
        let generation = {
            let mut inner = self.0.borrow_mut();
            if inner
                .scheduled_at
                .is_some_and(|scheduled_at| scheduled_at <= at)
            {
                return;
            }
            inner.scheduled_at = Some(at);
            inner.schedule_generation += 1;
            inner.schedule_generation
        };
        let hub = Rc::downgrade(&self.0);
        wasm_bindgen_futures::spawn_local(async move {
            TimeoutFuture::new(delay_ms).await;
            if let Some(inner) = hub.upgrade()
                && inner.borrow().schedule_generation == generation
            {
                inner.borrow_mut().scheduled_at = None;
                StatusHub(inner).reconcile();
            }
        });
    }

    /// Restarts the streams if a watched execution is not covered, stops them when nothing
    /// is watched. Executions that are no longer watched stay in the streams until then.
    fn reconcile(&self) {
        let mut inner = self.0.borrow_mut();
        let inner = &mut *inner;
        let desired: Vec<_> = inner
            .interest
            .keys()
            .filter(|execution_id| !inner.done.contains(*execution_id))
            .cloned()
            .collect();
        if desired.is_empty() {
            for stream in inner.streams.drain(..) {
                stream.abort();
            }
            inner.streaming.clear();
        } else if !desired
            .iter()
            .all(|execution_id| inner.streaming.contains(execution_id))
        {
            debug!("Following status of {} executions", desired.len());
            for stream in inner.streams.drain(..) {
                stream.abort();
            }
            // Statuses that were not streamed are stale, the new streams resend them.
            inner.latest.retain(|execution_id, _| {
                inner.done.contains(execution_id) || inner.streaming.contains(execution_id)
            });
            inner.streaming = desired.iter().cloned().collect();
            for chunk in desired.chunks(CHUNK) {
                let (task, handle) = abortable(run_stream(Rc::downgrade(&self.0), chunk.to_vec()));
                inner.streams.push(handle);
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = task.await;
                });
            }
        }
    }

    fn apply(&self, chunk: &[ExecutionId], updates: Vec<StatusUpdate>, first: bool) {
        let mut inner = self.0.borrow_mut();
        inner.reconnect_ms = 0;
        if first {
            // The first event lists every existing execution.
            for execution_id in chunk.iter().filter(|execution_id| {
                !updates
                    .iter()
                    .any(|update| update.status.execution_id == **execution_id)
            }) {
                debug!("Execution {execution_id} not found");
                inner.done.insert(execution_id.clone());
                inner.listeners.remove(execution_id);
                inner.waiters.remove(execution_id);
            }
        }
        for update in updates {
            let execution_id = update.status.execution_id.clone();
            if update.is_finished() {
                inner.done.insert(execution_id.clone());
            }
            if inner
                .latest
                .get(&execution_id)
                .is_none_or(|latest| **latest != update)
            {
                let update = Rc::new(update);
                inner.latest.insert(execution_id.clone(), update.clone());
                *inner.seq.entry(execution_id.clone()).or_default() += 1;
                if let Some(listeners) = inner.listeners.get_mut(&execution_id) {
                    listeners.retain(|(_, sender)| sender.unbounded_send(update.clone()).is_ok());
                }
                for waiter in inner.waiters.remove(&execution_id).into_iter().flatten() {
                    let _ = waiter.send(());
                }
            }
        }
    }
}

async fn run_stream(hub: Weak<RefCell<Inner>>, execution_ids: Vec<ExecutionId>) {
    let result = follow(&hub, &execution_ids).await;
    let Some(inner) = hub.upgrade() else {
        return;
    };
    let hub = StatusHub(inner);
    let delay_ms = {
        let mut inner = hub.0.borrow_mut();
        for execution_id in &execution_ids {
            inner.streaming.remove(execution_id);
        }
        match result {
            // Everything finished or missing.
            Ok(()) => ADDED_DELAY_MS,
            Err(err) => {
                let delay_ms = inner.reconnect_ms.clamp(RECONNECT_MS.0, RECONNECT_MS.1);
                warn!("Status stream failed, reconnecting in {delay_ms}ms: {err}");
                inner.reconnect_ms = delay_ms.saturating_mul(2);
                delay_ms
            }
        }
    };
    hub.schedule(delay_ms);
}

async fn follow(hub: &Weak<RefCell<Inner>>, execution_ids: &[ExecutionId]) -> Result<(), String> {
    let request = Request::post(&format!("{}/v1/executions/status/batch", crate::BASE_URL))
        .query([("follow", "true")]);
    let body = serde_json::json!({ "execution_ids": execution_ids });
    let mut stream = SseStream::open(request, Some(&body)).await?;
    let mut first = true;
    while let Some(event) = stream.next().await? {
        if event.event == "statuses" {
            let updates: Vec<StatusUpdate> =
                serde_json::from_str(&event.data).map_err(|err| err.to_string())?;
            let Some(inner) = hub.upgrade() else {
                return Ok(());
            };
            StatusHub(inner).apply(execution_ids, updates, first);
            first = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_batch_item() {
        let update: StatusUpdate = serde_json::from_str(
            r#"{
                "execution_id":"E_01KABCDEFGHJKMNPQRSTVWXYZ0", "ffqn":"test:pkg/ifc.run",
                "pending_state":{"status":"finished", "version":4,
                    "finished_at":"2026-09-25T12:00:01Z", "result_kind":"ok"},
                "created_at":"2026-09-25T12:00:00Z",
                "first_scheduled_at":"2026-09-25T12:00:00Z",
                "component_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "component_type":"workflow",
                "deployment_id":"Dep_01KABCDEFGHJKMNPQRSTVWXYZ0",
                "max_version":4
            }"#,
        )
        .unwrap();
        assert!(update.is_finished());
        assert_eq!(update.max_version, 4);
    }
}
