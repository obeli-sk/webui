use crate::{
    app::Route,
    components::{
        notification::{Notification, NotificationContext},
        system_nav::SystemNav,
        time_display::Timestamp,
    },
    loader::get_server_info,
};
use log::error;
use rest_types::server::ServerInfoResponse;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[component(ServerInfoPage)]
pub fn server_info_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let response = use_state(|| None::<Result<ServerInfoResponse, String>>);

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
                        <dt>{"WebUI version"}</dt>
                        <dd><code>{env!("CARGO_PKG_VERSION")}</code></dd>
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
                            <Link<Route> to={Route::AppPolicy}>
                                <code>{&info.app_config_digest}</code>
                            </Link<Route>>
                        </dd>
                    </dl>
                },
            }}
        </main>
    }
}
