use crate::{
    app::{AppState, Route},
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{self, SystemEventLevel},
    rest,
    util::time::{RelativeAgo, format_date},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use log::error;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, ops::Deref, str::FromStr};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

const PAGE_SIZE: u32 = 50;
const FILTER_LEVELS: [SystemEventLevel; 4] = [
    SystemEventLevel::Debug,
    SystemEventLevel::Info,
    SystemEventLevel::Warning,
    SystemEventLevel::Error,
];
const DEFAULT_LEVELS: [SystemEventLevel; 3] = [
    SystemEventLevel::Info,
    SystemEventLevel::Warning,
    SystemEventLevel::Error,
];

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
)]
struct LevelFilter(Vec<SystemEventLevel>);

impl Display for LevelFilter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, level) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(",")?;
            }
            formatter.write_str(level.as_str_name())?;
        }
        Ok(())
    }
}

impl FromStr for LevelFilter {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let levels = value
            .split(',')
            .filter(|value| !value.is_empty())
            .map(|value| {
                SystemEventLevel::from_str_name(value)
                    .filter(|level| FILTER_LEVELS.contains(level))
                    .ok_or_else(|| format!("invalid system event level `{value}`"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(levels))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemEventQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    levels: Option<LevelFilter>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    all_runs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    all_deployments: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deployment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    absolute_time: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct EventPage {
    events: Vec<SystemEvent>,
    next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct SystemEvent {
    event_id: String,
    node_run_id: String,
    created_at: DateTime<Utc>,
    level: String,
    code: String,
    message: String,
    execution_id: Option<String>,
    deployment_id: Option<String>,
    details: serde_json::Value,
}

fn selected_levels(query: &SystemEventQuery) -> Vec<SystemEventLevel> {
    query
        .levels
        .as_ref()
        .map_or_else(|| DEFAULT_LEVELS.to_vec(), |levels| levels.0.clone())
}

// `datetime-local` inputs omit the seconds when they are zero.
fn parse_utc(value: &str) -> Result<DateTime<Utc>, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M"))
        .map(|naive| naive.and_utc())
        .map_err(|_| format!("invalid UTC date and time `{value}`"))
}

fn parse_bound(value: Option<&String>) -> Result<Option<String>, String> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| parse_utc(value).map(|time| time.to_rfc3339()))
        .transpose()
}

fn input_value(node_ref: &NodeRef) -> Option<String> {
    node_ref
        .cast::<HtmlInputElement>()
        .map(|input| input.value().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn level_label(level: SystemEventLevel) -> &'static str {
    match level {
        SystemEventLevel::Debug => "Debug",
        SystemEventLevel::Info => "Info",
        SystemEventLevel::Warning => "Warning",
        SystemEventLevel::Error => "Error",
        SystemEventLevel::Unspecified => "Unspecified",
    }
}

fn event_card(event: &SystemEvent, absolute_time: bool) -> Html {
    let created_at = event.created_at;
    let execution = event.execution_id.as_ref().map(|execution_id| {
        html! {
            <Link<Route> to={Route::ExecutionTrace { execution_id: grpc_client::ExecutionId { id: execution_id.clone() } }}>
                <code>{execution_id}</code>
            </Link<Route>>
        }
    });
    let deployment = event.deployment_id.as_ref().map(|deployment_id| {
        html! {
            <Link<Route> to={Route::DeploymentDetail { deployment_id: grpc_client::DeploymentId { id: deployment_id.clone() } }}>
                <code>{deployment_id}</code>
            </Link<Route>>
        }
    });
    let level = match event.level.as_str() {
        "debug" => SystemEventLevel::Debug,
        "info" => SystemEventLevel::Info,
        "warning" => SystemEventLevel::Warning,
        "error" => SystemEventLevel::Error,
        _ => SystemEventLevel::Unspecified,
    };
    html! {
        <article class="system-event-list-item">
            <div class="system-event-heading">
                <span class={classes!("badge", "system-event-level", level_label(level).to_lowercase())}>
                    {level_label(level)}
                </span>
                <code class="system-event-code">{&event.code}</code>
                <time title={format!("{} UTC", format_date(created_at))}>
                    if absolute_time {
                        {format!("{} UTC", format_date(created_at))}
                    } else {
                        <RelativeAgo target={created_at} />
                    }
                </time>
            </div>
            <div class="system-event-message">{&event.message}</div>
            <div class="system-event-metadata">
                <Link<Route, SystemEventQuery>
                    to={Route::SystemEvents}
                    query={Some(SystemEventQuery { id: Some(event.event_id.clone()), absolute_time, ..SystemEventQuery::default() })}
                >
                    <code>{&event.event_id}</code>
                </Link<Route, SystemEventQuery>>
                <code title={event.node_run_id.clone()}>{&event.node_run_id}</code>
                {deployment}
                {execution}
            </div>
            if !event.details.is_null() && event.details != serde_json::json!({}) {
                <details class="system-event-details">
                    <summary>{"Details"}</summary>
                    <pre>{serde_json::to_string_pretty(&event.details).unwrap_or_default()}</pre>
                </details>
            }
        </article>
    }
}

#[component(SystemEventsPage)]
pub fn system_events_page() -> Html {
    let location = use_location().expect("must be rendered inside a router");
    let navigator = use_navigator().expect("must be rendered inside a router");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let app_state = use_context::<AppState>().expect("AppState context must be provided");
    let current_deployment_id = app_state.current_deployment_id.clone();
    let query = location.query::<SystemEventQuery>().unwrap_or_default();
    let current_run_id = use_state(|| None::<String>);
    let response = use_state(|| None::<EventPage>);
    let code_ref = use_node_ref();
    let run_ref = use_node_ref();
    let deployment_ref = use_node_ref();
    let from_ref = use_node_ref();
    let to_ref = use_node_ref();
    let id_ref = use_node_ref();

    {
        let current_run_id = current_run_id.clone();
        let notifications = notifications.clone();
        use_effect_with((), move |()| {
            spawn_local(async move {
                match rest::get::<String>("/v1/admin/node-run-id", &[]).await {
                    Ok(result) => current_run_id.set(Some(result)),
                    Err(err) => {
                        error!("Failed to get current node run ID: {err:?}");
                        notifications.push(Notification::error(format!(
                            "Failed to get current node run ID: {}",
                            err
                        )));
                    }
                }
            });
        });
    }

    {
        let query = query.clone();
        let response = response.clone();
        let notifications = notifications.clone();
        let current_run_id = current_run_id.deref().clone();
        let current_deployment_id = current_deployment_id.clone();
        use_effect_with(
            (query, current_run_id, current_deployment_id),
            move |(query, current_run_id, current_deployment_id)| {
                if let Some(event_id) = query.id.clone() {
                    response.set(None);
                    spawn_local(async move {
                        let event_id = js_sys::encode_uri_component(&event_id)
                            .as_string()
                            .expect("encoded event ID is a string");
                        match rest::get::<SystemEvent>(
                            &format!("/v1/admin/system-events/{event_id}"),
                            &[],
                        )
                        .await
                        {
                            Ok(result) => response.set(Some(EventPage {
                                events: vec![result],
                                next_cursor: None,
                            })),
                            Err(err) => {
                                error!("Failed to get system event: {err:?}");
                                notifications.push(Notification::error(format!(
                                    "Failed to get system event: {}",
                                    err
                                )));
                                response.set(Some(EventPage {
                                    events: Vec::new(),
                                    next_cursor: None,
                                }));
                            }
                        }
                    });
                    return;
                }
                let (created_from, created_to) = match (
                    parse_bound(query.from.as_ref()),
                    parse_bound(query.to.as_ref()),
                ) {
                    (Ok(from), Ok(to)) => (from, to),
                    (Err(err), _) | (_, Err(err)) => {
                        notifications.push(Notification::error(err));
                        response.set(Some(EventPage {
                            events: Vec::new(),
                            next_cursor: None,
                        }));
                        return;
                    }
                };
                if !query.all_runs && query.node_run_id.is_none() && current_run_id.is_none() {
                    return;
                }
                let node_run_id = if query.all_runs {
                    None
                } else {
                    query.node_run_id.clone().or_else(|| current_run_id.clone())
                };
                let deployment_id = if query.all_deployments {
                    None
                } else {
                    query
                        .deployment_id
                        .clone()
                        .or_else(|| current_deployment_id.as_ref().map(|id| id.id.clone()))
                };
                let levels = selected_levels(query);
                // Selecting every level is the same as not filtering, so ask for all of them at once.
                let requested_levels = if levels.len() == FILTER_LEVELS.len() {
                    vec![SystemEventLevel::Unspecified]
                } else {
                    levels
                };
                let query = query.clone();
                response.set(None);
                spawn_local(async move {
                    let mut events = Vec::new();
                    let mut has_more = false;
                    for level in requested_levels {
                        let mut params = vec![("limit", PAGE_SIZE.to_string())];
                        if level != SystemEventLevel::Unspecified {
                            params.push(("level", level_label(level).to_lowercase()));
                        }
                        if let Some(value) = query.code.as_ref().filter(|value| !value.is_empty()) {
                            params.push(("code", value.clone()));
                        }
                        if let Some(value) = deployment_id.as_ref() {
                            params.push(("deployment_id", value.clone()));
                        }
                        if let Some(value) = query.before.as_ref() {
                            params.push(("before", value.clone()));
                        }
                        if let Some(value) = node_run_id.as_ref() {
                            params.push(("node_run_id", value.clone()));
                        }
                        if let Some(value) = created_from.as_ref() {
                            params.push(("created_from", value.clone()));
                        }
                        if let Some(value) = created_to.as_ref() {
                            params.push(("created_to", value.clone()));
                        }
                        match rest::get::<EventPage>("/v1/admin/system-events", &params).await {
                            Ok(mut page) => {
                                has_more |= page.next_cursor.is_some();
                                events.append(&mut page.events);
                            }
                            Err(err) => {
                                error!("Failed to list system events: {err:?}");
                                notifications.push(Notification::error(format!(
                                    "Failed to list system events: {}",
                                    err
                                )));
                                return;
                            }
                        }
                    }
                    events.sort_unstable_by(|left, right| right.event_id.cmp(&left.event_id));
                    has_more |= events.len() > PAGE_SIZE as usize;
                    events.truncate(PAGE_SIZE as usize);
                    let next_cursor = has_more
                        .then(|| events.last().map(|event| event.event_id.clone()))
                        .flatten();
                    response.set(Some(EventPage {
                        events,
                        next_cursor,
                    }));
                });
            },
        );
    }

    let apply_more_filters = {
        let navigator = navigator.clone();
        let code_ref = code_ref.clone();
        let run_ref = run_ref.clone();
        let deployment_ref = deployment_ref.clone();
        let from_ref = from_ref.clone();
        let to_ref = to_ref.clone();
        let query = query.clone();
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();
            let mut query = query.clone();
            query.code = input_value(&code_ref);
            query.node_run_id = input_value(&run_ref);
            query.deployment_id = input_value(&deployment_ref);
            query.from = input_value(&from_ref);
            query.to = input_value(&to_ref);
            if query.node_run_id.is_some() {
                query.all_runs = false;
            }
            if query.deployment_id.is_some() {
                query.all_deployments = false;
            }
            query.before = None;
            query.id = None;
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    let find_by_id = {
        let navigator = navigator.clone();
        let id_ref = id_ref.clone();
        let query = query.clone();
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();
            let query = SystemEventQuery {
                id: input_value(&id_ref),
                absolute_time: query.absolute_time,
                ..SystemEventQuery::default()
            };
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    let set_run_scope = |all_runs: bool| {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |_| {
            let mut query = query.clone();
            query.all_runs = all_runs;
            query.node_run_id = None;
            query.before = None;
            query.id = None;
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    let set_deployment_scope = |all_deployments: bool| {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |_| {
            let mut query = query.clone();
            query.all_deployments = all_deployments;
            query.deployment_id = None;
            query.before = None;
            query.id = None;
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    let set_absolute_time = |absolute_time: bool| {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |_| {
            let mut query = query.clone();
            query.absolute_time = absolute_time;
            let _ = navigator.replace_with_query(&Route::SystemEvents, &query);
        })
    };

    let toggle_level = |level: SystemEventLevel, selected: bool| {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |_| {
            let mut query = query.clone();
            let mut levels = selected_levels(&query);
            levels.retain(|candidate| *candidate != level);
            if !selected {
                levels.push(level);
                levels.sort_unstable_by_key(|level| *level as i32);
            }
            query.levels = (levels != DEFAULT_LEVELS).then_some(LevelFilter(levels));
            query.before = None;
            query.id = None;
            let _ = navigator.push_with_query(&Route::SystemEvents, &query);
        })
    };

    let selected_levels = selected_levels(&query);
    let showing_all_deployments =
        query.all_deployments || (query.deployment_id.is_none() && current_deployment_id.is_none());
    let has_more_filters = query.code.is_some()
        || query.node_run_id.is_some()
        || query.deployment_id.is_some()
        || query.from.is_some()
        || query.to.is_some()
        || query.id.is_some();
    let time_range = match (&query.from, &query.to) {
        (None, None) => None,
        (from, to) => Some(format!(
            "{} to {} UTC",
            from.as_deref().unwrap_or("…"),
            to.as_deref().unwrap_or("now")
        )),
    };
    html! {
        <main>
            <h1>{"System events"}</h1>
            <div class="system-event-filters">
                <div class="system-event-filter-group">
                    {for FILTER_LEVELS.into_iter().map(|level| {
                        let selected = selected_levels.contains(&level);
                        html! {
                            <button class={classes!(selected.then_some("selected"))} onclick={toggle_level(level, selected)}>
                                {level_label(level)}
                            </button>
                        }
                    })}
                </div>
                <div class="system-event-filter-group">
                    <button class={classes!((!query.all_runs && query.node_run_id.is_none()).then_some("selected"))} onclick={set_run_scope(false)}>{"Current run"}</button>
                    <button class={classes!(query.all_runs.then_some("selected"))} onclick={set_run_scope(true)}>{"All runs"}</button>
                </div>
                <div class="system-event-filter-group">
                    <button class={classes!((!showing_all_deployments && query.deployment_id.is_none()).then_some("selected"))} onclick={set_deployment_scope(false)} disabled={current_deployment_id.is_none()}>{"Current deployment"}</button>
                    <button class={classes!(query.all_deployments.then_some("selected"))} onclick={set_deployment_scope(true)}>{"All deployments"}</button>
                </div>
                <div class="system-event-filter-group">
                    <button class={classes!((!query.absolute_time).then_some("selected"))} onclick={set_absolute_time(false)}>{"Relative time"}</button>
                    <button class={classes!(query.absolute_time.then_some("selected"))} onclick={set_absolute_time(true)}>{"UTC time"}</button>
                </div>
            </div>
            <details class="system-event-more-filters" open={has_more_filters}>
                <summary>{"More filters"}</summary>
                <form onsubmit={apply_more_filters}>
                    <input ref={code_ref} placeholder="Code, e.g. deployment.switch.completed" value={query.code.clone().unwrap_or_default()} />
                    <input ref={run_ref} class="system-event-ulid-input" placeholder="Node run, e.g. NodeRun_…" value={query.node_run_id.clone().unwrap_or_default()} />
                    <input ref={deployment_ref} class="system-event-ulid-input" placeholder="Deployment, e.g. Dep_…" value={query.deployment_id.clone().unwrap_or_default()} />
                    <div class="system-event-time-range">
                        <label class="system-event-time-bound" title="Inclusive">
                            {"From (UTC)"}
                            <input ref={from_ref} type="datetime-local" step="1" value={query.from.clone().unwrap_or_default()} />
                        </label>
                        <label class="system-event-time-bound" title="Exclusive">
                            {"To (UTC)"}
                            <input ref={to_ref} type="datetime-local" step="1" value={query.to.clone().unwrap_or_default()} />
                        </label>
                    </div>
                    <button type="submit">{"Apply"}</button>
                </form>
                <form onsubmit={find_by_id}>
                    <input ref={id_ref} class="system-event-ulid-input" placeholder="Event ID, e.g. Sysevt_…" value={query.id.clone().unwrap_or_default()} />
                    <button type="submit">{"Find"}</button>
                </form>
            </details>
            <p class="system-event-scope-summary">
                if let Some(id) = &query.id {
                    {format!("Event {id} · ")}
                    <Link<Route> to={Route::SystemEvents}>{"Show all events"}</Link<Route>>
                } else {
                    {if query.all_runs { "All node runs" } else if query.node_run_id.is_some() { "One node run" } else { "Current node run" }}
                    {" · "}
                    {if query.all_deployments { "All deployments" } else if query.deployment_id.is_some() { "One deployment" } else if showing_all_deployments { "All deployments" } else { "Current deployment" }}
                    if let Some(time_range) = time_range {
                        {" · "}{time_range}
                    }
                }
            </p>
            if let Some(response) = response.deref() {
                if response.events.is_empty() {
                    if query.id.is_some() {
                        <p>{"System event not found."}</p>
                    } else {
                        <p>{"No system events match these filters."}</p>
                    }
                } else {
                    <div class="system-event-list">{for response.events.iter().map(|event| event_card(event, query.absolute_time))}</div>
                }
                if query.id.is_none() {
                    <div class="pagination">
                        if query.before.is_some() {
                            <button onclick={{ let navigator = navigator.clone(); Callback::from(move |_| navigator.back()) }}>{"← Newer"}</button>
                        } else {
                            <button disabled={true}>{"← Newer"}</button>
                        }
                        if let Some(cursor) = &response.next_cursor {
                            <button onclick={{
                                let navigator = navigator.clone();
                                let older_query = SystemEventQuery {
                                    before: Some(cursor.clone()),
                                    ..query.clone()
                                };
                                Callback::from(move |_| {
                                    let _ = navigator.push_with_query(&Route::SystemEvents, &older_query);
                                })
                            }}>{"Older →"}</button>
                        } else {
                            <button disabled={true}>{"Older →"}</button>
                        }
                    </div>
                }
            } else {
                <p>{"Loading system events…"}</p>
            }
        </main>
    }
}
