use crate::{
    app::{AppState, Route},
    components::{
        component_graph::ComponentGraph,
        deployment_actions::DeploymentActions,
        deployment_config_view::{DeploymentConfigView, build_sections_from_manifest, toml_block},
        execution_list_page::{ExecutionQuery, StatusFilter, StatusFilterList},
        notification::{Notification, NotificationContext},
    },
    rest,
    util::time::format_date,
};
use concepts::prefixed_ulid::DeploymentId;
use hashbrown::HashMap;
use log::error;
use rest_types::{
    components::ComponentConfig,
    deployment::{DeploymentRecordSer, DeploymentStateSer, DeploymentStatusSer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::Deref;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::{
    history::{BrowserHistory, History},
    prelude::*,
};

/// Optional query of the deployment detail page: the component to open and scroll to.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DeploymentQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

#[derive(Properties, PartialEq)]
pub struct DeploymentDetailPageProps {
    pub deployment_id: DeploymentId,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DeploymentTab {
    #[default]
    Overview,
    Graph,
    Toml,
}

impl DeploymentTab {
    fn from_hash(hash: &str) -> Self {
        match hash.strip_prefix('#').unwrap_or(hash) {
            // backcompat: 0.28.1 links used #components before the tabs were merged.
            "components" => Self::Overview,
            "graph" => Self::Graph,
            "toml" => Self::Toml,
            _ => Self::Overview,
        }
    }

    fn fragment(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Graph => "graph",
            Self::Toml => "toml",
        }
    }
}

fn status_badge(status: DeploymentStatusSer) -> Html {
    match status {
        DeploymentStatusSer::Active => html! { <span class="badge current">{"Active"}</span> },
        DeploymentStatusSer::Enqueued => html! { <span class="badge enqueued">{"Enqueued"}</span> },
        DeploymentStatusSer::Inactive => html! { <span class="badge inactive">{"Inactive"}</span> },
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

    let deployment_state = use_state(|| None::<DeploymentRecordSer>);
    let execution_summary = use_state(|| None::<DeploymentStateSer>);
    let components_by_name = use_state(|| Rc::new(HashMap::<String, ComponentConfig>::new()));
    // Bumped after a successful switch action to refetch the deployment.
    let refresh = use_state(|| 0u32);
    let show_derived = use_state(|| false);
    let location = use_location().expect("location must be available inside a router");
    let active_tab = DeploymentTab::from_hash(location.hash());
    let focused_component = location
        .query::<DeploymentQuery>()
        .unwrap_or_default()
        .component;

    {
        let deployment_state = deployment_state.clone();
        let components_by_name = components_by_name.clone();
        let notifications = notifications.clone();
        use_effect_with((*deployment_id, *refresh), move |(deployment_id, _)| {
            let deployment_id = *deployment_id;
            spawn_local(async move {
                match rest::deployments::get(&deployment_id).await {
                    Ok(resp) => {
                        deployment_state.set(Some(resp));
                    }
                    Err(e) => {
                        error!("Failed to get deployment: {e:?}");
                        notifications.push(Notification::error(format!(
                            "Failed to get deployment: {}",
                            e
                        )));
                    }
                }
                // Resolve component IDs of this deployment for links and source fetching.
                match rest::components::list(Some(&deployment_id), None).await {
                    Ok(components) => {
                        let map = components
                            .into_iter()
                            .map(|component| (component.component_id.name.to_string(), component))
                            .collect();
                        components_by_name.set(Rc::new(map));
                    }
                    Err(e) => {
                        // Components may be unavailable for old deployments; not fatal.
                        error!("Failed to list components of the deployment: {e:?}");
                    }
                }
            });
        });
    }

    {
        let deployment_id = *deployment_id;
        let execution_summary = execution_summary.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (deployment_id, *show_derived, *refresh),
            move |(deployment_id, show_derived, _)| {
                let deployment_id = *deployment_id;
                let show_derived = *show_derived;
                execution_summary.set(None);
                spawn_local(async move {
                    let response = rest::deployments::list(
                        Some(&deployment_id),
                        "older",
                        1,
                        true,
                        show_derived,
                        false,
                    )
                    .await;
                    match response {
                        Ok(response) => {
                            let summary = response
                                .into_iter()
                                .find(|summary| summary.deployment_id == deployment_id);
                            execution_summary.set(summary);
                        }
                        Err(e) => {
                            error!("Failed to load deployment execution summary: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to load deployment execution summary: {}",
                                e
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
    let status = deployment.status;
    let is_current = app_state.current_deployment_id.as_ref() == Some(deployment_id);
    let description = deployment
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty());

    let parsed_manifest: Result<Value, String> =
        toml::from_str::<Value>(&deployment.deployment_toml).map_err(|e| e.to_string());
    let exec_badge = match &parsed_manifest {
        Ok(manifest) => match activity_exec_count(manifest) {
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
        Err(_) => html! {
            <span
                class="badge dangerous-exec"
                title="The manifest could not be inspected; exec activity status is unknown"
            >
                {"⚠ Exec unknown"}
            </span>
        },
    };

    // A deployment is empty when its manifest parses but yields no component sections.
    let is_empty = matches!(&parsed_manifest, Ok(manifest)
        if build_sections_from_manifest(manifest, &deployment.files).is_empty());

    let components_html = match &parsed_manifest {
        Err(parse_err) => {
            let raw = deployment.deployment_toml.clone();
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
        Ok(manifest) => {
            let sections = build_sections_from_manifest(manifest, &deployment.files);
            if sections.is_empty() {
                html! { <p>{"This deployment is empty."}</p> }
            } else {
                html! {
                    <DeploymentConfigView
                        sections={sections}
                        components_by_name={components_by_name.deref().clone()}
                        deployment_id={*deployment_id}
                        focused_component={focused_component.clone()}
                    />
                }
            }
        }
    };
    let toml_html = toml_block(deployment.deployment_toml.clone());

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
                deployment_id: Some(deployment_id.to_string()),
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
        let tab_url = format!(
            "{}{}#{}",
            location.path(),
            location.query_str(),
            tab.fragment()
        );
        html! {
            <button
                class={classes!((active_tab == tab).then_some("active"))}
                onclick={Callback::from(move |_| {
                    if active_tab != tab {
                        BrowserHistory::new().push(&tab_url);
                    }
                })}
            >
                {label}
            </button>
        }
    };

    let overview_html = html! {
        <>
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
        <section class="deployment-components">
            <div class="deployment-section-heading">
                <h4>{"Components"}</h4>
            </div>
            {components_html}
        </section>
        </>
    };

    let tab_content = match active_tab {
        DeploymentTab::Overview => overview_html,
        DeploymentTab::Graph => html! {
            <ComponentGraph
                deployment_id={*deployment_id}
                components_by_name={components_by_name.deref().clone()}
            />
        },
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
                <div class="deployment-detail-id">{ deployment_id.to_string() }</div>
                <div class="deployment-detail-metadata">
                    <span>{"Created "}{ format_date(deployment.created_at) }{" UTC"}</span>
                    if let Some(last_active_at) = deployment.last_active_at {
                        <span>{"Last deployed "}{ format_date(last_active_at) }{" UTC"}</span>
                    } else {
                        <span>{"Never deployed"}</span>
                    }
                </div>
            </header>
            <div class="deployment-detail-tools">
                <DeploymentActions
                    deployment_id={*deployment_id}
                    status={status}
                    on_switched={on_switched}
                />
                if let Some(current_id) = &app_state.current_deployment_id
                    && !is_current
                {
                    <Link<Route> to={Route::DeploymentDiff {
                        from: *current_id,
                        to: *deployment_id,
                    }}>
                        {"Diff against current deployment"}
                    </Link<Route>>
                }
            </div>
            <div class="view-tabs deployment-detail-tabs">
                { tab_button("Overview", DeploymentTab::Overview) }
                { tab_button("Graph", DeploymentTab::Graph) }
                { tab_button("TOML", DeploymentTab::Toml) }
            </div>
            <div class="deployment-tab-content">{ tab_content }</div>
        </>
    }
}
