use crate::{
    components::execution_detail::finished::FinishedEvent,
    model::{FinishedResultKind, FinishedStatus, StatusMessage},
    status_hub::StatusHub,
    util::{
        time::{TimeGranularity, format_date, relative_time, use_relative_now},
        trace_id,
    },
};
use chrono::{DateTime, Utc};
use concepts::{
    ExecutionId,
    storage::{PendingState, PendingStateBlockedByJoinSet, PendingStateFinished},
};
use futures::StreamExt as _;
use gloo::timers::future::TimeoutFuture;
use hashbrown::HashMap;
use log::{debug, error, trace};
use rest_types::executions::ExecutionWithStateSer;
use std::rc::Rc;
use yew::prelude::*;

const FINISHED_RETRY_MS: u32 = 5_000;

/// Controls how the ExecutionStatus component handles finished status
#[derive(Clone, PartialEq, Default)]
pub enum FinishedStatusMode {
    /// Do not request detailed finished status from the server
    #[default]
    Skip,
    /// Request detailed finished status from the server
    Request,
    /// Request detailed finished status and invoke callback when execution finishes
    RequestAndNotify(Callback<()>),
}

#[derive(Properties, PartialEq)]
pub struct ExecutionStatusProps {
    pub status: Option<PendingState>,
    pub execution_id: ExecutionId,
    /// Controls whether to request detailed finished status and optional callback
    #[prop_or_default]
    pub finished_status: FinishedStatusMode,
    /// Called when the summary is received from the status stream
    #[prop_or_default]
    pub on_summary: Option<Callback<ExecutionWithStateSer>>,
    /// Called when the pending state changes (provides the current status)
    #[prop_or_default]
    pub on_status_change: Option<Callback<PendingState>>,
}

pub enum StatusStateAction {
    Update {
        execution_id: ExecutionId,
        message: StatusMessage,
    },
}

#[derive(Default, PartialEq)]
pub struct StatusState {
    pub statuses: HashMap<ExecutionId, StatusMessage>,
    /// Last `Summary` message seen per execution. Kept separately because the
    /// immutable component metadata it carries is otherwise lost once a later
    /// `CurrentStatus`/`Finished` message overwrites `statuses`.
    pub summaries: HashMap<ExecutionId, ExecutionWithStateSer>,
}

impl Reducible for StatusState {
    type Action = StatusStateAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            StatusStateAction::Update {
                execution_id,
                message,
            } => {
                let mut statuses = self.statuses.clone();
                let mut summaries = self.summaries.clone();
                if let StatusMessage::Summary(summary) = &message {
                    summaries.insert(execution_id.clone(), summary.clone());
                }
                statuses.insert(execution_id, message);
                Self {
                    statuses,
                    summaries,
                }
                .into()
            }
        }
    }
}

pub type StatusCacheContext = UseReducerHandle<StatusState>;

fn is_finished_detailed(msg: &StatusMessage) -> bool {
    matches!(msg, StatusMessage::Finished(_))
}

fn is_finished_any(msg: &StatusMessage) -> bool {
    matches!(
        msg,
        StatusMessage::Finished(_)
            | StatusMessage::CurrentStatus(PendingState::Finished(_))
            | StatusMessage::Summary(ExecutionWithStateSer {
                pending_state: PendingState::Finished(_),
                ..
            })
    )
}

/// Extracts the execution status from a status message
pub fn extract_status(msg: &StatusMessage) -> Option<PendingState> {
    match msg {
        StatusMessage::CurrentStatus(status) => Some(status.clone()),
        StatusMessage::Summary(summary) => Some(summary.pending_state.clone()),
        StatusMessage::Finished(_) => None,
    }
}

async fn run_status_subscription(
    hub: StatusHub,
    status_state: UseReducerHandle<StatusState>,
    connection_id: Rc<str>,
    execution_id: ExecutionId,
    finished_status: FinishedStatusMode,
    on_summary: Option<Callback<ExecutionWithStateSer>>,
    on_status_change: Option<Callback<PendingState>>,
) {
    let send_finished_status = !matches!(finished_status, FinishedStatusMode::Skip);
    let (_watch, mut updates) = hub.subscribe(&execution_id);
    let mut last_status = None;
    loop {
        let Some(update) = updates.next().await else {
            error!("[{connection_id}] Status of {execution_id} is not available");
            break;
        };
        let summary = &update.status;
        let current_status = Some(summary.pending_state.clone());
        if last_status.is_none() {
            if let Some(callback) = &on_summary {
                callback.emit(summary.clone());
            }
            status_state.dispatch(StatusStateAction::Update {
                execution_id: execution_id.clone(),
                message: StatusMessage::Summary(summary.clone()),
            });
        }
        if current_status != last_status {
            if let (Some(callback), Some(status)) = (&on_status_change, &current_status) {
                callback.emit(status.clone());
            }
            status_state.dispatch(StatusStateAction::Update {
                execution_id: execution_id.clone(),
                message: StatusMessage::CurrentStatus(summary.pending_state.clone()),
            });
            last_status = current_status;
        }
        if matches!(last_status, Some(PendingState::Finished(_))) {
            if send_finished_status {
                // Retried until loaded, no further status updates come.
                loop {
                    match crate::rest::executions::finished_status(summary).await {
                        Ok(detail) => {
                            status_state.dispatch(StatusStateAction::Update {
                                execution_id: execution_id.clone(),
                                message: StatusMessage::Finished(detail),
                            });
                            break;
                        }
                        Err(err) => {
                            error!(
                                "[{connection_id}] Failed to load finished status for {execution_id}: {err}"
                            );
                            TimeoutFuture::new(FINISHED_RETRY_MS).await;
                        }
                    }
                }
            }
            if let FinishedStatusMode::RequestAndNotify(ref callback) = finished_status {
                callback.emit(());
            }
            break;
        }
    }
    debug!("[{connection_id}] <ExecutionStatus /> Ended subscription");
}

#[component(ExecutionStatus)]
pub fn execution_status(
    ExecutionStatusProps {
        status,
        execution_id,
        finished_status,
        on_summary,
        on_status_change,
    }: &ExecutionStatusProps,
) -> Html {
    let finished_status = finished_status.clone();
    let on_summary = on_summary.clone();
    let on_status_change = on_status_change.clone();
    let request_finished_status = !matches!(finished_status, FinishedStatusMode::Skip);

    let hub = use_context::<StatusHub>().expect("StatusHub should be provided by App");
    // Both hooks must be called unconditionally
    let context_state = use_context::<StatusCacheContext>();
    let local_state = use_reducer_eq(StatusState::default);

    // Prefer shared context if available, otherwise use local state
    let status_state = context_state.unwrap_or(local_state);

    // Sync status from props to state (if provided and better than current)
    use_effect_with((execution_id.clone(), status.clone()), {
        let status_state = status_state.clone();
        move |(execution_id, status)| {
            if let Some(status) = status {
                status_state.dispatch(StatusStateAction::Update {
                    execution_id: execution_id.clone(),
                    message: StatusMessage::CurrentStatus(status.clone()),
                });
            }
        }
    });

    let stored_message = status_state.statuses.get(execution_id).cloned();

    // Determine if we have a sufficient finished status to avoid re-subscribing.
    let is_done = if let Some(msg) = &stored_message {
        if request_finished_status {
            is_finished_detailed(msg)
        } else {
            is_finished_any(msg)
        }
    } else {
        false
    };

    // Subscription Effect
    {
        let status_state = status_state.clone();
        let hub = hub.clone();
        let execution_id = execution_id.clone();
        let connection_id = trace_id();
        let on_summary = on_summary.clone();
        let on_status_change = on_status_change.clone();
        let finished_status = finished_status.clone();
        // Cached state to replay when the subscription is skipped (e.g. after
        // navigating back to an already-finished execution). Consumers reset
        // their derived state on `execution_id` change and rely on these
        // callbacks to repopulate it.
        let cached_summary = status_state.summaries.get(&execution_id).cloned();
        let cached_status = stored_message.as_ref().and_then(extract_status);
        let cached_finished = stored_message.as_ref().is_some_and(is_finished_any);

        use_effect_with(
            (execution_id.clone(), is_done),
            move |(execution_id, is_done)| {
                let execution_id = execution_id.clone();
                let subscription = if *is_done {
                    trace!(
                        "[{connection_id}] Execution {execution_id} status is finished. Skipping subscription."
                    );
                    // Deferred so it runs after synchronous effects (e.g. a
                    // parent clearing its state on `execution_id` change).
                    wasm_bindgen_futures::spawn_local(async move {
                        if let (Some(callback), Some(summary)) = (&on_summary, &cached_summary) {
                            callback.emit(summary.clone());
                        }
                        if let (Some(callback), Some(status)) = (&on_status_change, &cached_status)
                        {
                            callback.emit(status.clone());
                        }
                        if cached_finished
                            && let FinishedStatusMode::RequestAndNotify(callback) = &finished_status
                        {
                            callback.emit(());
                        }
                    });
                    None
                } else {
                    debug!(
                        "[{connection_id}] <ExecutionStatus /> Subscribing to status of {execution_id}"
                    );

                    let (task, subscription) = futures::future::abortable(run_status_subscription(
                        hub,
                        status_state,
                        connection_id,
                        execution_id.clone(),
                        finished_status.clone(),
                        on_summary.clone(),
                        on_status_change.clone(),
                    ));
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = task.await;
                    });
                    Some(subscription)
                };

                move || {
                    if let Some(subscription) = subscription {
                        trace!("Cleaning up {execution_id}");
                        subscription.abort();
                    }
                }
            },
        );
    }

    match stored_message.as_ref() {
        None => {
            html! {
                {"Loading..."}
            }
        }
        Some(StatusMessage::Summary(ExecutionWithStateSer {
            pending_state: status,
            ..
        }))
        | Some(StatusMessage::CurrentStatus(status)) => status_to_string(status),
        Some(StatusMessage::Finished(FinishedStatus {
            scheduled_at,
            finished_at,
            retval,
        })) => {
            let since_scheduled = relative_time(*scheduled_at, *finished_at, TimeGranularity::Fine);
            html! {<>
                <FinishedEvent result_detail={retval.clone()} version={None} is_selected={false}/>
                <p>{format!("Completed in {since_scheduled}.")}</p>
            </>}
        }
    }
}

pub fn status_to_string(status: &PendingState) -> Html {
    match status {
        PendingState::Locked(locked) => html! {
            <RelativeStatus target={locked.lock_expires_at} kind={RelativeStatusKind::Locked} />
        },
        PendingState::PendingAt(pending_at) => html! {
            <RelativeStatus target={pending_at.scheduled_at} kind={RelativeStatusKind::Pending} />
        },
        PendingState::BlockedByJoinSet(PendingStateBlockedByJoinSet { join_set_id, .. }) => {
            Html::from(format!("Blocked by {join_set_id}"))
        }
        PendingState::Finished(PendingStateFinished { result_kind, .. }) => {
            Html::from(FinishedResultKind::from(result_kind).to_string())
        }
        PendingState::Paused(_) => html! {"Paused"},
        PendingState::Cancelling(_) => html! {"Cancelling"},
    }
}

#[derive(Clone, Copy, PartialEq)]
enum RelativeStatusKind {
    /// `PendingAt`: future = not yet runnable, past = waiting for an executor.
    Pending,
    /// `Locked`: future = lock still held, past = lock expired but not yet transitioned.
    Locked,
}
impl RelativeStatusKind {
    fn label(self, target: DateTime<Utc>, now: DateTime<Utc>) -> String {
        match (self, target > now) {
            (RelativeStatusKind::Pending, true) => format!(
                "Scheduled in {}",
                relative_time(now, target, TimeGranularity::Coarse)
            ),
            (RelativeStatusKind::Pending, false) => format!(
                "Pending for {}",
                relative_time(target, now, TimeGranularity::Coarse)
            ),
            (RelativeStatusKind::Locked, true) => format!(
                "Locked for another {}",
                relative_time(now, target, TimeGranularity::Coarse)
            ),
            (RelativeStatusKind::Locked, false) => format!(
                "Locked, expired {} ago",
                relative_time(target, now, TimeGranularity::Coarse)
            ),
        }
    }
}

#[derive(Properties, PartialEq)]
struct RelativeStatusProps {
    target: DateTime<Utc>,
    kind: RelativeStatusKind,
}

/// Renders a status label relative to the browser clock, ticking itself so the
/// value stays fresh while the underlying execution status doesn't change.
#[function_component(RelativeStatus)]
fn relative_status(props: &RelativeStatusProps) -> Html {
    let now = use_relative_now(props.target);
    html! { <span title={format_date(props.target)}>{ props.kind.label(props.target, now) }</span> }
}
