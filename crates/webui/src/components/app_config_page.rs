use crate::{
    app::Route,
    components::{
        deployment_config_view::toml_block,
        notification::{Notification, NotificationContext},
        system_nav::SystemNav,
    },
    rest,
};
use log::error;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[component(AppConfigPage)]
pub fn app_config_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let response = use_state(|| None::<Result<String, String>>);

    {
        let response = response.clone();
        use_effect_with((), move |()| {
            spawn_local(async move {
                match rest::get_text_with_accept("/v1/app-toml", &[], "application/toml").await {
                    Ok(app_toml) => response.set(Some(Ok(app_toml))),
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
                Some(Ok(app_toml)) => toml_block(app_toml.clone()),
            }}
        </main>
    }
}
