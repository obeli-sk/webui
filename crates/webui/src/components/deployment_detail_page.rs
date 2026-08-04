use crate::{
    app::{AppState, Route},
    components::{
        deployment_actions::DeploymentActions,
        deployment_config_view::{DeploymentConfigView, build_sections_from_manifest, toml_block},
        execution_list_page::{ExecutionQuery, StatusFilter, StatusFilterList},
        notification::{Notification, NotificationContext},
    },
    grpc::grpc_client::{
        self, DeploymentExecutionSummary, DeploymentId, DeploymentStatus,
        deployment_repository_client::DeploymentRepositoryClient,
        function_repository_client::FunctionRepositoryClient,
        list_deployments_request::{OlderThan, Pagination},
    },
    util::time::format_date,
};
use chrono::DateTime;
use hashbrown::HashMap;
use log::error;
use serde_json::Value;
use std::ops::Deref;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentDetailPageProps {
    pub deployment_id: DeploymentId,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DeploymentTab {
    #[default]
    Overview,
    Components,
    Toml,
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
    let execution_summary = use_state(|| None::<DeploymentExecutionSummary>);
    let components_by_name = use_state(HashMap::<String, grpc_client::Component>::new);
    // Bumped after a successful switch action to refetch the deployment.
    let refresh = use_state(|| 0u32);
    let active_tab = use_state(DeploymentTab::default);
    let show_derived = use_state(|| false);

    {
        let deployment_state = deployment_state.clone();
        let components_by_name = components_by_name.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (deployment_id.clone(), *refresh),
            move |(deployment_id, _)| {
                let deployment_id = deployment_id.clone();
                spawn_local(async move {
                    let mut client = DeploymentRepositoryClient::new(crate::auth::client());
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
                    let mut fn_client = FunctionRepositoryClient::new(crate::auth::client());
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
                                .filter_map(|component| {
                                    let name = component.component_id.as_ref()?.name.clone();
                                    Some((name, component))
                                })
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

    {
        let deployment_id = deployment_id.clone();
        let execution_summary = execution_summary.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (deployment_id.clone(), *show_derived, *refresh),
            move |(deployment_id, show_derived, _)| {
                let deployment_id = deployment_id.clone();
                let show_derived = *show_derived;
                execution_summary.set(None);
                spawn_local(async move {
                    let mut client = DeploymentRepositoryClient::new(crate::auth::client());
                    let response = client
                        .list_deployments(grpc_client::ListDeploymentsRequest {
                            pagination: Some(Pagination::OlderThan(OlderThan {
                                length: 1,
                                cursor: Some(deployment_id.clone()),
                                including_cursor: true,
                            })),
                            include_deployment_toml: false,
                            include_derived: show_derived,
                            include_execution_counts: true,
                            include_component_summary: false,
                        })
                        .await;
                    match response {
                        Ok(response) => {
                            let summary =
                                response
                                    .into_inner()
                                    .deployments
                                    .into_iter()
                                    .find(|summary| {
                                        summary.deployment.as_ref().and_then(|deployment| {
                                            deployment.deployment_id.as_ref()
                                        }) == Some(&deployment_id)
                                    })
                                    .and_then(|summary| summary.execution_summary);
                            execution_summary.set(summary);
                        }
                        Err(e) => {
                            error!("Failed to load deployment execution summary: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to load deployment execution summary: {}",
                                e.message()
                            )));
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

    let components_html = match &parsed_manifest {
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
                html! {
                    <DeploymentConfigView
                        sections={sections}
                        components_by_name={components_by_name.deref().clone()}
                        deployment_id={deployment_id.clone()}
                        allow_submit={is_current}
                    />
                }
            }
        }
    };
    let toml_html = deployment.deployment_toml.as_ref().map_or_else(
        || html! { <p>{"The server did not return the deployment manifest."}</p> },
        |manifest| toml_block(manifest.clone()),
    );

    let on_switched = {
        let refresh = refresh.clone();
        Callback::from(move |()| refresh.set(*refresh + 1))
    };

    let execution_summary_html = execution_summary.as_ref().map(|summary| {
        let total = summary.locked
            + summary.pending
            + summary.scheduled
            + summary.blocked
            + summary.paused
            + summary.cancelling
            + summary.finished_ok
            + summary.finished_error
            + summary.finished_execution_failure;
        let count_metric = |label: &'static str,
                            count: u32,
                            status: Option<StatusFilterList>| {
            let query = ExecutionQuery {
                deployment_id: Some(deployment_id.id.clone()),
                status,
                show_derived: *show_derived,
                ..Default::default()
            };
            html! {
                <div class={classes!("execution-metric", (count == 0).then_some("empty"))}>
                    <span>{label}</span>
                    <strong>
                        if count > 0 {
                            <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={query}>
                                {count}
                            </Link<Route, ExecutionQuery>>
                        } else {
                            {count}
                        }
                    </strong>
                </div>
            }
        };
        html! {
            <div class="deployment-execution-summary">
                { count_metric("Total", total, None) }
                { count_metric("In progress", summary.locked + summary.pending + summary.blocked, Some(StatusFilterList::in_progress())) }
                { count_metric("Scheduled", summary.scheduled, Some(StatusFilterList::single(StatusFilter::Scheduled))) }
                { count_metric("Paused", summary.paused, Some(StatusFilterList::single(StatusFilter::Paused))) }
                { count_metric("Cancelling", summary.cancelling, Some(StatusFilterList::single(StatusFilter::Cancelling))) }
                { count_metric("Successful", summary.finished_ok, Some(StatusFilterList::single(StatusFilter::FinishedOk))) }
                { count_metric("Errors", summary.finished_error, Some(StatusFilterList::single(StatusFilter::FinishedError))) }
                { count_metric("Failures", summary.finished_execution_failure, Some(StatusFilterList::single(StatusFilter::FinishedExecutionFailure))) }
            </div>
        }
    });

    let on_toggle_derived = {
        let show_derived = show_derived.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            show_derived.set(input.checked());
        })
    };

    let tab_button = |label: &'static str, tab: DeploymentTab| {
        let active_tab = active_tab.clone();
        html! {
            <button
                class={classes!((*active_tab == tab).then_some("active"))}
                onclick={Callback::from(move |_| active_tab.set(tab))}
            >
                {label}
            </button>
        }
    };

    let overview_html = html! {
        <section class="deployment-executions">
            <div class="deployment-section-heading">
                <h4>{"Executions"}</h4>
                if !is_empty {
                    <label>
                        <input
                            type="checkbox"
                            checked={*show_derived}
                            onchange={on_toggle_derived}
                        />
                        {" Include derived"}
                    </label>
                }
            </div>
            if is_empty {
                <p class="secondary-text">{"This deployment contains no components."}</p>
            } else if let Some(execution_summary_html) = execution_summary_html {
                { execution_summary_html }
            } else {
                <p>{"Loading execution summary..."}</p>
            }
        </section>
    };

    let tab_content = match *active_tab {
        DeploymentTab::Overview => overview_html,
        DeploymentTab::Components => components_html,
        DeploymentTab::Toml => toml_html,
    };

    html! {
        <>
            <header class="deployment-detail-header">
                <div class="deployment-detail-title">
                    <h3>{ description.unwrap_or("Deployment") }</h3>
                    { status_badge(status) }
                    { exec_badge }
                </div>
                <div class="deployment-detail-id">{ &deployment_id.id }</div>
                <div class="deployment-detail-metadata">
                    if let Some(created_at) = deployment.created_at {
                        <span>{"Created "}{ format_date(DateTime::from(created_at)) }{" UTC"}</span>
                    }
                    if let Some(last_active_at) = deployment.last_active_at {
                        <span>{"Last deployed "}{ format_date(DateTime::from(last_active_at)) }{" UTC"}</span>
                    } else {
                        <span>{"Never deployed"}</span>
                    }
                </div>
            </header>
            <div class="deployment-detail-tools">
                <DeploymentActions
                    deployment_id={deployment_id.clone()}
                    status={status}
                    on_switched={on_switched}
                />
                if let Some(current_id) = &app_state.current_deployment_id
                    && !is_current
                {
                    <Link<Route> to={Route::DeploymentDiff {
                        from: current_id.clone(),
                        to: deployment_id.clone(),
                    }}>
                        {"Diff against current deployment"}
                    </Link<Route>>
                }
            </div>
            <div class="view-tabs deployment-detail-tabs">
                { tab_button("Overview", DeploymentTab::Overview) }
                { tab_button("Components", DeploymentTab::Components) }
                { tab_button("TOML", DeploymentTab::Toml) }
            </div>
            <div class="deployment-tab-content">{ tab_content }</div>
        </>
    }
}
