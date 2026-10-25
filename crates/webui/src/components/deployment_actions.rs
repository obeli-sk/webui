use crate::{
    components::notification::{Notification, NotificationContext},
    rest,
};
use concepts::prefixed_ulid::DeploymentId;
use gloo::timers::callback::Timeout;
use log::error;
use obelisk_api_types::{
    DeploymentSwitchOutcome, DeploymentSwitchPayload, DeploymentSwitchResponse,
};
use rest_types::deployment::DeploymentStatusSer;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentActionsProps {
    pub deployment_id: DeploymentId,
    pub status: DeploymentStatusSer,
    /// Called after a successful `SwitchDeployment` so the parent can refresh.
    #[prop_or_default]
    pub on_switched: Callback<()>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArmedAction {
    Apply,
    Enqueue,
}

/// Buttons to apply a deployment immediately or enqueue it for the next server restart.
#[component(DeploymentActions)]
pub fn deployment_actions(
    DeploymentActionsProps {
        deployment_id,
        status,
        on_switched,
    }: &DeploymentActionsProps,
) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let in_flight = use_state(|| false);
    let armed = use_state(|| None::<ArmedAction>);
    // Dropping the previous timeout cancels it when another button is armed.
    let disarm_timer = use_mut_ref(|| None::<Timeout>);

    if *status == DeploymentStatusSer::Active {
        return html! {};
    }

    let arm = {
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        move |action: ArmedAction| {
            let armed = armed.clone();
            let disarm_timer = disarm_timer.clone();
            Callback::from(move |_| {
                armed.set(Some(action));
                let armed = armed.clone();
                *disarm_timer.borrow_mut() = Some(Timeout::new(5000, move || armed.set(None)));
            })
        }
    };

    let switch = {
        let deployment_id = *deployment_id;
        let notifications = notifications.clone();
        let on_switched = on_switched.clone();
        let in_flight = in_flight.clone();
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        // `allow_unavailable` only matters for enqueueing; apply is always strict.
        move |apply: bool, allow_unavailable: bool| {
            let notifications = notifications.clone();
            let on_switched = on_switched.clone();
            let in_flight = in_flight.clone();
            let armed = armed.clone();
            let disarm_timer = disarm_timer.clone();
            Callback::from(move |_| {
                armed.set(None);
                *disarm_timer.borrow_mut() = None;
                let notifications = notifications.clone();
                let on_switched = on_switched.clone();
                let in_flight = in_flight.clone();
                in_flight.set(true);
                spawn_local(async move {
                    let response = rest::put::<_, DeploymentSwitchResponse>(
                        &format!("/v1/deployments/{deployment_id}/switch"),
                        &DeploymentSwitchPayload {
                            allow_unavailable_runtime_config: allow_unavailable,
                            apply,
                        },
                    )
                    .await;
                    in_flight.set(false);
                    match response {
                        Ok(DeploymentSwitchResponse { ok }) => {
                            match ok {
                                DeploymentSwitchOutcome::Switched => {
                                    notifications.push(Notification::success(
                                        "Apply succeeded, the deployment is now live",
                                    ))
                                }
                                DeploymentSwitchOutcome::RestartRequired => {
                                    notifications.push(Notification::info(
                                        "Deployment enqueued, restart the server to apply it",
                                    ))
                                }
                            }
                            on_switched.emit(());
                        }
                        Err(e) => {
                            error!("Failed to switch deployment: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to switch deployment: {}",
                                e
                            )));
                        }
                    }
                });
            })
        }
    };

    let enqueue_disabled = *in_flight || *status == DeploymentStatusSer::Enqueued;
    let apply_armed = *armed == Some(ArmedAction::Apply);
    let enqueue_armed = *armed == Some(ArmedAction::Enqueue);
    html! {
        <div class="deployment-actions">
            <button
                class={classes!(
                    "action-button",
                    apply_armed.then_some("confirm"),
                )}
                onclick={
                    if apply_armed {
                        switch(true, false)
                    } else {
                        arm(ArmedAction::Apply)
                    }
                }
                disabled={*in_flight}
            >
                { if apply_armed { "Confirm apply" } else { "Apply" } }
            </button>
            if enqueue_armed {
                <button
                    class="action-button confirm"
                    onclick={switch(false, false)}
                    disabled={enqueue_disabled}
                    title="Enqueues the deployment for the next server restart, failing if any runtime requirement is unavailable"
                >
                    {"Confirm enqueue"}
                </button>
                <button
                    class="action-button warning"
                    onclick={switch(false, true)}
                    disabled={enqueue_disabled}
                    title="Enqueues the deployment even if environment variables, secrets, or server capabilities are unavailable; activation may still fail at the next server restart"
                >
                    {"Allow unavailable requirements"}
                </button>
            } else {
                <button
                    class="action-button"
                    onclick={arm(ArmedAction::Enqueue)}
                    disabled={enqueue_disabled}
                    title="Enqueues the deployment for the next server restart"
                >
                    {"Enqueue for next restart"}
                </button>
            }
        </div>
    }
}
