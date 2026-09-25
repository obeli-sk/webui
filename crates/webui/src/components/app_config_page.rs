use crate::{
    app::Route,
    components::{
        notification::{Notification, NotificationContext},
        system_nav::SystemNav,
    },
    rest,
};
use log::error;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Deserialize)]
struct AppConfigResponse {
    app_config_digest: String,
    policy: serde_json::Value,
}

#[component(AppConfigPage)]
pub fn app_config_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let response = use_state(|| None::<Result<AppConfigResponse, String>>);

    {
        let response = response.clone();
        use_effect_with((), move |()| {
            spawn_local(async move {
                match rest::get::<AppConfigResponse>("/v1/app-config", &[]).await {
                    Ok(config) => response.set(Some(Ok(config))),
                    Err(err) => {
                        error!("Failed to get app config: {err}");
                        notifications.push(Notification::error(format!(
                            "Failed to get app config: {err}"
                        )));
                        response.set(Some(Err(err)));
                    }
                }
            });
        });
    }

    html! {
        <main>
            <SystemNav active={Route::AppConfig} />
            <h1>{"App config"}</h1>
            {match response.as_ref() {
                None => html! { <p>{"Loading app config…"}</p> },
                Some(Err(err)) => html! { <p class="error">{format!("Cannot load app config: {err}")}</p> },
                Some(Ok(config)) => html! {
                    <>
                        <p class="app-config-digest">{"Running app policy digest: "}<code>{&config.app_config_digest}</code></p>
                        <h2>{"Policy"}</h2>
                        <pre class="app-config-policy">{serde_json::to_string_pretty(&config.policy).unwrap_or_default()}</pre>
                    </>
                },
            }}
        </main>
    }
}
