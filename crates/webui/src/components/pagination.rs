use crate::app::Route;
use serde::Serialize;
use yew::prelude::*;
use yew_router::prelude::*;

/// Renders a pagination link, or an inert placeholder when there is no page to go to.
pub fn page_link<Q>(route: Route, query: Option<Q>, label: &'static str) -> Html
where
    Q: Serialize + Clone + PartialEq + 'static,
{
    match query {
        Some(query) => html! {
            <Link<Route, Q> to={route} query={Some(query)}>{label}</Link<Route, Q>>
        },
        None => html! { <span class="disabled" aria-disabled="true">{label}</span> },
    }
}
