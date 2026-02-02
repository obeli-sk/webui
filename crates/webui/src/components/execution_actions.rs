//! Execution action components: Replay and Upgrade functionality

use crate::{
    BASE_URL,
    app::AppState,
    grpc::grpc_client::{
        self, ComponentType, ContentDigest, ExecutionId,
        execution_repository_client::ExecutionRepositoryClient,
    },
};
use log::{debug, error};
use std::ops::Deref;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

/// Validates a digest string has the correct format (sha256:hex)
fn validate_digest(input: &str) -> Result<(), String> {
    let hash_hex = input
        .strip_prefix("sha256:")
        .ok_or_else(|| "Digest must start with 'sha256:'".to_string())?;

    if hash_hex.len() != 64 {
        return Err(format!(
            "Expected 64 hex characters after 'sha256:', got {}",
            hash_hex.len()
        ));
    }

    if !hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Digest must contain only hexadecimal characters".to_string());
    }

    Ok(())
}

#[derive(Clone, PartialEq)]
pub enum ActionResult {
    None,
    Success(String),
    Error(String),
}

// ============================================================================
// Replay Execution Button
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct ReplayButtonProps {
    pub execution_id: ExecutionId,
    /// Only workflows can be replayed
    pub is_workflow: bool,
}

#[function_component(ReplayButton)]
pub fn replay_button(props: &ReplayButtonProps) -> Html {
    let result_state = use_state(|| ActionResult::None);
    let loading_state = use_state(|| false);

    let onclick = {
        let execution_id = props.execution_id.clone();
        let result_state = result_state.clone();
        let loading_state = loading_state.clone();

        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let result_state = result_state.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);
            result_state.set(ActionResult::None);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));

                let result = client
                    .replay_execution(grpc_client::ReplayExecutionRequest {
                        execution_id: Some(execution_id.clone()),
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(_) => {
                        debug!("Replay requested for execution {}", execution_id);
                        result_state.set(ActionResult::Success(
                            "Replay requested successfully".to_string(),
                        ));
                    }
                    Err(e) => {
                        error!("Failed to replay execution {}: {:?}", execution_id, e);
                        result_state.set(ActionResult::Error(e.message().to_string()));
                    }
                }
            });
        })
    };

    if !props.is_workflow {
        return html! {};
    }

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
            { render_result(result_state.deref()) }
        </div>
    }
}

// ============================================================================
// Upgrade Execution Component Form
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct UpgradeFormProps {
    pub execution_id: ExecutionId,
    pub current_digest: Option<ContentDigest>,
    /// Only workflows can be upgraded
    pub is_workflow: bool,
}

#[function_component(UpgradeForm)]
pub fn upgrade_form(props: &UpgradeFormProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");

    let result_state = use_state(|| ActionResult::None);
    let loading_state = use_state(|| false);
    let new_digest_state = use_state(String::new);
    let skip_determinism_state = use_state(|| false);
    let validation_error_state = use_state(|| None::<String>);
    let show_form_state = use_state(|| false);

    let new_digest_ref = use_node_ref();

    // Get workflow components for the dropdown
    let workflow_components: Vec<_> = app_state
        .components_by_id
        .iter()
        .filter(|(id, _)| id.component_type() == ComponentType::Workflow)
        .map(|(id, _)| id.clone())
        .collect();

    let on_toggle_form = {
        let show_form_state = show_form_state.clone();
        Callback::from(move |_| {
            show_form_state.set(!*show_form_state);
        })
    };

    let on_digest_change = {
        let new_digest_state = new_digest_state.clone();
        let validation_error_state = validation_error_state.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let value = input.value();
            new_digest_state.set(value.clone());

            if value.is_empty() {
                validation_error_state.set(None);
            } else {
                match validate_digest(&value) {
                    Ok(()) => validation_error_state.set(None),
                    Err(msg) => validation_error_state.set(Some(msg)),
                }
            }
        })
    };

    let on_select_component = {
        let new_digest_state = new_digest_state.clone();
        let validation_error_state = validation_error_state.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let value = select.value();
            if !value.is_empty() {
                new_digest_state.set(value.clone());
                validation_error_state.set(None);
            } else {
                new_digest_state.set(String::new());
            }
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
        let current_digest = props.current_digest.clone();
        let new_digest_state = new_digest_state.clone();
        let skip_determinism_state = skip_determinism_state.clone();
        let result_state = result_state.clone();
        let loading_state = loading_state.clone();
        let validation_error_state = validation_error_state.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let new_digest = (*new_digest_state).clone();

            // Validate
            if new_digest.is_empty() {
                validation_error_state.set(Some("New digest is required".to_string()));
                return;
            }

            if let Err(msg) = validate_digest(&new_digest) {
                validation_error_state.set(Some(msg));
                return;
            }

            let Some(expected_digest) = current_digest.clone() else {
                result_state.set(ActionResult::Error(
                    "Current component digest not available".to_string(),
                ));
                return;
            };

            let execution_id = execution_id.clone();
            let skip_determinism = *skip_determinism_state;
            let result_state = result_state.clone();
            let loading_state = loading_state.clone();

            loading_state.set(true);
            result_state.set(ActionResult::None);

            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));

                let result = client
                    .upgrade_execution_component(grpc_client::UpgradeExecutionComponentRequest {
                        execution_id: Some(execution_id.clone()),
                        expected_component_digest: Some(expected_digest),
                        new_component_digest: Some(ContentDigest {
                            digest: new_digest.clone(),
                        }),
                        skip_determinism_check: skip_determinism,
                    })
                    .await;

                loading_state.set(false);

                match result {
                    Ok(_) => {
                        debug!(
                            "Upgrade requested for execution {} to {}",
                            execution_id, new_digest
                        );
                        result_state.set(ActionResult::Success(format!(
                            "Upgraded to {}",
                            &new_digest[..20.min(new_digest.len())]
                        )));
                    }
                    Err(e) => {
                        error!("Failed to upgrade execution {}: {:?}", execution_id, e);
                        result_state.set(ActionResult::Error(e.message().to_string()));
                    }
                }
            });
        })
    };

    if !props.is_workflow {
        return html! {};
    }

    let is_loading = *loading_state;
    let show_form = *show_form_state;
    let validation_error = (*validation_error_state).clone();

    html! {
        <div class="action-container upgrade-action">
            <button
                class="action-button toggle-upgrade-button"
                onclick={on_toggle_form}
            >
                if show_form {
                    {"Hide Upgrade Form"}
                } else {
                    {"Upgrade Component"}
                }
            </button>

            if show_form {
                <form class="upgrade-form" onsubmit={on_submit}>
                    <div class="form-row">
                        <label>{"Current digest:"}</label>
                        <span class="current-digest">
                            if let Some(digest) = &props.current_digest {
                                { &digest.digest }
                            } else {
                                {"Loading..."}
                            }
                        </span>
                    </div>

                    <div class="form-row">
                        <label for="new-digest">{"New digest:"}</label>
                        <input
                            ref={new_digest_ref}
                            id="new-digest"
                            type="text"
                            placeholder="sha256:..."
                            value={(*new_digest_state).clone()}
                            onchange={on_digest_change}
                        />
                    </div>

                    if !workflow_components.is_empty() {
                        <div class="form-row">
                            <label for="select-component">{"Or select:"}</label>
                            <select id="select-component" onchange={on_select_component}>
                                <option value="" selected=true>{"-- Select a workflow component --"}</option>
                                {
                                    workflow_components.iter().map(|comp_id| {
                                        let digest = comp_id.digest.as_ref()
                                            .map(|d| d.digest.clone())
                                            .unwrap_or_default();
                                        let name = &comp_id.name;
                                        let is_current = props.current_digest.as_ref()
                                            .is_some_and(|d| d.digest == digest);
                                        html! {
                                            <option
                                                value={digest.clone()}
                                                disabled={is_current}
                                            >
                                                { format!("{} ({}...)", name, &digest[..20.min(digest.len())]) }
                                                if is_current {
                                                    {" [current]"}
                                                }
                                            </option>
                                        }
                                    }).collect::<Html>()
                                }
                            </select>
                        </div>
                    }

                    if let Some(error) = validation_error {
                        <div class="validation-error">{ error }</div>
                    }

                    <div class="form-row checkbox-row">
                        <label>
                            <input
                                type="checkbox"
                                checked={*skip_determinism_state}
                                onchange={on_skip_determinism_change}
                            />
                            {" Skip determinism check"}
                        </label>
                    </div>

                    <div class="form-row">
                        <button
                            type="submit"
                            class="action-button submit-upgrade-button"
                            disabled={is_loading || validation_error_state.is_some()}
                        >
                            if is_loading {
                                {"Upgrading..."}
                            } else {
                                {"Upgrade"}
                            }
                        </button>
                    </div>

                    { render_result(result_state.deref()) }
                </form>
            }
        </div>
    }
}

fn render_result(result: &ActionResult) -> Html {
    match result {
        ActionResult::None => html! {},
        ActionResult::Success(msg) => {
            html! { <div class="action-result success">{ msg }</div> }
        }
        ActionResult::Error(msg) => {
            html! { <div class="action-result error">{ msg }</div> }
        }
    }
}
