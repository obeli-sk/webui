use crate::{
    BASE_URL,
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{
        self, DeploymentId, DeploymentStatus,
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
    // Which `(hot_redeploy, verify)` action is armed for the confirming second click.
    let armed = use_state(|| None::<(bool, bool)>);
    // Dropping the previous timeout cancels it when another button is armed.
    let disarm_timer = use_mut_ref(|| None::<Timeout>);

    if *status == DeploymentStatus::Active {
        return html! {};
    }

    let switch = {
        let deployment_id = deployment_id.clone();
        let notifications = notifications.clone();
        let on_switched = on_switched.clone();
        let in_flight = in_flight.clone();
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        // `verify` only matters for enqueueing; hot redeploy always verifies.
        move |hot_redeploy: bool, verify: bool| {
            let deployment_id = deployment_id.clone();
            let notifications = notifications.clone();
            let on_switched = on_switched.clone();
            let in_flight = in_flight.clone();
            let armed = armed.clone();
            let disarm_timer = disarm_timer.clone();
            Callback::from(move |_| {
                // First click arms the button, second click within the timeout executes.
                if *armed != Some((hot_redeploy, verify)) {
                    armed.set(Some((hot_redeploy, verify)));
                    let armed = armed.clone();
                    *disarm_timer.borrow_mut() = Some(Timeout::new(5000, move || armed.set(None)));
                    return;
                }
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
                            verify,
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
    let is_armed = |hot_redeploy: bool, verify: bool| *armed == Some((hot_redeploy, verify));
    html! {
        <div class="deployment-actions">
            <button
                class={classes!("action-button", is_armed(true, true).then_some("armed"))}
                onclick={switch(true, true)}
                disabled={*in_flight}
            >
                { if is_armed(true, true) { "Confirm hot redeploy" } else { "Hot redeploy" } }
            </button>
            <button
                class={classes!("action-button", is_armed(false, true).then_some("armed"))}
                onclick={switch(false, true)}
                disabled={enqueue_disabled}
                title="Runs `obelisk server verify` on the deployment before enqueueing it"
            >
                { if is_armed(false, true) { "Confirm enqueue" } else { "Verify and enqueue for next restart" } }
            </button>
            <button
                class={classes!("action-button", "warning", is_armed(false, false).then_some("armed"))}
                onclick={switch(false, false)}
                disabled={enqueue_disabled}
                title="Enqueues the deployment as-is; problems will only surface at the next server restart"
            >
                { if is_armed(false, false) { "Confirm enqueue without verifying" } else { "Enqueue without verifying" } }
            </button>
        </div>
    }
}
