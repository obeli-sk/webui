use crate::app::Route;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(PartialEq, Properties)]
pub struct SystemNavProps {
    pub active: Route,
}

#[component(SystemNav)]
pub fn system_nav(props: &SystemNavProps) -> Html {
    html! {
        <div class="system-nav" aria-label="System pages">
            <Link<Route> classes={classes!((props.active == Route::SystemEvents).then_some("active"))} to={Route::SystemEvents}>{"System events"}</Link<Route>>
            <Link<Route> classes={classes!((props.active == Route::AppConfig).then_some("active"))} to={Route::AppConfig}>{"App config"}</Link<Route>>
        </div>
    }
}
