use crate::{
    app::{AppState, Route},
    components::deployment_detail_page::DeploymentQuery,
};
use concepts::{ComponentId, prefixed_ulid::DeploymentId};
use log::error;
use serde::{Deserialize, Serialize};
use yew::prelude::*;
use yew_router::hooks::{use_location, use_navigator};

/// Query of the former component detail page: the deployment the component belongs to.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
struct ComponentQuery {
    deployment_id: Option<DeploymentId>,
}

#[derive(Properties, PartialEq)]
pub struct ComponentRedirectProps {
    #[prop_or_default]
    pub component_id: Option<ComponentId>,
}

// backcompat: 0.28.1 had standalone pages at /components and /component/:component_id; components now live on the deployment detail page.
#[component(ComponentRedirect)]
pub fn component_redirect(
    ComponentRedirectProps { component_id }: &ComponentRedirectProps,
) -> Html {
    let app_state = use_context::<AppState>().expect("AppState context must be provided");
    let location = use_location().expect("location must be available inside a router");
    let navigator = use_navigator().expect("navigator must be available inside a router");
    let query_deployment_id = location
        .query::<ComponentQuery>()
        .unwrap_or_default()
        .deployment_id;
    let in_active_deployment = component_id
        .as_ref()
        .is_some_and(|component_id| app_state.components_by_id.contains_key(component_id));
    let target = match query_deployment_id {
        Some(id) if !in_active_deployment => Some(id),
        _ => app_state.current_deployment_id,
    };
    let component = component_id
        .as_ref()
        .map(|component_id| component_id.name.to_string());
    use_effect_with((target, component), move |(target, component)| {
        if let Some(deployment_id) = *target {
            let query = DeploymentQuery {
                component: component.clone(),
            };
            if let Err(err) =
                navigator.replace_with_query(&Route::DeploymentDetail { deployment_id }, &query)
            {
                error!("Cannot redirect to the deployment: {err:?}");
            }
        }
    });
    if target.is_some() {
        html! { <p>{"Redirecting..."}</p> }
    } else {
        html! { <p>{"No active deployment found."}</p> }
    }
}
