use crate::{
    app::{AppState, Route},
    components::{
        component_graph::ComponentGraph,
        deployment_actions::DeploymentActions,
        deployment_config_view::{DeploymentConfigView, build_sections_from_manifest, toml_block},
        deployment_executions::DeploymentExecutions,
        http_servers::{HttpServerRoutes, http_server_routes},
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
    deployment::{DeploymentRecordSer, DeploymentStatusSer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::Deref;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;
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
    let components_by_name = use_state(|| Rc::new(HashMap::<String, ComponentConfig>::new()));
    // Bumped after a successful switch action to refetch the deployment.
    let refresh = use_state(|| 0u32);
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

    let servers = Rc::new(
        parsed_manifest
            .as_ref()
            .map(http_server_routes)
            .unwrap_or_default(),
    );

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
        <DeploymentExecutions deployment_id={*deployment_id} empty={is_empty} refresh={*refresh} />
        if !is_empty {
            <HttpServerRoutes
                deployment_id={*deployment_id}
                servers={servers.clone()}
                components_by_name={components_by_name.deref().clone()}
            />
        }
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
                servers={servers}
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
