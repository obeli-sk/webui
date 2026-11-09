use crate::{
    app::Route,
    components::{
        notification::{Notification, NotificationContext},
        system_nav::SystemNav,
        time_display::Timestamp,
    },
    loader::{ServerInfo, get_server_info},
    util::time::format_exact_millis,
};
use log::error;
use obelisk_api_types::{ServerInfoGc, ServerInfoGcCountedRetention};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[component(ServerInfoPage)]
pub fn server_info_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let response = use_state(|| None::<Result<ServerInfo, String>>);

    {
        let response = response.clone();
        use_effect_with((), move |()| {
            spawn_local(async move {
                match get_server_info().await {
                    Ok(info) => response.set(Some(Ok(info))),
                    Err(err) => {
                        error!("Failed to get server info: {err}");
                        notifications.push(Notification::error(format!(
                            "Failed to get server info: {err}"
                        )));
                        response.set(Some(Err(err)));
                    }
                }
            });
        });
    }

    html! {
        <main>
            <SystemNav active={Route::ServerInfo} />
            <h1>{"Info"}</h1>
            {match response.as_ref() {
                None => html! { <p>{"Loading server info…"}</p> },
                Some(Err(err)) => html! { <p class="error">{format!("Cannot load server info: {err}")}</p> },
                Some(Ok(info)) => html! {
                    <dl class="server-info">
                        <dt>{"App name"}</dt>
                        <dd>{&info.app_name}</dd>
                        if !info.app_env_prefix.is_empty() {
                            <dt>{"App env prefix"}</dt>
                            <dd><code>{&info.app_env_prefix}</code></dd>
                        }
                        <dt>{"Obelisk version"}</dt>
                        <dd><code>{&info.obelisk_version}</code></dd>
                        <dt>{"Started"}</dt>
                        <dd><Timestamp target={info.started_at} /></dd>
                        <dt>{"Active deployment"}</dt>
                        <dd>
                            <Link<Route> to={Route::DeploymentDetail { deployment_id: info.deployment_id }}>
                                <code>{info.deployment_id.to_string()}</code>
                            </Link<Route>>
                        </dd>
                        <dt>{"Node run ID"}</dt>
                        <dd><code>{info.node_run_id.to_string()}</code></dd>
                        <dt>{"App config digest"}</dt>
                        <dd>
                            <Link<Route> to={Route::AppConfig}>
                                <code>{&info.app_config_digest}</code>
                            </Link<Route>>
                        </dd>
                    </dl>
                },
            }}
            if let Some(Ok(info)) = response.as_ref() {
                <h2>{"Garbage collection"}</h2>
                {match &info.gc {
                    None => html! { <p>{"Not reported by this server."}</p> },
                    Some(gc) => gc_config(gc),
                }}
            }
        </main>
    }
}

fn gc_config(gc: &ServerInfoGc) -> Html {
    let retention = &gc.retention;
    html! {
        <dl class="server-info">
            <dt>{"Status"}</dt>
            <dd>{if gc.enabled { "Enabled" } else { "Disabled" }}</dd>
            <dt>{"Interval"}</dt>
            <dd>{format_exact_millis(gc.interval_ms)}</dd>
            <dt>{"Batch size"}</dt>
            <dd>{gc.batch_size}</dd>
            <dt>{"Batch delay"}</dt>
            <dd>{format_exact_millis(gc.batch_delay_ms)}</dd>
            <dt>{"Upload grace period"}</dt>
            <dd>{format_exact_millis(gc.upload_grace_period_ms)}</dd>
            <dt>{"Execution retention"}</dt>
            <dd>{counted_retention(&retention.executions)}</dd>
            <dt>{"Deployment retention"}</dt>
            <dd>{counted_retention(&retention.deployments)}</dd>
            <dt>{"System event retention"}</dt>
            <dd>{
                if retention.system_events.enabled {
                    format!("max age {}", format_exact_millis(retention.system_events.max_age_ms))
                } else {
                    "Disabled".to_string()
                }
            }</dd>
        </dl>
    }
}

fn counted_retention(retention: &ServerInfoGcCountedRetention) -> String {
    if !retention.enabled {
        return "Disabled".to_string();
    }
    let max_age = format_exact_millis(retention.max_age_ms);
    if retention.max_count == 0 {
        format!("max age {max_age}, no count limit")
    } else {
        format!("max age {max_age}, max count {}", retention.max_count)
    }
}
