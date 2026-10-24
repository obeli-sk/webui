use crate::app::Route;
use crate::components::advance_modal::AdvanceModal;
use crate::components::execution_actions::{
    AdvanceButton, CancelExecutionButton, PauseButton, ReplayButton, SubmitStubButton,
    UnpauseButton, UpgradeForm, call_replay, process_replay_response,
};
use crate::components::execution_status::{ExecutionStatus, FinishedStatusMode};
use crate::components::notification::{Notification, NotificationContext};
use crate::model::ExecutionIdExt as _;
use concepts::{
    ComponentType, ExecutionId, FunctionFqn,
    component_id::ComponentDigest,
    storage::{PendingState, VersionType},
};
use gloo::timers::callback::Interval;
use log::{debug, error};
use rest_types::{
    executions::ExecutionWithStateSer,
    replay::{AdvanceErrorSer, CapturedWriteSer, ReplayOutcomeSer, ReplayResponseSer},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Clone, PartialEq)]
struct ExecutionInfo {
    component_type: ComponentType,
    component_digest: ComponentDigest,
    ffqn: FunctionFqn,
}

/// Compute the version of the last event written by the captured writes for the given execution.
fn last_version_of(execution_id: &ExecutionId, writes: &[CapturedWriteSer]) -> Option<VersionType> {
    let eid = execution_id.to_string();
    let last_of_batch = |version: u32, events: usize| version + events.saturating_sub(1) as u32;
    writes.iter().rev().find_map(|write| match write {
        CapturedWriteSer::Append {
            execution_id,
            version,
            ..
        }
        | CapturedWriteSer::AppendFinished {
            execution_id,
            version,
            ..
        } if *execution_id == eid => Some(*version),
        CapturedWriteSer::AppendBatch {
            execution_id,
            version,
            events,
            ..
        }
        | CapturedWriteSer::AppendBatchCreateNewExecution {
            execution_id,
            version,
            events,
            ..
        }
        | CapturedWriteSer::AppendBatchWithDelayResponse {
            execution_id,
            version,
            events,
            ..
        }
        | CapturedWriteSer::AppendStubResponse {
            execution_id,
            version,
            events,
            ..
        } if *execution_id == eid => Some(last_of_batch(*version, events.len())),
        _ => None,
    })
}

#[derive(Properties, PartialEq)]
pub struct ExecutionHeaderProps {
    pub execution_id: ExecutionId,
    pub link: ExecutionLink,
    /// Called after a successful advance with the version of the last written event.
    #[prop_or_default]
    pub on_advanced: Option<Callback<VersionType>>,
    #[prop_or_default]
    pub additional_action: Html,
    #[prop_or_default]
    pub on_finished: Option<Callback<()>>,
}

#[component(ExecutionHeader)]
pub fn execution_header(
    ExecutionHeaderProps {
        execution_id,
        link,
        on_advanced,
        additional_action,
        on_finished,
    }: &ExecutionHeaderProps,
) -> Html {
    let exec_info = use_state(|| None::<ExecutionInfo>);
    let is_finished = use_state(|| false);
    let is_paused = use_state(|| false);
    let is_cancelling = use_state(|| false);

    // Cached replay response for the Advance button
    let cached_replay = use_state(|| None::<ReplayResponseSer>);
    // Captured writes to show in the modal (None = modal closed)
    let modal_writes = use_state(|| None::<Vec<CapturedWriteSer>>);
    // Whether replay returned Blocked — triggers polling
    let is_blocked = use_state(|| false);
    // Guard against overlapping poll requests
    let poll_in_flight = use_mut_ref(|| false);

    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    // Reset state when execution_id changes to prevent stale buttons
    {
        let exec_info = exec_info.clone();
        let is_finished = is_finished.clone();
        let is_paused = is_paused.clone();
        let is_cancelling = is_cancelling.clone();
        let cached_replay = cached_replay.clone();
        let modal_writes = modal_writes.clone();
        let is_blocked = is_blocked.clone();
        use_effect_with(execution_id.clone(), move |_| {
            exec_info.set(None);
            is_finished.set(false);
            is_paused.set(false);
            is_cancelling.set(false);
            cached_replay.set(None);
            modal_writes.set(None);
            is_blocked.set(false);
        });
    }

    // Callback to receive the summary from ExecutionStatus
    let on_summary = {
        let exec_info = exec_info.clone();
        Callback::from(move |summary: ExecutionWithStateSer| {
            exec_info.set(Some(ExecutionInfo {
                component_type: summary.component_type,
                component_digest: summary.component_digest,
                ffqn: summary.ffqn,
            }));
        })
    };

    // Callback when execution finishes - updates is_finished state
    let finished_status = {
        let is_finished = is_finished.clone();
        let on_finished = on_finished.clone();
        FinishedStatusMode::RequestAndNotify(Callback::from(move |()| {
            is_finished.set(true);
            if let Some(on_finished) = &on_finished {
                on_finished.emit(());
            }
        }))
    };

    // Callback when status changes - updates lifecycle state
    let on_status_change = {
        let is_paused = is_paused.clone();
        let is_cancelling = is_cancelling.clone();
        Callback::from(move |status: PendingState| {
            is_paused.set(matches!(status, PendingState::Paused(_)));
            is_cancelling.set(matches!(status, PendingState::Cancelling(_)));
        })
    };

    let workflow_digest = exec_info.as_ref().and_then(|exec_info| {
        if exec_info.component_type == ComponentType::Workflow {
            Some(exec_info.component_digest.clone())
        } else {
            None
        }
    });

    let is_activity = exec_info.as_ref().is_some_and(|exec_info| {
        matches!(
            exec_info.component_type,
            ComponentType::Activity | ComponentType::ActivityStub
        )
    });

    let is_cron = exec_info
        .as_ref()
        .is_some_and(|exec_info| exec_info.component_type == ComponentType::Cron);

    let is_cancellable_workflow = exec_info.as_ref().is_some_and(|exec_info| {
        exec_info.component_type == ComponentType::Workflow && exec_info.ffqn.is_cancellable()
    });

    let stub_info = exec_info.as_ref().and_then(|exec_info| {
        if exec_info.component_type == ComponentType::ActivityStub {
            Some(exec_info.ffqn.clone())
        } else {
            None
        }
    });

    // Poll replay every 1s while execution is blocked
    {
        let is_blocked = is_blocked.clone();
        let poll_in_flight = poll_in_flight.clone();
        let execution_id = execution_id.clone();
        let modal_writes = modal_writes.clone();
        let cached_replay = cached_replay.clone();
        let notifications = notifications.clone();
        use_effect_with(*is_blocked, move |blocked| {
            let interval = if !blocked {
                None
            } else {
                Some(Interval::new(1_000, move || {
                    if *poll_in_flight.borrow() {
                        return;
                    }
                    *poll_in_flight.borrow_mut() = true;

                    let execution_id = execution_id.clone();
                    let modal_writes = modal_writes.clone();
                    let cached_replay = cached_replay.clone();
                    let notifications = notifications.clone();
                    let is_blocked = is_blocked.clone();
                    let poll_in_flight = poll_in_flight.clone();

                    spawn_local(async move {
                        if let Some(response) = call_replay(&execution_id, &notifications).await {
                            cached_replay.set(Some(response.clone()));
                            match &response.outcome {
                                ReplayOutcomeSer::Advanceable { captured_writes }
                                    if !captured_writes.is_empty() =>
                                {
                                    modal_writes.set(Some(captured_writes.clone()));
                                    is_blocked.set(false);
                                }
                                ReplayOutcomeSer::Finished { .. } => {
                                    notifications.push(Notification::info("Execution finished"));
                                    modal_writes.set(None);
                                    is_blocked.set(false);
                                }
                                ReplayOutcomeSer::Blocked => {
                                    // Still blocked, continue polling
                                }
                                ReplayOutcomeSer::ReplayFailed { error, .. } => {
                                    notifications.push(Notification::error(format!(
                                        "Replay failed: {error}"
                                    )));
                                    modal_writes.set(None);
                                    is_blocked.set(false);
                                }
                                ReplayOutcomeSer::Advanceable { .. } => {
                                    // Empty advanceable — keep polling
                                }
                            }
                        }
                        *poll_in_flight.borrow_mut() = false;
                    });
                }))
            };

            move || drop(interval)
        });
    }

    let tab_link = |tab: ExecutionLink, title: &str| {
        tab.link_with_classes(
            execution_id.clone(),
            title,
            classes!((tab == *link).then_some("active")),
        )
    };

    html! {
        <div class="execution-header">
            <div class="execution-heading">
                <h3>{ execution_id.render_execution_parts(false, *link) }</h3>
                <div class="execution-heading-status">
                    <ExecutionStatus execution_id={execution_id.clone()} status={None} {finished_status} on_summary={on_summary} on_status_change={on_status_change} />
                </div>
            </div>
            <div class="view-tabs execution-detail-tabs">
                { tab_link(ExecutionLink::Trace, "Trace") }
                { tab_link(ExecutionLink::ExecutionLog, "Execution Log") }
                { tab_link(ExecutionLink::Debug, "Debugger") }
                { tab_link(ExecutionLink::Logs, "App Logs") }
            </div>

            if let Some(workflow_digest) = workflow_digest {
                <div class="execution-actions">
                    if !*is_finished && !*is_cancelling {
                        <PauseButton
                            execution_id={execution_id.clone()}
                            is_paused={*is_paused}
                        />
                        <UnpauseButton
                            execution_id={execution_id.clone()}
                            is_paused={*is_paused}
                        />
                        <UpgradeForm
                            execution_id={execution_id.clone()}
                            current_digest={workflow_digest}
                            ffqn={exec_info.as_ref().map(|info| info.ffqn.clone())}
                        />
                        if is_cancellable_workflow {
                            <CancelExecutionButton
                                execution_id={execution_id.clone()}
                            />
                        }
                    }
                    <ReplayButton
                        execution_id={execution_id.clone()}
                        on_replay_response={
                            let cached_replay = cached_replay.clone();
                            Callback::from(move |resp: ReplayResponseSer| {
                                cached_replay.set(Some(resp));
                            })
                        }
                    />
                    {additional_action.clone()}
                    if !*is_finished && !*is_cancelling && *is_paused {
                        <AdvanceButton
                            execution_id={execution_id.clone()}
                            cached_replay_response={(*cached_replay).clone()}
                            on_open_modal={
                                let modal_writes = modal_writes.clone();
                                Callback::from(move |writes: Vec<CapturedWriteSer>| {
                                    modal_writes.set(Some(writes));
                                })
                            }
                        />
                    }
                </div>
            }

            // Advance modal
            if let Some(writes) = (*modal_writes).clone() {
                <AdvanceModal
                    execution_id={execution_id.clone()}
                    captured_writes={writes}
                    is_blocked={*is_blocked}
                    on_advance={
                        let execution_id = execution_id.clone();
                        let modal_writes = modal_writes.clone();
                        let cached_replay = cached_replay.clone();
                        let notifications = notifications.clone();
                        let is_blocked = is_blocked.clone();
                        let on_advanced = on_advanced.clone();
                        Callback::from(move |writes: Vec<CapturedWriteSer>| {
                            let execution_id = execution_id.clone();
                            let modal_writes = modal_writes.clone();
                            let cached_replay = cached_replay.clone();
                            let notifications = notifications.clone();
                            let is_blocked = is_blocked.clone();
                            let on_advanced = on_advanced.clone();
                            let advanced_version = last_version_of(&execution_id, &writes);
                            spawn_local(async move {
                                let result = crate::rest::replay::advance(&execution_id, writes).await;
                                match result {
                                    Ok(Ok(_)) => {
                                        debug!("Advance succeeded for {execution_id}");
                                        notifications.push(Notification::success("Advance succeeded"));
                                        if let Some(version) = advanced_version
                                            && let Some(cb) = &on_advanced
                                        {
                                            cb.emit(version);
                                        }
                                        // Replay again to check next state
                                        if let Some(response) =
                                            call_replay(&execution_id, &notifications).await
                                        {
                                            cached_replay.set(Some(response.clone()));
                                            match &response.outcome {
                                                ReplayOutcomeSer::Advanceable { captured_writes }
                                                    if !captured_writes.is_empty() =>
                                                {
                                                    modal_writes.set(Some(captured_writes.clone()));
                                                    return;
                                                }
                                                ReplayOutcomeSer::Blocked => {
                                                    is_blocked.set(true);
                                                    return;
                                                }
                                                _ => {
                                                    process_replay_response(&response, &notifications);
                                                }
                                            }
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        let msg = match e {
                                            AdvanceErrorSer::VersionMismatch { expected } => {
                                                format!("Version mismatch (expected {expected})")
                                            }
                                            AdvanceErrorSer::ReplayMismatch => {
                                                "Replay mismatch".to_string()
                                            }
                                            AdvanceErrorSer::Transient(message) => {
                                                format!("Transient error: {message}")
                                            }
                                        };
                                        notifications.push(Notification::error(msg));
                                    }
                                    Err(e) => {
                                        error!("Advance failed for {execution_id}: {e:?}");
                                        notifications.push(Notification::error(e));
                                    }
                                }
                                // Close modal and invalidate cache
                                modal_writes.set(None);
                                cached_replay.set(None);
                            });
                        })
                    }
                    on_unpause={
                        let execution_id = execution_id.clone();
                        let modal_writes = modal_writes.clone();
                        let cached_replay = cached_replay.clone();
                        let is_blocked = is_blocked.clone();
                        let notifications = notifications.clone();
                        Callback::from(move |()| {
                            let execution_id = execution_id.clone();
                            let modal_writes = modal_writes.clone();
                            let cached_replay = cached_replay.clone();
                            let is_blocked = is_blocked.clone();
                            let notifications = notifications.clone();
                            spawn_local(async move {
                                match crate::rest::put_action(&format!(
                                    "/v1/executions/{execution_id}/unpause"
                                )).await {
                                    Ok(_) => {
                                        debug!("Unpause requested for execution {}", execution_id);
                                        notifications.push(Notification::success(
                                            "Execution unpaused successfully",
                                        ));
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to unpause execution {}: {:?}",
                                            execution_id, e
                                        );
                                        notifications.push(Notification::error(e));
                                    }
                                }
                                modal_writes.set(None);
                                cached_replay.set(None);
                                is_blocked.set(false);
                            });
                        })
                    }
                    on_close={
                        let modal_writes = modal_writes.clone();
                        let is_blocked = is_blocked.clone();
                        Callback::from(move |()| {
                            modal_writes.set(None);
                            is_blocked.set(false);
                        })
                    }
                />
            }

            if is_activity && !*is_finished && !*is_cancelling {
                <div class="execution-actions">
                    <PauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                    <UnpauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                    <CancelExecutionButton
                        execution_id={execution_id.clone()}
                    />
                    if let Some(ffqn) = &stub_info {
                        <SubmitStubButton
                            execution_id={execution_id.clone()}
                            ffqn={ffqn.clone()}
                        />
                    }
                </div>
            }

            if is_cron && !*is_finished {
                <div class="execution-actions">
                    <PauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                    <UnpauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                </div>
            }
        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionLink {
    Trace,
    ExecutionLog,
    Debug,
    Logs,
}

impl ExecutionLink {
    pub fn link(self, execution_id: ExecutionId, title: &str) -> Html {
        self.link_with_classes(execution_id, title, Classes::new())
    }

    fn link_with_classes(self, execution_id: ExecutionId, title: &str, classes: Classes) -> Html {
        let route = match self {
            ExecutionLink::Trace => Route::ExecutionTrace { execution_id },
            ExecutionLink::ExecutionLog => Route::ExecutionLog { execution_id },
            ExecutionLink::Debug => Route::ExecutionDebugger { execution_id },
            ExecutionLink::Logs => Route::Logs { execution_id },
        };
        html! {
            <Link<Route> {classes} to={route}>
                {title}
            </Link<Route>>
        }
    }
}
