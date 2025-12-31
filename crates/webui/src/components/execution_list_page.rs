use crate::{
    BASE_URL,
    app::{AppState, Route, query::Direction},
    components::{
        component_tree::{ComponentTree, ComponentTreeConfig},
        execution_status::ExecutionStatus,
    },
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{
            self, ExecutionId, ExecutionSummary,
            execution_repository_client::ExecutionRepositoryClient,
            list_executions_request::{NewerThan, OlderThan, Pagination, cursor},
        },
    },
    util::time::relative_time,
};
use chrono::{DateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{ops::Deref, str::FromStr};
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;
use yewprint::Icon;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ExecutionQuery {
    /// If true, shows child executions. (Maps to !top_level_only)
    #[serde(default)]
    pub show_derived: bool,
    #[serde(default)]
    pub hide_finished: bool,
    pub execution_id_prefix: Option<String>,
    pub ffqn_prefix: Option<String>,
    pub cursor: Option<ExecutionsCursor>,
    pub direction: Option<Direction>,
    #[serde(default)]
    pub include_cursor: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    derive_more::Display,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
)]
pub enum ExecutionsCursor {
    #[display("{_0}")]
    ExecutionId(ExecutionId),
    #[display("Created_{_0:?}")]
    CreatedAt(DateTime<Utc>),
}

impl FromStr for ExecutionsCursor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once("_") {
            Some(("E", rest)) => Ok(ExecutionsCursor::ExecutionId(ExecutionId {
                id: format!("E_{rest}"),
            })),
            Some(("Created", date)) => DateTime::from_str(date)
                .map(ExecutionsCursor::CreatedAt)
                .map_err(|err| err.to_string()),
            _ => Err("wrong prefix".to_string()),
        }
    }
}

impl ExecutionsCursor {
    fn as_type(&self) -> CursorType {
        match self {
            ExecutionsCursor::ExecutionId(_) => CursorType::ExecutionId,
            ExecutionsCursor::CreatedAt(_) => CursorType::CreatedAt,
        }
    }

    fn into_grpc_cursor(self) -> grpc_client::list_executions_request::Cursor {
        match self {
            ExecutionsCursor::ExecutionId(execution_id) => {
                grpc_client::list_executions_request::Cursor {
                    cursor: Some(cursor::Cursor::ExecutionId(execution_id)),
                }
            }
            ExecutionsCursor::CreatedAt(created_at) => {
                grpc_client::list_executions_request::Cursor {
                    cursor: Some(cursor::Cursor::CreatedAt(created_at.into())),
                }
            }
        }
    }

    fn from_summary(execution: &ExecutionSummary, cursor_type: CursorType) -> Self {
        match cursor_type {
            CursorType::CreatedAt => ExecutionsCursor::CreatedAt(DateTime::from(
                execution
                    .created_at
                    .expect("`created_at` is sent by the server"),
            )),
            CursorType::ExecutionId => ExecutionsCursor::ExecutionId(
                execution
                    .execution_id
                    .clone()
                    .expect("`execution_id` is sent by the server"),
            ),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum CursorType {
    #[default]
    CreatedAt,
    ExecutionId,
}

#[function_component(ExecutionListPage)]
pub fn execution_list_page() -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");

    let location = use_location().expect("should be called inside a router");
    let navigator = use_navigator().expect("should be called inside a router");

    // Deserialize query from URL or use default
    let query = location.query::<ExecutionQuery>().unwrap_or_default();

    // State to hold the API response
    let response_state = use_state(|| None);

    let prefix_ref = use_node_ref();
    let ffqn_ref = use_node_ref();

    // Effect: Fetch data when the URL query changes
    {
        let query = query.clone();
        let response_state = response_state.clone();

        use_effect_with(query, move |query_params| {
            let query_params = query_params.clone();
            spawn_local(async move {
                let mut execution_client =
                    ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));

                let page_size = 20;

                let cursor = query_params
                    .cursor
                    .as_ref()
                    .map(|c| c.clone().into_grpc_cursor());

                // Determine pagination based on direction
                let pagination = match query_params.direction.unwrap_or_default() {
                    Direction::Older => Some(Pagination::OlderThan(OlderThan {
                        cursor,
                        length: page_size,
                        including_cursor: query_params.include_cursor,
                    })),
                    Direction::Newer => Some(Pagination::NewerThan(NewerThan {
                        cursor,
                        length: page_size,
                        including_cursor: query_params.include_cursor,
                    })),
                };
                let req = grpc_client::ListExecutionsRequest {
                    function_name_prefix: query_params.ffqn_prefix,
                    top_level_only: !query_params.show_derived,
                    pagination,
                    hide_finished: query_params.hide_finished,
                    execution_id_prefix: query_params.execution_id_prefix.filter(|s| !s.is_empty()),
                };
                debug!("Fetching executions with query: {req:?}");
                let response = execution_client.list_executions(req).await;

                match response {
                    Ok(resp) => response_state.set(Some(resp.into_inner())),
                    Err(e) => log::error!("Failed to list executions: {:?}", e),
                }
            })
        });
    }

    // Callbacks for filter updates
    let on_apply_filters = {
        let navigator = navigator.clone();
        let query = query.clone();
        let prefix_ref = prefix_ref.clone();
        let ffqn_ref = ffqn_ref.clone();
        Callback::from(move |_| {
            let mut new_query = query.clone();
            // Reset cursor when changing filters to start from top
            new_query.cursor = None;
            new_query.direction = None;
            new_query.include_cursor = false;
            let ffqn = ffqn_ref.cast::<HtmlInputElement>().unwrap().value();
            new_query.ffqn_prefix = (!ffqn.is_empty()).then(|| ffqn);
            let prefix = prefix_ref.cast::<HtmlInputElement>().unwrap().value();
            new_query.execution_id_prefix = (!prefix.is_empty()).then(|| prefix);
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };

    let on_toggle_derived = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            let mut new_query = query.clone();
            new_query.show_derived = input.checked();
            // We usually want to reset pagination when changing view modes
            new_query.cursor = None;
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };

    let on_toggle_hide_finished = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            let mut new_query = query.clone();
            new_query.hide_finished = input.checked();
            new_query.cursor = None;
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };

    // Render logic
    if let Some(response) = response_state.deref() {
        let rows = response.executions.iter().map(|execution| {
            let ffqn = FunctionFqn::from(
                execution.function_name.clone().expect("function_name missing"),
            );
            let status = Some(
                execution.current_status.clone().expect("current_status missing")
                    .status.expect("status detail missing")
            );
            let execution_id = execution.execution_id.clone().expect("execution_id missing");

            let play = if app_state.ffqns_to_details.contains_key(&ffqn) {
                html!{
                    <Link<Route> to={Route::ExecutionSubmit { ffqn: ffqn.clone() } }>
                        <Icon icon = { Icon::Play }/>
                    </Link<Route>>
                }
            } else {
                ".".to_html()
            };

            let first_scheduled_at: DateTime<Utc> = execution.first_scheduled_at.expect("`first_scheduled_at` is sent").into();
            let now = Utc::now();
            html! {
                <tr key={execution_id.id.clone()}>
                    <td>
                        // Execution id column
                        <Link<Route> to={Route::ExecutionTrace { execution_id: execution_id.clone() }}>
                            {&execution_id}
                        </Link<Route>>
                    </td>
                    <td>
                        // FFQN column
                        <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={
                            let mut q = query.clone();
                            q.ffqn_prefix = Some(ffqn.ifc_fqn.to_string());
                            q.cursor = None;
                            q
                        }>
                            { ffqn.ifc_fqn.to_string() }
                        </Link<Route, ExecutionQuery>>

                        {play}

                        <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={
                            let mut q = query.clone();
                            q.ffqn_prefix = Some(ffqn.to_string());
                            q.cursor = None;
                            q
                        }>
                            { &ffqn.function_name }
                        </Link<Route, ExecutionQuery>>
                    </td>
                    <td>
                        // Status column
                        <ExecutionStatus {status} {execution_id} print_finished_status={false} />
                    </td>
                    <td>
                        <label title={first_scheduled_at.to_string()}>
                            {relative_time(first_scheduled_at, now)}{" ago"}
                        </label>
                    </td>
                </tr>
            }
        }).collect::<Vec<_>>();

        // Calculate cursors for pagination
        let cursor_type = query
            .cursor
            .as_ref()
            .map(|cursor| cursor.as_type())
            .unwrap_or_default();

        let first_item_cursor = response
            .executions
            .first()
            .map(|e| ExecutionsCursor::from_summary(e, cursor_type));
        let last_item_cursor = response
            .executions
            .last()
            .map(|e| ExecutionsCursor::from_summary(e, cursor_type));

        // Pagination Query Generators
        let prev_page_query = |cursor| {
            let mut q = query.clone();
            q.cursor = Some(cursor);
            q.direction = Some(Direction::Newer); // "Previous" means newer items in a log list
            q.include_cursor = false;
            q
        };

        let next_page_query = |cursor| {
            let mut q = query.clone();
            q.cursor = Some(cursor);
            q.direction = Some(Direction::Older); // "Next" means older items
            q.include_cursor = false;
            q
        };

        html! {
            <>
                <h3>{"Executions"}</h3>

                <div class="filters" style="margin-bottom: 1em; padding: 1em; border: 1px solid #ccc;">
                    <div style="margin-bottom: 0.5em;">
                        <label style="margin-right: 1em;">
                            <input
                                type="checkbox"
                                checked={query.show_derived}
                                onchange={on_toggle_derived}
                            />
                            {" Show Derived Executions"}
                        </label>
                        <label>
                            <input
                                type="checkbox"
                                checked={query.hide_finished}
                                onchange={on_toggle_hide_finished}
                            />
                            {" Hide Finished"}
                        </label>
                    </div>
                    <div>
                        <input
                            type="text"
                            ref={prefix_ref.clone()}
                            placeholder="Execution ID Prefix..."
                            value={(query.execution_id_prefix).clone()}
                        />
                        {" "}
                        <input
                            type="text"
                            ref={ffqn_ref.clone()}
                            placeholder="Function Name Prefix..."
                            value={query.ffqn_prefix.as_ref().map(|ffqn| ffqn.to_string())}
                        />
                        {" "}
                        <button onclick={on_apply_filters}>{"Filter"}</button>

                        if query != ExecutionQuery::default() {
                            {" "}
                            <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={Some(ExecutionQuery::default())}>
                                {"Clear Filters"}
                            </Link<Route, ExecutionQuery>>
                        }
                    </div>
                </div>

                <ComponentTree config={ComponentTreeConfig::ExecutionListFiltering} />

                <table class="execution_list">
                    <tr><th>{"Execution ID"}</th><th>{"Function"}</th><th>{"Status"}</th><th>{"First Scheduled At"}</th></tr>
                    { rows }
                </table>


                <div class="pagination">
                    <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={
                        let mut q = query.clone();
                        q.cursor = None;
                        q.direction = None;
                        q
                    }>
                        {"<< Latest"}
                    </Link<Route, ExecutionQuery>>

                    {" | "}

                    if let Some(cursor) = first_item_cursor {
                        <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={prev_page_query(cursor)}>
                            {"< Previous (Newer)"}
                        </Link<Route, ExecutionQuery>>
                    } else {
                        <span class="disabled">{"< Previous"}</span>
                    }

                    {" | "}

                    if let Some(cursor) = last_item_cursor {
                        <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={next_page_query(cursor)}>
                            {"Next (Older) >"}
                        </Link<Route, ExecutionQuery>>
                    } else {
                        <span class="disabled">{"Next >"}</span>
                    }
                </div>

            </>
        }
    } else {
        html! { <p>{"Loading..."}</p> }
    }
}
