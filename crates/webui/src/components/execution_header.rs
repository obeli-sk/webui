use crate::BASE_URL;
use crate::app::Route;
use crate::components::advance_modal::AdvanceModal;
use crate::components::execution_actions::{
    AdvanceButton, CancelActivityButton, PauseButton, ReplayButton, SubmitStubButton,
    UnpauseButton, UpgradeForm, call_replay, process_replay_response,
};
use crate::components::execution_list_page::ExecutionQuery;
use crate::components::execution_status::{ExecutionStatus, FinishedStatusMode};
use crate::components::notification::{Notification, NotificationContext};
use crate::grpc::ffqn::FunctionFqn;
use crate::grpc::grpc_client::{
    self, CapturedWrite, ComponentType, ContentDigest, ExecutionId, ExecutionSummary,
    captured_write, execution_repository_client::ExecutionRepositoryClient, execution_status,
};
use crate::grpc::version::VersionType;
use gloo::timers::callback::Interval;
use log::{debug, error};
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Clone, PartialEq, Default)]
struct ExecutionInfo {
    component_type: ComponentType,
    component_digest: ContentDigest,
    ffqn: Option<FunctionFqn>,
}

/// Compute the version of the last event written by the captured writes for the given execution.
fn last_version_of(execution_id: &ExecutionId, writes: &[CapturedWrite]) -> Option<VersionType> {
    let eid = &execution_id.id;
    writes.iter().rev().find_map(|cw| match &cw.write {
        Some(captured_write::Write::Append(a))
            if a.execution_id.as_ref().is_some_and(|id| id.id == *eid) =>
        {
            Some(a.version)
        }
        Some(captured_write::Write::AppendBatch(b))
            if b.execution_id.as_ref().is_some_and(|id| id.id == *eid) =>
        {
            Some(b.version + b.events.len().saturating_sub(1) as u32)
        }
        Some(captured_write::Write::AppendBatchCreateNewExecution(b))
            if b.execution_id.as_ref().is_some_and(|id| id.id == *eid) =>
        {
            Some(b.version + b.events.len().saturating_sub(1) as u32)
        }
        Some(captured_write::Write::AppendFinished(f))
            if f.execution_id.as_ref().is_some_and(|id| id.id == *eid) =>
        {
            Some(f.version)
        }
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
}

#[component(ExecutionHeader)]
pub fn execution_header(
    ExecutionHeaderProps {
        execution_id,
        link,
        on_advanced,
    }: &ExecutionHeaderProps,
) -> Html {
    let exec_info = use_state(|| None::<ExecutionInfo>);
    let is_finished = use_state(|| false);
    let is_paused = use_state(|| false);

    // Cached replay response for the Advance button
    let cached_replay = use_state(|| None::<grpc_client::ReplayExecutionResponse>);
    // Captured writes to show in the modal (None = modal closed)
    let modal_writes = use_state(|| None::<Vec<CapturedWrite>>);
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
        let cached_replay = cached_replay.clone();
        let modal_writes = modal_writes.clone();
        let is_blocked = is_blocked.clone();
        use_effect_with(execution_id.clone(), move |_| {
            exec_info.set(None);
            is_finished.set(false);
            is_paused.set(false);
            cached_replay.set(None);
            modal_writes.set(None);
            is_blocked.set(false);
        });
    }

    // Callback to receive the summary from ExecutionStatus
    let on_summary = {
        let exec_info = exec_info.clone();
        Callback::from(move |summary: ExecutionSummary| {
            exec_info.set(Some(ExecutionInfo {
                component_type: summary.component_type(),
                component_digest: summary.component_digest.unwrap(),
                ffqn: summary.function_name.map(FunctionFqn::from),
            }));
        })
    };

    // Callback when execution finishes - updates is_finished state
    let finished_status = {
        let is_finished = is_finished.clone();
        FinishedStatusMode::RequestAndNotify(Callback::from(move |()| {
            is_finished.set(true);
        }))
    };

    // Callback when status changes - updates is_paused state
    let on_status_change = {
        let is_paused = is_paused.clone();
        Callback::from(move |status: execution_status::Status| {
            is_paused.set(matches!(status, execution_status::Status::Paused(_)));
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

    let stub_info = exec_info.as_ref().and_then(|exec_info| {
        if exec_info.component_type == ComponentType::ActivityStub {
            exec_info.ffqn.clone()
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
                            use grpc_client::replay_execution_response::Outcome;
                            cached_replay.set(Some(response.clone()));
                            match &response.outcome {
                                Some(Outcome::Advanceable(adv))
                                    if !adv.captured_writes.is_empty() =>
                                {
                                    modal_writes.set(Some(adv.captured_writes.clone()));
                                    is_blocked.set(false);
                                }
                                Some(Outcome::Finished(_)) => {
                                    notifications.push(Notification::info("Execution finished"));
                                    modal_writes.set(None);
                                    is_blocked.set(false);
                                }
                                Some(Outcome::Blocked(_)) => {
                                    // Still blocked, continue polling
                                }
                                Some(Outcome::ReplayFailed(f)) => {
                                    notifications.push(Notification::error(format!(
                                        "Replay failed: {}",
                                        f.error
                                    )));
                                    modal_writes.set(None);
                                    is_blocked.set(false);
                                }
                                _ => {
                                    // Empty advanceable or unknown — keep polling
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

    html! {
        <div class="execution-header">
            <div class="header-and-links">
                <h3>{ execution_id.render_execution_parts(false, *link) }</h3>

                <div class="execution-links">
                    { ExecutionLink::Trace.link(execution_id.clone(), "Trace") }
                    { ExecutionLink::ExecutionLog.link(execution_id.clone(), "Execution Log") }
                    { ExecutionLink::Debug.link(execution_id.clone(), "Debugger") }
                    { ExecutionLink::Logs.link(execution_id.clone(), "App Logs") }
                    <Link<Route, ExecutionQuery>
                        to={Route::ExecutionList}
                        query={ExecutionQuery { execution_id_prefix: Some(execution_id.to_string()), show_derived: true, ..Default::default() }}
                    >
                        {"Child executions"}
                    </Link<Route, ExecutionQuery>>
                </div>
            </div>

            <ExecutionStatus execution_id={execution_id.clone()} status={None} {finished_status} on_summary={on_summary} on_status_change={on_status_change} />

            if let Some(workflow_digest) = workflow_digest {
                <div class="execution-actions">
                    if !*is_finished {
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
                            ffqn={exec_info.as_ref().and_then(|info| info.ffqn.clone())}
                        />
                    }
                    <ReplayButton
                        execution_id={execution_id.clone()}
                        on_replay_response={
                            let cached_replay = cached_replay.clone();
                            Callback::from(move |resp: grpc_client::ReplayExecutionResponse| {
                                cached_replay.set(Some(resp));
                            })
                        }
                    />
                    if !*is_finished && *is_paused {
                        <AdvanceButton
                            execution_id={execution_id.clone()}
                            cached_replay_response={(*cached_replay).clone()}
                            on_open_modal={
                                let modal_writes = modal_writes.clone();
                                Callback::from(move |writes: Vec<CapturedWrite>| {
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
                        Callback::from(move |writes: Vec<CapturedWrite>| {
                            let execution_id = execution_id.clone();
                            let modal_writes = modal_writes.clone();
                            let cached_replay = cached_replay.clone();
                            let notifications = notifications.clone();
                            let is_blocked = is_blocked.clone();
                            let on_advanced = on_advanced.clone();
                            let advanced_version = last_version_of(&execution_id, &writes);
                            spawn_local(async move {
                                let mut client = ExecutionRepositoryClient::new(
                                    Client::new(BASE_URL.to_string()),
                                );
                                let result = client
                                    .advance_execution(grpc_client::AdvanceExecutionRequest {
                                        execution_id: Some(execution_id.clone()),
                                        captured_writes: writes,
                                    })
                                    .await;
                                match result {
                                    Ok(resp) => {
                                        use grpc_client::advance_execution_response;
                                        let inner = resp.into_inner();
                                        match inner.result {
                                            Some(advance_execution_response::Result::Success(
                                                _,
                                            )) => {
                                                debug!(
                                                    "Advance succeeded for {}",
                                                    execution_id
                                                );
                                                notifications.push(Notification::success(
                                                    "Advance succeeded",
                                                ));
                                                if let Some(version) = advanced_version
                                                    && let Some(cb) = &on_advanced
                                                {
                                                    cb.emit(version);
                                                }
                                                // Replay again to check next state
                                                if let Some(response) =
                                                    call_replay(&execution_id, &notifications).await
                                                {
                                                    use grpc_client::replay_execution_response::Outcome;
                                                    cached_replay.set(Some(response.clone()));
                                                    match &response.outcome {
                                                        Some(Outcome::Advanceable(adv))
                                                            if !adv.captured_writes.is_empty() =>
                                                        {
                                                            modal_writes.set(Some(
                                                                adv.captured_writes.clone(),
                                                            ));
                                                            return;
                                                        }
                                                        Some(Outcome::Blocked(_)) => {
                                                            is_blocked.set(true);
                                                            return;
                                                        }
                                                        _ => {
                                                            process_replay_response(
                                                                &response,
                                                                &notifications,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            Some(advance_execution_response::Result::Error(
                                                e,
                                            )) => {
                                                let msg = match e.error {
                                                    Some(
                                                        advance_execution_response::error::Error::NoWrites(
                                                            _,
                                                        ),
                                                    ) => "No writes to advance".to_string(),
                                                    Some(
                                                        advance_execution_response::error::Error::ReplayError(
                                                            re,
                                                        ),
                                                    ) => {
                                                        format!("Replay error: {}", re.message)
                                                    }
                                                    Some(
                                                        advance_execution_response::error::Error::VersionMismatch(
                                                            vm,
                                                        ),
                                                    ) => format!(
                                                        "Version mismatch (expected {})",
                                                        vm.expected
                                                    ),
                                                    Some(
                                                        advance_execution_response::error::Error::ReplayMismatch(
                                                            _,
                                                        ),
                                                    ) => "Replay mismatch".to_string(),
                                                    None => "Unknown advance error".to_string(),
                                                };
                                                notifications
                                                    .push(Notification::error(msg));
                                            }
                                            None => {
                                                notifications.push(Notification::error(
                                                    "Empty advance response",
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(
                                            "Advance RPC failed for {}: {:?}",
                                            execution_id, e
                                        );
                                        notifications.push(Notification::error(
                                            e.message().to_string(),
                                        ));
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
                                let mut client = ExecutionRepositoryClient::new(
                                    Client::new(BASE_URL.to_string()),
                                );
                                match client
                                    .unpause_execution(grpc_client::UnpauseExecutionRequest {
                                        execution_id: Some(execution_id.clone()),
                                    })
                                    .await
                                {
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
                                        notifications
                                            .push(Notification::error(e.message().to_string()));
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

            if is_activity && !*is_finished {
                <div class="execution-actions">
                    <PauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                    <UnpauseButton
                        execution_id={execution_id.clone()}
                        is_paused={*is_paused}
                    />
                    <CancelActivityButton
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
        match self {
            ExecutionLink::Trace => html! {
                <Link<Route> to={Route::ExecutionTrace { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::ExecutionLog => html! {
                <Link<Route> to={Route::ExecutionLog { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Debug => html! {
                <Link<Route> to={Route::ExecutionDebugger { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Logs => html! {
                <Link<Route> to={Route::Logs { execution_id }}>
                    {title}
                </Link<Route>>
            },
        }
    }
}
