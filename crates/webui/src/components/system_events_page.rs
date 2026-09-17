use crate::{
    app::Route,
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{
        self, SystemEvent, SystemEventLevel, admin_repository_client::AdminRepositoryClient,
    },
    util::time::format_date,
};
use chrono::DateTime;
use log::error;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

const PAGE_SIZE: u32 = 50;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemEventQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deployment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before: Option<String>,
}

fn level_label(level: SystemEventLevel) -> &'static str {
    match level {
        SystemEventLevel::Info => "Info",
        SystemEventLevel::Warning => "Warning",
        SystemEventLevel::Error => "Error",
        SystemEventLevel::Unspecified => "Unspecified",
    }
}

fn pretty_details(details: &str) -> String {
    serde_json::from_str::<serde_json::Value>(details)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| details.to_string())
}

fn event_row(event: &SystemEvent) -> Html {
    let created_at = event.created_at.map(DateTime::from);
    let execution = event.execution_id.as_ref().map(|execution_id| {
        html! {
            <Link<Route> to={Route::ExecutionTrace { execution_id: execution_id.clone() }}>
                {execution_id.to_string()}
            </Link<Route>>
        }
    });
    let deployment = event.deployment_id.as_ref().map(|deployment_id| {
        html! {
            <Link<Route> to={Route::DeploymentDetail { deployment_id: deployment_id.clone() }}>
                {deployment_id.to_string()}
            </Link<Route>>
        }
    });
    html! {
        <tr>
            <td>{created_at.map(|at| format!("{} UTC", format_date(at))).unwrap_or_default()}</td>
            <td>{level_label(event.level())}</td>
            <td><code>{&event.code}</code></td>
            <td>
                <div>{&event.message}</div>
                if !event.details_json.is_empty() {
                    <details>
                        <summary>{"Details"}</summary>
                        <pre>{pretty_details(&event.details_json)}</pre>
                    </details>
                }
            </td>
            <td>
                {execution}
                if event.execution_id.is_some() && event.deployment_id.is_some() { <br /> }
                {deployment}
            </td>
            <td><code title={event.server_run_id.clone()}>{&event.server_run_id}</code></td>
        </tr>
    }
}

#[component(SystemEventsPage)]
pub fn system_events_page() -> Html {
    let location = use_location().expect("must be rendered inside a router");
    let navigator = use_navigator().expect("must be rendered inside a router");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let query = location.query::<SystemEventQuery>().unwrap_or_default();
    let response = use_state(|| None::<grpc_client::ListSystemEventsResponse>);
    let code_ref = use_node_ref();
    let run_ref = use_node_ref();
    let deployment_ref = use_node_ref();

    {
        let query = query.clone();
        let response = response.clone();
        let notifications = notifications.clone();
        use_effect_with(query, move |query| {
            let request = grpc_client::ListSystemEventsRequest {
                level: query.level,
                code: query.code.clone().filter(|value| !value.is_empty()),
                deployment_id: query
                    .deployment_id
                    .clone()
                    .filter(|value| !value.is_empty())
                    .map(grpc_client::DeploymentId::from),
                before_event_id: query.before.clone(),
                limit: PAGE_SIZE,
                server_run_id: query
                    .server_run_id
                    .clone()
                    .filter(|value| !value.is_empty()),
            };
            spawn_local(async move {
                let mut client = AdminRepositoryClient::new(crate::auth::client());
                match client.list_system_events(request).await {
                    Ok(result) => response.set(Some(result.into_inner())),
                    Err(err) => {
                        error!("Failed to list system events: {err:?}");
                        notifications.push(Notification::error(format!(
                            "Failed to list system events: {}",
                            err.message()
                        )));
                    }
                }
            });
        });
    }

    let apply_filters = {
        let navigator = navigator.clone();
        let code_ref = code_ref.clone();
        let run_ref = run_ref.clone();
        let deployment_ref = deployment_ref.clone();
        let level = query.level;
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();
            let code = code_ref
                .cast::<HtmlInputElement>()
                .map(|input| input.value());
            let server_run_id = run_ref
                .cast::<HtmlInputElement>()
                .map(|input| input.value());
            let deployment_id = deployment_ref
                .cast::<HtmlInputElement>()
                .map(|input| input.value());
            let _ = navigator.push_with_query(
                &Route::SystemEvents,
                &SystemEventQuery {
                    code,
                    level,
                    server_run_id,
                    deployment_id,
                    before: None,
                },
            );
        })
    };

    let set_level = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |level: Option<i32>| {
            let mut query = query.clone();
            query.level = level;
            query.before = None;
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    html! {
        <main>
            <h1>{"System events"}</h1>
            <form onsubmit={apply_filters}>
                <label>{"Code "}<input ref={code_ref} value={query.code.clone().unwrap_or_default()} /></label>
                {" "}
                <label>{"Server run "}<input ref={run_ref} value={query.server_run_id.clone().unwrap_or_default()} /></label>
                {" "}
                <label>{"Deployment "}<input ref={deployment_ref} value={query.deployment_id.clone().unwrap_or_default()} /></label>
                {" "}<button type="submit">{"Filter"}</button>
            </form>
            <p>
                <button onclick={{ let set_level = set_level.clone(); Callback::from(move |_| set_level.emit(None)) }}>{"All"}</button>
                {" "}<button onclick={{ let set_level = set_level.clone(); Callback::from(move |_| set_level.emit(Some(SystemEventLevel::Info as i32))) }}>{"Info"}</button>
                {" "}<button onclick={{ let set_level = set_level.clone(); Callback::from(move |_| set_level.emit(Some(SystemEventLevel::Warning as i32))) }}>{"Warning"}</button>
                {" "}<button onclick={Callback::from(move |_| set_level.emit(Some(SystemEventLevel::Error as i32)))}>{"Error"}</button>
            </p>
            if let Some(response) = response.deref() {
                <table>
                    <thead><tr><th>{"Time"}</th><th>{"Level"}</th><th>{"Code"}</th><th>{"Message"}</th><th>{"Context"}</th><th>{"Server run"}</th></tr></thead>
                    <tbody>{for response.events.iter().map(event_row)}</tbody>
                </table>
                <p>
                    if query.before.is_some() {
                        <button onclick={{ let navigator = navigator.clone(); Callback::from(move |_| navigator.back()) }}>{"Newer"}</button>
                    }
                    if let Some(cursor) = &response.next_cursor {
                        {" "}<Link<Route, SystemEventQuery>
                            to={Route::SystemEvents}
                            query={SystemEventQuery { before: Some(cursor.clone()), ..query.clone() }}
                        >{"Older"}</Link<Route, SystemEventQuery>>
                    }
                </p>
            } else {
                <p>{"Loading system events…"}</p>
            }
        </main>
    }
}
