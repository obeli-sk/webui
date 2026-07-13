//! Execution action components: Replay, Pause, Unpause, and Upgrade functionality

use crate::{
    app::{AppState, Route},
    components::notification::{Notification, NotificationContext},
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{
            self, ContentDigest, ExecutionId,
            execution_repository_client::ExecutionRepositoryClient,
        },
    },
};
use log::{debug, error};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::Link;

// ============================================================================
// Replay Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct ReplayButtonProps {
    pub execution_id: ExecutionId,
    /// Called with the captured writes when replay returns Advanceable with non-empty writes.
    #[prop_or_default]
    pub on_replay_response: Option<Callback<grpc_client::ReplayExecutionResponse>>,
}

/// Calls ReplayExecution and returns the response, or None on RPC error.
pub async fn call_replay(
    execution_id: &ExecutionId,
    notifications: &NotificationContext,
) -> Option<grpc_client::ReplayExecutionResponse> {
    let mut client = ExecutionRepositoryClient::new(crate::auth::client());
    match client
        .replay_execution(grpc_client::ReplayExecutionRequest {
            execution_id: Some(execution_id.clone()),
        })
        .await
    {
        Ok(resp) => Some(resp.into_inner()),
        Err(e) => {
            error!("Failed to replay execution {}: {:?}", execution_id, e);
            notifications.push(Notification::error(e.message().to_string()));
            None
        }
    }
}

/// Processes a ReplayExecutionResponse: shows notifications for non-advanceable outcomes,
/// returns the captured_writes for Advanceable, or None.
pub fn process_replay_response(
    response: &grpc_client::ReplayExecutionResponse,
    notifications: &NotificationContext,
) -> Option<Vec<grpc_client::CapturedWrite>> {
    use grpc_client::replay_execution_response::Outcome;
    match &response.outcome {
        Some(Outcome::Advanceable(adv)) => {
            if adv.captured_writes.is_empty() {
                notifications.push(Notification::info("Replay OK, no pending writes"));
                None
            } else {
                Some(adv.captured_writes.clone())
            }
        }
        Some(Outcome::Finished(_)) => {
            notifications.push(Notification::info("Replay OK, finished execution"));
            None
        }
        Some(Outcome::Blocked(_)) => {
            notifications.push(Notification::info("Replay OK, execution is blocked"));
            None
        }
        Some(Outcome::ReplayFailed(failed)) => {
            notifications.push(Notification::error(format!(
                "Replay failed: {}",
                failed.error
            )));
            None
        }
        None => {
            notifications.push(Notification::error("Empty replay response"));
            None
        }
    }
}

#[component(ReplayButton)]
pub fn replay_button(props: &ReplayButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();
        let on_replay_response = props.on_replay_response.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();
            let on_replay_response = on_replay_response.clone();

            loading_state.set(true);

            spawn_local(async move {
                if let Some(response) = call_replay(&execution_id, &notifications).await {
                    if let Some(cb) = &on_replay_response {
                        cb.emit(response.clone());
                    }
                    let writes = process_replay_response(&response, &notifications);
                    if writes.is_some() {
                        notifications
                            .push(Notification::success("Replay: advanceable writes ready"));
                    }
                }
                loading_state.set(false);
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container replay-action">
            <button
                class="action-button replay-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Replaying..."}
                } else {
                    {"Replay"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Advance Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct AdvanceButtonProps {
    pub execution_id: ExecutionId,
    /// Cached replay response from a previous Replay click.
    pub cached_replay_response: Option<grpc_client::ReplayExecutionResponse>,
    /// Called to open the advance modal with captured writes.
    pub on_open_modal: Callback<Vec<grpc_client::CapturedWrite>>,
}

#[component(AdvanceButton)]
pub fn advance_button(props: &AdvanceButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let cached = props.cached_replay_response.clone();
        let on_open_modal = props.on_open_modal.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let cached = cached.clone();
            let on_open_modal = on_open_modal.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            // Try cached response first
            if let Some(ref response) = cached
                && let Some(writes) = process_replay_response(response, &notifications)
            {
                on_open_modal.emit(writes);
                return;
            }

            // No cache or cache was not advanceable — call replay
            loading_state.set(true);
            spawn_local(async move {
                if let Some(response) = call_replay(&execution_id, &notifications).await
                    && let Some(writes) = process_replay_response(&response, &notifications)
                {
                    on_open_modal.emit(writes);
                }
                loading_state.set(false);
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container advance-action">
            <button
                class="action-button advance-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Loading..."}
                } else {
                    {"Advance"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Upgrade Execution Component Form
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct UpgradeFormProps {
    pub execution_id: ExecutionId,
    pub current_digest: ContentDigest,
    pub ffqn: Option<FunctionFqn>,
}

/// Find the digest of the deployed component that exports the given FFQN.
/// Returns `None` if the FFQN is not found in the current deployment.
fn find_upgrade_digest(
    app_state: &AppState,
    ffqn: &FunctionFqn,
    current_digest: &ContentDigest,
) -> Option<ContentDigest> {
    let (_, component_id) = app_state.ffqns_to_details.get(ffqn)?;
    let digest = component_id.digest.as_ref()?;
    if digest.digest != current_digest.digest {
        Some(digest.clone())
    } else {
        None
    }
}

#[component(UpgradeForm)]
pub fn upgrade_form(props: &UpgradeFormProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    let loading_state = use_state(|| false);
    let skip_determinism_state = use_state(|| false);
    let show_modal_state = use_state(|| false);
    // Tracks the digest after a successful upgrade, so we can disable the button
    // immediately without waiting for the parent to propagate the new digest.
    let upgraded_digest = use_state(|| None::<ContentDigest>);

    // Use the upgraded digest if available, otherwise fall back to the prop.
    let effective_digest = upgraded_digest
        .as_ref()
        .cloned()
        .unwrap_or_else(|| props.current_digest.clone());

    // Determine upgrade target from the FFQN
    let upgrade_digest = props
        .ffqn
        .as_ref()
        .and_then(|ffqn| find_upgrade_digest(&app_state, ffqn, &effective_digest));

    let (button_disabled, button_title) = if props.ffqn.is_none() {
        (true, "No function name available for this execution")
    } else if upgrade_digest.is_some() {
        (
            false,
            "Upgrade to the component version from the current deployment",
        )
    } else if app_state
        .ffqns_to_details
        .contains_key(props.ffqn.as_ref().unwrap())
    {
        (
            true,
            "Execution already uses the component from the current deployment",
        )
    } else {
        (
            true,
            "No component exporting this function found in the current deployment",
        )
    };

    let on_open_modal = {
        let show_modal_state = show_modal_state.clone();
        Callback::from(move |_| {
            show_modal_state.set(true);
        })
    };

    let on_close_modal = {
        let show_modal_state = show_modal_state.clone();
        Callback::from(move |_| {
            show_modal_state.set(false);
        })
    };

    let on_close_modal_click = {
        let on_close_modal = on_close_modal.clone();
        Callback::from(move |_: MouseEvent| {
            on_close_modal.emit(());
        })
    };

    let on_skip_determinism_change = {
        let skip_determinism_state = skip_determinism_state.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            skip_determinism_state.set(input.checked());
        })
    };

    let on_submit = {
        let execution_id = props.execution_id.clone();
        let effective_digest = effective_digest.clone();
        let upgrade_digest = upgrade_digest.clone();
        let skip_determinism_state = skip_determinism_state.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();
        let show_modal_state = show_modal_state.clone();
        let upgraded_digest = upgraded_digest.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let new_digest = match &upgrade_digest {
                Some(d) => d.clone(),
                None => return,
            };

            loading_state.set(true);

            spawn_local({
                let execution_id = execution_id.clone();
                let skip_determinism = *skip_determinism_state;
                let notifications = notifications.clone();
                let loading_state = loading_state.clone();
                let effective_digest = effective_digest.clone();
                let show_modal_state = show_modal_state.clone();
                let upgraded_digest = upgraded_digest.clone();

                async move {
                    let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                    let result = client
                        .upgrade_execution_component(
                            grpc_client::UpgradeExecutionComponentRequest {
                                execution_id: Some(execution_id.clone()),
                                expected_component_digest: Some(effective_digest),
                                new_component_digest: Some(new_digest.clone()),
                                skip_determinism_check: skip_determinism,
                            },
                        )
                        .await;

                    loading_state.set(false);

                    match result {
                        Ok(_) => {
                            let digest_str = &new_digest.digest;
                            debug!(
                                "Upgrade requested for execution {} to {}",
                                execution_id, digest_str
                            );
                            notifications.push(Notification::success(format!(
                                "Upgraded to {}",
                                &digest_str[..20.min(digest_str.len())]
                            )));
                            upgraded_digest.set(Some(new_digest));
                            show_modal_state.set(false);
                        }
                        Err(e) => {
                            error!("Failed to upgrade execution {}: {:?}", execution_id, e);
                            notifications.push(Notification::error(e.message().to_string()));
                        }
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;
    let show_modal = *show_modal_state;

    {
        let on_close_modal = on_close_modal.clone();
        use_effect_with(show_modal, move |is_open| {
            let listener = if *is_open {
                let closure = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(
                    move |e: web_sys::KeyboardEvent| {
                        if e.key() == "Escape" {
                            on_close_modal.emit(());
                        }
                    },
                );
                let window = web_sys::window().expect("window should exist");
                window
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                    .expect("failed to add keydown listener");
                Some((window, closure))
            } else {
                None
            };

            move || {
                if let Some((window, closure)) = listener {
                    window
                        .remove_event_listener_with_callback(
                            "keydown",
                            closure.as_ref().unchecked_ref(),
                        )
                        .expect("failed to remove keydown listener");
                }
            }
        });
    }

    let on_overlay_click = {
        let on_close_modal = on_close_modal.clone();
        Callback::from(move |e: MouseEvent| {
            if let Some(target) = e.target_dyn_into::<web_sys::Element>()
                && target
                    .get_attribute("class")
                    .unwrap_or_default()
                    .contains("modal-overlay")
            {
                on_close_modal.emit(());
            }
        })
    };

    let target_digest = upgrade_digest.as_ref().map(|digest| digest.digest.clone());
    let current_digest = effective_digest.digest.clone();

    html! {
        <div class="action-container upgrade-action">
            <button
                class="action-button toggle-upgrade-button"
                onclick={on_open_modal}
                disabled={button_disabled}
                title={button_title}
            >
                {"Upgrade Component"}
            </button>

            if show_modal {
                <div class="modal-overlay" onclick={on_overlay_click}>
                    <div class="modal-window upgrade-modal-window">
                        <form class="upgrade-modal-form" onsubmit={on_submit}>
                            <div class="modal-header">
                                <h3>{"Upgrade Component"}</h3>
                                <button
                                    type="button"
                                    class="modal-dismiss"
                                    onclick={on_close_modal_click.clone()}
                                >
                                    {"×"}
                                </button>
                            </div>

                            <div class="upgrade-modal-body">
                                <div class="upgrade-modal-copy">
                                    {"Switch this execution to the component version from the current deployment."}
                                </div>
                                <div class="upgrade-modal-digests">
                                    <div class="upgrade-modal-digest-row">
                                        <span class="upgrade-modal-digest-label">{"Current"}</span>
                                        <code class="upgrade-modal-digest-value">{current_digest}</code>
                                    </div>
                                    if let Some(target_digest) = target_digest {
                                        <div class="upgrade-modal-digest-row">
                                            <span class="upgrade-modal-digest-label">{"Target"}</span>
                                            <code class="upgrade-modal-digest-value">{target_digest}</code>
                                        </div>
                                    }
                                </div>
                            </div>

                            <div class="modal-footer">
                                <div class="modal-footer-left">
                                    <label class="modal-checkbox upgrade-modal-checkbox">
                                        <input
                                            type="checkbox"
                                            checked={*skip_determinism_state}
                                            onchange={on_skip_determinism_change}
                                        />
                                        {"Skip determinism check"}
                                    </label>
                                </div>
                                <button
                                    type="submit"
                                    class="action-button submit-upgrade-button"
                                    disabled={is_loading}
                                >
                                    if is_loading {
                                        {"Upgrading..."}
                                    } else {
                                        {"Upgrade"}
                                    }
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            }
        </div>
    }
}

// ============================================================================
// Cancel Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct CancelExecutionButtonProps {
    pub execution_id: ExecutionId,
}

#[component(CancelExecutionButton)]
pub fn cancel_execution_button(props: &CancelExecutionButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .cancel_execution(grpc_client::CancelExecutionRequest {
                        execution_id: Some(execution_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(response) => {
                        let outcome = response.into_inner().outcome();
                        debug!(
                            "Cancel requested for execution {}: {:?}",
                            execution_id, outcome
                        );
                        let message = match outcome {
                            grpc_client::cancel_execution_response::CancelExecutionOutcome::CancellationRequested => {
                                "Cancellation requested"
                            }
                            grpc_client::cancel_execution_response::CancelExecutionOutcome::AlreadyFinished => {
                                "Execution already finished"
                            }
                            grpc_client::cancel_execution_response::CancelExecutionOutcome::AlreadyCancelling => {
                                "Execution already cancelling"
                            }
                            grpc_client::cancel_execution_response::CancelExecutionOutcome::Unspecified => {
                                "Unknown cancel outcome"
                            }
                        };
                        notifications.push(Notification::success(message));
                    }
                    Err(e) => {
                        error!("Failed to cancel execution {}: {:?}", execution_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container cancel-action">
            <button
                class="action-button cancel-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Cancelling..."}
                } else {
                    {"Cancel Execution"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Pause Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct PauseButtonProps {
    pub execution_id: ExecutionId,
    /// Whether the execution is currently paused
    pub is_paused: bool,
}

#[component(PauseButton)]
pub fn pause_button(props: &PauseButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .pause_execution(grpc_client::PauseExecutionRequest {
                        execution_id: Some(execution_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(_response) => {
                        debug!("Pause requested for execution {execution_id}");
                        notifications.push(Notification::success("Execution paused successfully"));
                    }
                    Err(e) => {
                        error!("Failed to pause execution {}: {:?}", execution_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;
    let is_disabled = is_loading || props.is_paused;

    html! {
        <div class="action-container pause-action">
            <button
                class="action-button pause-button"
                onclick={onclick}
                disabled={is_disabled}
            >
                if is_loading {
                    {"Pausing..."}
                } else {
                    {"Pause"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Unpause Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct UnpauseButtonProps {
    pub execution_id: ExecutionId,
    /// Whether the execution is currently paused
    pub is_paused: bool,
}

#[component(UnpauseButton)]
pub fn unpause_button(props: &UnpauseButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .unpause_execution(grpc_client::UnpauseExecutionRequest {
                        execution_id: Some(execution_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(_response) => {
                        debug!("Unpause requested for execution {execution_id}",);
                        notifications
                            .push(Notification::success("Execution unpaused successfully"));
                    }
                    Err(e) => {
                        error!("Failed to unpause execution {}: {:?}", execution_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;
    let is_disabled = is_loading || !props.is_paused;

    html! {
        <div class="action-container unpause-action">
            <button
                class="action-button unpause-button"
                onclick={onclick}
                disabled={is_disabled}
            >
                if is_loading {
                    {"Unpausing..."}
                } else {
                    {"Unpause"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Cancel Delay Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct CancelDelayButtonProps {
    pub delay_id: grpc_client::DelayId,
}

#[component(CancelDelayButton)]
pub fn cancel_delay_button(props: &CancelDelayButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let delay_id = props.delay_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let delay_id = delay_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .cancel_delay(grpc_client::CancelDelayRequest {
                        delay_id: Some(delay_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(response) => {
                        let outcome = response.into_inner().outcome();
                        debug!("Cancel requested for delay {}: {:?}", delay_id, outcome);
                        let message = match outcome {
                            grpc_client::cancel_delay_response::CancelDelayOutcome::Cancelled => {
                                "Delay cancelled successfully"
                            }
                            grpc_client::cancel_delay_response::CancelDelayOutcome::AlreadyFinished => {
                                "Delay already finished"
                            }
                            grpc_client::cancel_delay_response::CancelDelayOutcome::Unspecified => {
                                "Unknown cancel outcome"
                            }
                        };
                        notifications.push(Notification::success(message));
                    }
                    Err(e) => {
                        error!("Failed to cancel delay {}: {:?}", delay_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container cancel-delay-action">
            <button
                class="action-button cancel-delay-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Cancelling..."}
                } else {
                    {"Cancel Delay"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Pause Delay Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct PauseDelayButtonProps {
    pub delay_id: grpc_client::DelayId,
}

#[component(PauseDelayButton)]
pub fn pause_delay_button(props: &PauseDelayButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let delay_id = props.delay_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let delay_id = delay_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .pause_delay(grpc_client::PauseDelayRequest {
                        delay_id: Some(delay_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(response) => {
                        let outcome = response.into_inner().outcome();
                        debug!("Pause requested for delay {}: {:?}", delay_id, outcome);
                        let message = match outcome {
                            grpc_client::pause_delay_response::PauseDelayOutcome::Paused => {
                                "Delay paused successfully"
                            }
                            grpc_client::pause_delay_response::PauseDelayOutcome::AlreadyFinished => {
                                "Delay already finished"
                            }
                            grpc_client::pause_delay_response::PauseDelayOutcome::Unspecified => {
                                "Unknown pause outcome"
                            }
                        };
                        notifications.push(Notification::success(message));
                    }
                    Err(e) => {
                        error!("Failed to pause delay {}: {:?}", delay_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container pause-delay-action">
            <button
                class="action-button pause-delay-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Pausing..."}
                } else {
                    {"Pause Delay"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Unpause Delay Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct UnpauseDelayButtonProps {
    pub delay_id: grpc_client::DelayId,
}

#[component(UnpauseDelayButton)]
pub fn unpause_delay_button(props: &UnpauseDelayButtonProps) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let loading_state = use_state(|| false);

    let onclick = {
        let delay_id = props.delay_id.clone();
        let notifications = notifications.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let delay_id = delay_id.clone();
            let notifications = notifications.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(crate::auth::client());

                let result = client
                    .unpause_delay(grpc_client::UnpauseDelayRequest {
                        delay_id: Some(delay_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(response) => {
                        let outcome = response.into_inner().outcome();
                        debug!("Unpause requested for delay {}: {:?}", delay_id, outcome);
                        let message = match outcome {
                            grpc_client::unpause_delay_response::UnpauseDelayOutcome::Unpaused => {
                                "Delay unpaused successfully"
                            }
                            grpc_client::unpause_delay_response::UnpauseDelayOutcome::AlreadyFinished => {
                                "Delay already finished"
                            }
                            grpc_client::unpause_delay_response::UnpauseDelayOutcome::Unspecified => {
                                "Unknown unpause outcome"
                            }
                        };
                        notifications.push(Notification::success(message));
                    }
                    Err(e) => {
                        error!("Failed to unpause delay {}: {:?}", delay_id, e);
                        notifications.push(Notification::error(e.message().to_string()));
                    }
                }
            });
        })
    };

    let is_loading = *loading_state;

    html! {
        <div class="action-container unpause-delay-action">
            <button
                class="action-button unpause-delay-button"
                onclick={onclick}
                disabled={is_loading}
            >
                if is_loading {
                    {"Unpausing..."}
                } else {
                    {"Unpause Delay"}
                }
            </button>
        </div>
    }
}

// ============================================================================
// Submit Stub Response Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct SubmitStubButtonProps {
    pub execution_id: ExecutionId,
    pub ffqn: FunctionFqn,
}

/// Renders a "Submit stub response" button for ActivityStub executions.
/// Should only be rendered when the execution is an unfinished ActivityStub.
#[component(SubmitStubButton)]
pub fn submit_stub_button(props: &SubmitStubButtonProps) -> Html {
    html! {
        <div class="action-container submit-stub-action">
            <Link<Route>
                to={Route::ExecutionStubResult { ffqn: props.ffqn.clone(), execution_id: props.execution_id.clone() }}
                classes="action-button submit-stub-button"
            >
                {"Submit Stub Response"}
            </Link<Route>>
        </div>
    }
}
