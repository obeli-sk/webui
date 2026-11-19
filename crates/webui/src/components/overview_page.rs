use crate::{
    app::{AppState, Route},
    components::{
        deployment_config_view::build_sections_from_manifest,
        deployment_executions::DeploymentExecutions,
        http_servers::{HttpServerRoutes, http_server_routes},
        notification::{Notification, NotificationContext},
        system_events_page::{DEFAULT_LEVELS, event_card},
        time_display::Timestamp,
    },
    loader::{ServerInfo, get_server_info},
    rest,
};
use hashbrown::HashMap;
use log::error;
use rest_types::{
    admin::SystemEventsResponse, components::ComponentConfig, deployment::DeploymentRecordSer,
};
use serde_json::Value;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

const RECENT_EVENTS: usize = 5;

#[component(OverviewPage)]
pub fn overview_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let app_state = use_context::<AppState>().expect("AppState context must be provided");
    let deployment_id = app_state.current_deployment_id;
    let info = use_state(|| None::<ServerInfo>);
    let deployment = use_state(|| None::<DeploymentRecordSer>);
    let components_by_name = use_state(|| Rc::new(HashMap::<String, ComponentConfig>::new()));
    let events = use_state(|| None::<Vec<rest_types::admin::SystemEventResponse>>);

    {
        let info = info.clone();
        let deployment = deployment.clone();
        let components_by_name = components_by_name.clone();
        let events = events.clone();
        use_effect_with(deployment_id, move |deployment_id| {
            let deployment_id = *deployment_id;
            spawn_local(async move {
                match get_server_info().await {
                    Ok(response) => info.set(Some(response)),
                    Err(err) => {
                        error!("Failed to get server info: {err}");
                        notifications.push(Notification::error(format!(
                            "Failed to get server info: {err}"
                        )));
                    }
                }
                if let Some(deployment_id) = deployment_id {
                    match rest::deployments::get(&deployment_id).await {
                        Ok(response) => deployment.set(Some(response)),
                        Err(err) => error!("Failed to get the current deployment: {err}"),
                    }
                    match rest::components::list(Some(&deployment_id), None).await {
                        Ok(components) => components_by_name.set(Rc::new(
                            components
                                .into_iter()
                                .map(|component| {
                                    (component.component_id.name.to_string(), component)
                                })
                                .collect(),
                        )),
                        Err(err) => error!("Failed to list components: {err}"),
                    }
                }
                // The API filters by a single level, so ask for each default level.
                let mut recent = Vec::new();
                for level in DEFAULT_LEVELS {
                    let params = [
                        ("limit", RECENT_EVENTS.to_string()),
                        ("level", level.as_str().to_string()),
                    ];
                    match rest::get::<SystemEventsResponse>("/v1/admin/system-events", &params)
                        .await
                    {
                        Ok(mut page) => recent.append(&mut page.events),
                        Err(err) => {
                            error!("Failed to list system events: {err}");
                            return;
                        }
                    }
                }
                recent.sort_unstable_by_key(|event| std::cmp::Reverse(event.event_id));
                recent.truncate(RECENT_EVENTS);
                events.set(Some(recent));
            });
        });
    }

    let manifest = deployment
        .as_ref()
        .and_then(|deployment| toml::from_str::<Value>(&deployment.deployment_toml).ok());
    let empty = match (&*deployment, &manifest) {
        (Some(deployment), Some(manifest)) => {
            build_sections_from_manifest(manifest, &deployment.files).is_empty()
        }
        _ => false,
    };
    let servers = Rc::new(
        manifest
            .as_ref()
            .map(http_server_routes)
            .unwrap_or_default(),
    );

    html! {
        <main class="overview-page">
            <h1>{"Overview"}</h1>
            <section class="overview-server">
                if let Some(info) = &*info {
                    <dl class="server-info">
                        <dt>{"App name"}</dt>
                        <dd>{&info.app_name}</dd>
                        <dt>{"Obelisk version"}</dt>
                        <dd><code>{&info.obelisk_version}</code></dd>
                        <dt>{"Started"}</dt>
                        <dd><Timestamp target={info.started_at} /></dd>
                        <dt>{"Active deployment"}</dt>
                        <dd>
                            <Link<Route> to={Route::DeploymentDetail { deployment_id: info.deployment_id }}>
                                {
                                    deployment
                                        .as_ref()
                                        .and_then(|deployment| deployment.description.clone())
                                        .filter(|description| !description.trim().is_empty())
                                        .map_or_else(|| html! { <code>{info.deployment_id.to_string()}</code> }, Html::from)
                                }
                            </Link<Route>>
                            if let Some(last_active_at) = deployment.as_ref().and_then(|deployment| deployment.last_active_at) {
                                {" deployed "}<Timestamp target={last_active_at} />
                            }
                        </dd>
                    </dl>
                    <Link<Route> to={Route::ServerInfo}>{"More server info"}</Link<Route>>
                } else {
                    <p>{"Loading server info…"}</p>
                }
            </section>
            if let Some(deployment_id) = deployment_id {
                <DeploymentExecutions {deployment_id} {empty} />
                if deployment.is_some() && !empty {
                    <HttpServerRoutes
                        {deployment_id}
                        {servers}
                        components_by_name={(*components_by_name).clone()}
                    />
                }
            }
            <section class="overview-events">
                <div class="deployment-section-heading">
                    <h4>{"Recent system events"}</h4>
                    <Link<Route> to={Route::SystemEvents}>{"All system events"}</Link<Route>>
                </div>
                {match &*events {
                    None => html! { <p>{"Loading system events…"}</p> },
                    Some(events) if events.is_empty() => html! { <p class="secondary-text">{"No system events."}</p> },
                    Some(events) => html! {
                        <div class="system-event-list">{for events.iter().map(|event| event_card(event, false))}</div>
                    },
                }}
            </section>
        </main>
    }
}
