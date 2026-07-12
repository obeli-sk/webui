use crate::{
    BASE_URL,
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{
        self, DeploymentId, DeploymentStatus, RuntimeConfigCheck,
        deployment_repository_client::DeploymentRepositoryClient,
        switch_deployment_response::Outcome,
    },
};
use gloo::timers::callback::Timeout;
use log::error;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentActionsProps {
    pub deployment_id: DeploymentId,
    pub status: DeploymentStatus,
    /// Called after a successful `SwitchDeployment` so the parent can refresh.
    #[prop_or_default]
    pub on_switched: Callback<()>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArmedAction {
    HotRedeploy,
    Enqueue,
}

/// Buttons to hot redeploy a deployment or enqueue it for the next server restart.
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

    if *status == DeploymentStatus::Active {
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
        let deployment_id = deployment_id.clone();
        let notifications = notifications.clone();
        let on_switched = on_switched.clone();
        let in_flight = in_flight.clone();
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        // `allow_unavailable` only matters for enqueueing; hot redeploy is always strict.
        move |hot_redeploy: bool, allow_unavailable: bool| {
            let deployment_id = deployment_id.clone();
            let notifications = notifications.clone();
            let on_switched = on_switched.clone();
            let in_flight = in_flight.clone();
            let armed = armed.clone();
            let disarm_timer = disarm_timer.clone();
            Callback::from(move |_| {
                armed.set(None);
                *disarm_timer.borrow_mut() = None;
                let deployment_id = deployment_id.clone();
                let notifications = notifications.clone();
                let on_switched = on_switched.clone();
                let in_flight = in_flight.clone();
                in_flight.set(true);
                spawn_local(async move {
                    let mut client =
                        DeploymentRepositoryClient::new(Client::new(BASE_URL.to_string()));
                    let response = client
                        .switch_deployment(grpc_client::SwitchDeploymentRequest {
                            deployment_id: Some(deployment_id),
                            runtime_config_check: if allow_unavailable {
                                RuntimeConfigCheck::AllowUnavailable as i32
                            } else {
                                RuntimeConfigCheck::Strict as i32
                            },
                            hot_redeploy,
                        })
                        .await;
                    in_flight.set(false);
                    match response {
                        Ok(resp) => {
                            match resp.into_inner().outcome() {
                                Outcome::SwitchOutcomeSwitched => {
                                    notifications.push(Notification::success(
                                        "Hot redeploy succeeded, the deployment is now live",
                                    ))
                                }
                                Outcome::SwitchOutcomeRestartRequired => {
                                    notifications.push(Notification::info(
                                        "Deployment enqueued, restart the server to apply it",
                                    ))
                                }
                                Outcome::SwitchOutcomeUnspecified => notifications
                                    .push(Notification::info("Deployment switch finished")),
                            }
                            on_switched.emit(());
                        }
                        Err(e) => {
                            error!("Failed to switch deployment: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to switch deployment: {}",
                                e.message()
                            )));
                        }
                    }
                });
            })
        }
    };

    let enqueue_disabled = *in_flight || *status == DeploymentStatus::Enqueued;
    let hot_redeploy_armed = *armed == Some(ArmedAction::HotRedeploy);
    let enqueue_armed = *armed == Some(ArmedAction::Enqueue);
    html! {
        <div class="deployment-actions">
            <button
                class={classes!(
                    "action-button",
                    hot_redeploy_armed.then_some("confirm"),
                )}
                onclick={
                    if hot_redeploy_armed {
                        switch(true, false)
                    } else {
                        arm(ArmedAction::HotRedeploy)
                    }
                }
                disabled={*in_flight}
            >
                { if hot_redeploy_armed { "Confirm hot redeploy" } else { "Hot redeploy" } }
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
