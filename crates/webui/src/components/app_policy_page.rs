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

#[component(AppPolicyPage)]
pub fn app_policy_page() -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let response = use_state(|| None::<Result<String, String>>);

    {
        let response = response.clone();
        use_effect_with((), move |()| {
            spawn_local(async move {
                match rest::get_text_with_accept("/v1/app-policy", &[], "application/toml").await {
                    Ok(policy) => response.set(Some(Ok(policy))),
                    Err(err) => {
                        error!("Failed to get app policy: {err}");
                        notifications.push(Notification::error(format!(
                            "Failed to get app policy: {err}"
                        )));
                        response.set(Some(Err(err)));
                    }
                }
            });
        });
    }

    html! {
        <main>
            <SystemNav active={Route::AppPolicy} />
            <h1>{"App policy"}</h1>
            {match response.as_ref() {
                None => html! { <p>{"Loading app policy…"}</p> },
                Some(Err(err)) => html! { <p class="error">{format!("Cannot load app policy: {err}")}</p> },
                Some(Ok(policy)) => toml_block(policy.clone()),
            }}
        </main>
    }
}
