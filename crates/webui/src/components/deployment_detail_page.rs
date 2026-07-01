use crate::{
    BASE_URL,
    app::{AppState, Route},
    components::{
        deployment_actions::DeploymentActions,
        deployment_config_view::{DeploymentConfigView, build_sections_from_manifest, toml_block},
        execution_list_page::ExecutionQuery,
        notification::{Notification, NotificationContext},
    },
    grpc::grpc_client::{
        self, DeploymentId, DeploymentStatus,
        deployment_repository_client::DeploymentRepositoryClient,
        function_repository_client::FunctionRepositoryClient,
    },
    util::time::format_date,
};
use chrono::DateTime;
use hashbrown::HashMap;
use log::error;
use serde_json::Value;
use std::ops::Deref;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentDetailPageProps {
    pub deployment_id: DeploymentId,
}

fn status_badge(status: DeploymentStatus) -> Html {
    match status {
        DeploymentStatus::Active => html! { <span class="badge current">{"Active"}</span> },
        DeploymentStatus::Enqueued => html! { <span class="badge enqueued">{"Enqueued"}</span> },
        DeploymentStatus::Inactive => html! { <span class="badge inactive">{"Inactive"}</span> },
        DeploymentStatus::Unspecified => html! {},
    }
}

fn activity_exec_count(manifest: &Value) -> usize {
    manifest
        .get("activity_exec")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

#[component(DeploymentDetailPage)]
pub fn deployment_detail_page(
    DeploymentDetailPageProps { deployment_id }: &DeploymentDetailPageProps,
) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let app_state = use_context::<AppState>().expect("AppState context must be provided");

    let deployment_state = use_state(|| None::<grpc_client::Deployment>);
    let components_by_name = use_state(HashMap::<String, grpc_client::ComponentId>::new);
    // Bumped after a successful switch action to refetch the deployment.
    let refresh = use_state(|| 0u32);
    // Whether to show the configuration as one TOML document instead of per-component sections.
    let show_toml = use_state(|| false);

    {
        let deployment_state = deployment_state.clone();
        let components_by_name = components_by_name.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (deployment_id.clone(), *refresh),
            move |(deployment_id, _)| {
                let deployment_id = deployment_id.clone();
                spawn_local(async move {
                    let mut client =
                        DeploymentRepositoryClient::new(Client::new(BASE_URL.to_string()));
                    match client
                        .get_deployment(grpc_client::GetDeploymentRequest {
                            deployment_id: Some(deployment_id.clone()),
                        })
                        .await
                    {
                        Ok(resp) => {
                            deployment_state.set(resp.into_inner().deployment);
                        }
                        Err(e) => {
                            error!("Failed to get deployment: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to get deployment: {}",
                                e.message()
                            )));
                        }
                    }
                    // Resolve component IDs of this deployment for links and source fetching.
                    let mut fn_client =
                        FunctionRepositoryClient::new(Client::new(BASE_URL.to_string()));
                    match fn_client
                        .list_components(grpc_client::ListComponentsRequest {
                            function_name: None,
                            component_digest: None,
                            extensions: false,
                            deployment_id: Some(deployment_id),
                        })
                        .await
                    {
                        Ok(resp) => {
                            let map = resp
                                .into_inner()
                                .components
                                .into_iter()
                                .filter_map(|component| component.component_id)
                                .map(|component_id| (component_id.name.clone(), component_id))
                                .collect();
                            components_by_name.set(map);
                        }
                        Err(e) => {
                            // Components may be unavailable for old deployments; not fatal.
                            error!("Failed to list components of the deployment: {e:?}");
                        }
                    }
                });
            },
        );
    }

    let Some(deployment) = deployment_state.deref().clone() else {
        return html! { <p>{"Loading..."}</p> };
    };
    let status = deployment.status();
    let is_current = app_state.current_deployment_id.as_ref() == Some(deployment_id);
    let description = deployment
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty());

    let parsed_manifest: Option<Result<Value, String>> = deployment
        .deployment_toml
        .as_ref()
        .map(|manifest| toml::from_str::<Value>(manifest).map_err(|e| e.to_string()));
    let exec_badge = match &parsed_manifest {
        Some(Ok(manifest)) => match activity_exec_count(manifest) {
            0 => html! {},
            count => {
                let activity_label = if count == 1 { "activity" } else { "activities" };
                html! {
                    <span
                        class="badge dangerous-exec"
                        title={format!(
                            "This deployment includes {count} exec {activity_label}, which run outside the component sandbox"
                        )}
                    >
                        {"⚠ Exec"}
                    </span>
                }
            }
        },
        None | Some(Err(_)) => html! {
            <span
                class="badge dangerous-exec"
                title="The manifest could not be inspected; exec activity status is unknown"
            >
                {"⚠ Exec unknown"}
            </span>
        },
    };

    // A deployment is empty when its manifest parses but yields no component sections.
    let is_empty = matches!(&parsed_manifest, Some(Ok(manifest))
        if build_sections_from_manifest(manifest).is_empty());

    let config_html = match &parsed_manifest {
        None => html! { <p>{"The server did not return the deployment manifest."}</p> },
        Some(Err(parse_err)) => {
            let raw = deployment.deployment_toml.clone().unwrap_or_default();
            html! {<>
                <p class="error">
                    { format!("Cannot parse the deployment manifest, it was probably \
                        written by an incompatible server version: {parse_err}") }
                </p>
                <details>
                    <summary>{"Raw manifest"}</summary>
                    <pre>{ raw }</pre>
                </details>
            </>}
        }
        Some(Ok(manifest)) => {
            let sections = build_sections_from_manifest(manifest);
            if sections.is_empty() {
                html! { <p>{"This deployment is empty."}</p> }
            } else {
                let tab_button = |label: &'static str, toml: bool| {
                    let show_toml = show_toml.clone();
                    let active = *show_toml == toml;
                    html! {
                        <button
                            class={classes!(active.then_some("active"))}
                            onclick={Callback::from(move |_| show_toml.set(toml))}
                        >
                            {label}
                        </button>
                    }
                };
                html! {<>
                    <div class="view-tabs">
                        { tab_button("Components", false) }
                        { tab_button("TOML", true) }
                    </div>
                    if *show_toml {
                        { toml_block(deployment.deployment_toml.clone().unwrap_or_default()) }
                    } else {
                        <DeploymentConfigView
                            sections={sections}
                            components_by_name={components_by_name.deref().clone()}
                            deployment_id={deployment_id.clone()}
                        />
                    }
                </>}
            }
        }
    };

    let on_switched = {
        let refresh = refresh.clone();
        Callback::from(move |()| refresh.set(*refresh + 1))
    };

    let execution_link_query = ExecutionQuery {
        deployment_id: Some(deployment_id.id.clone()),
        ..Default::default()
    };

    html! {
        <>
            <p class="breadcrumbs">
                <Link<Route> to={Route::DeploymentList}>{"Deployments"}</Link<Route>>
            </p>
            <h3>
                {"Deployment "}{ &deployment_id.id }
                {" "}
                { status_badge(status) }
                {" "}
                { exec_badge }
            </h3>
            if let Some(description) = description {
                <p>
                    <strong>{"Description: "}</strong>
                    { description }
                </p>
            }
            <p>
                if let Some(created_at) = deployment.created_at {
                    {"Created: "}{ format_date(DateTime::from(created_at)) }
                }
                if let Some(last_active_at) = deployment.last_active_at {
                    {" | Last active: "}{ format_date(DateTime::from(last_active_at)) }
                }
            </p>
            <p>
                if !is_empty {
                    <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={execution_link_query}>
                        {"Executions of this deployment"}
                    </Link<Route, ExecutionQuery>>
                }
                if let Some(current_id) = &app_state.current_deployment_id
                    && !is_current
                {
                    if !is_empty {
                        {" | "}
                    }
                    <Link<Route> to={Route::DeploymentDiff {
                        from: current_id.clone(),
                        to: deployment_id.clone(),
                    }}>
                        {"Diff against current deployment"}
                    </Link<Route>>
                }
            </p>
            <DeploymentActions
                deployment_id={deployment_id.clone()}
                status={status}
                on_switched={on_switched}
            />
            { config_html }
        </>
    }
}
