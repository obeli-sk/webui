use crate::tree::Icon;
use crate::{
    BASE_URL,
    app::{AppState, Route},
    components::{
        execution_status::{ExecutionStatus, StatusCacheContext, StatusState},
        notification::{Notification, NotificationContext},
    },
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{
            self, ExecutionId, ExecutionSummary,
            execution_repository_client::ExecutionRepositoryClient,
            list_executions_request::{NewerThan, OlderThan, Pagination, cursor},
        },
    },
    util::time::{TimeGranularity, human_formatted_timedelta, relative_time},
};
use chrono::{DateTime, Utc};
use hashbrown::{HashMap, HashSet};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Deref, str::FromStr};
use tonic_web_wasm_client::Client;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ExecutionQuery {
    /// If true, shows child executions. (Maps to !top_level_only)
    #[serde(default)]
    pub show_derived: bool,
    #[serde(default)]
    pub hide_finished: bool,
    #[serde(default)]
    pub show_details: bool,
    pub execution_id_prefix: Option<String>,
    pub ffqn_prefix: Option<String>,
    pub component_digest: Option<String>,
    pub deployment_id: Option<String>,
    #[serde(default)]
    pub status: Option<StatusFilterList>,
    pub cursor: Option<ExecutionsCursor>,
    pub direction: Option<Direction>,
    #[serde(default)]
    pub include_cursor: bool,
}

/// Execution state filter, using the same buckets as the deployment summary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatusFilter {
    Locked,
    Pending,
    Scheduled,
    Blocked,
    Paused,
    Finished,
    FinishedOk,
    FinishedError,
    FinishedExecutionFailure,
}

/// Comma-separated list of [`StatusFilter`]s, matched with OR semantics.
/// Encoded in the URL as e.g. `status=locked,pending,blocked`.
#[derive(Clone, Debug, PartialEq, serde_with::SerializeDisplay, serde_with::DeserializeFromStr)]
pub struct StatusFilterList(pub Vec<StatusFilter>);

impl StatusFilterList {
    /// Locked, pending or blocked: executions that will progress on their own.
    pub fn in_progress() -> StatusFilterList {
        StatusFilterList(vec![
            StatusFilter::Locked,
            StatusFilter::Pending,
            StatusFilter::Blocked,
        ])
    }

    pub fn single(status: StatusFilter) -> StatusFilterList {
        StatusFilterList(vec![status])
    }
}

impl std::fmt::Display for StatusFilterList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for status in &self.0 {
            if !first {
                write!(f, ",")?;
            }
            first = false;
            write!(f, "{}", status.as_str())?;
        }
        Ok(())
    }
}

impl FromStr for StatusFilterList {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let filters = s
            .split(',')
            .map(StatusFilter::from_value)
            .collect::<Option<Vec<_>>>()
            .filter(|filters| !filters.is_empty())
            .ok_or_else(|| format!("invalid status filter list: `{s}`"))?;
        Ok(StatusFilterList(filters))
    }
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 9] = [
        StatusFilter::Locked,
        StatusFilter::Pending,
        StatusFilter::Scheduled,
        StatusFilter::Blocked,
        StatusFilter::Paused,
        StatusFilter::Finished,
        StatusFilter::FinishedOk,
        StatusFilter::FinishedError,
        StatusFilter::FinishedExecutionFailure,
    ];

    fn as_str(self) -> &'static str {
        match self {
            StatusFilter::Locked => "locked",
            StatusFilter::Pending => "pending",
            StatusFilter::Scheduled => "scheduled",
            StatusFilter::Blocked => "blocked",
            StatusFilter::Paused => "paused",
            StatusFilter::Finished => "finished",
            StatusFilter::FinishedOk => "finished_ok",
            StatusFilter::FinishedError => "finished_error",
            StatusFilter::FinishedExecutionFailure => "finished_execution_failure",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::Locked => "Locked",
            StatusFilter::Pending => "Pending",
            StatusFilter::Scheduled => "Scheduled",
            StatusFilter::Blocked => "Blocked",
            StatusFilter::Paused => "Paused",
            StatusFilter::Finished => "Finished (any)",
            StatusFilter::FinishedOk => "Finished OK",
            StatusFilter::FinishedError => "Finished with error",
            StatusFilter::FinishedExecutionFailure => "Execution failed",
        }
    }

    fn from_value(value: &str) -> Option<StatusFilter> {
        StatusFilter::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
    }

    fn to_grpc(self) -> grpc_client::list_executions_request::ExecutionStateFilter {
        use grpc_client::list_executions_request::ExecutionStateFilter;
        match self {
            StatusFilter::Locked => ExecutionStateFilter::Locked,
            StatusFilter::Pending => ExecutionStateFilter::Pending,
            StatusFilter::Scheduled => ExecutionStateFilter::Scheduled,
            StatusFilter::Blocked => ExecutionStateFilter::Blocked,
            StatusFilter::Paused => ExecutionStateFilter::Paused,
            StatusFilter::Finished => ExecutionStateFilter::Finished,
            StatusFilter::FinishedOk => ExecutionStateFilter::FinishedOk,
            StatusFilter::FinishedError => ExecutionStateFilter::FinishedError,
            StatusFilter::FinishedExecutionFailure => {
                ExecutionStateFilter::FinishedExecutionFailure
            }
        }
    }
}
impl ExecutionQuery {
    fn flip(mut self, old_direction: Direction) -> ExecutionQuery {
        self.direction = Some(old_direction.flip());
        self.include_cursor = !self.include_cursor;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Direction {
    #[default]
    Older,
    Newer,
}
impl Direction {
    fn flip(&self) -> Direction {
        match self {
            Direction::Older => Direction::Newer,
            Direction::Newer => Direction::Older,
        }
    }
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

fn grpc_execution_function_filter(
    value: String,
) -> grpc_client::list_executions_request::ExecutionFunctionFilter {
    let scope = if value
        .rsplit_once('.')
        .is_some_and(|(left, _)| left.contains('/'))
    {
        grpc_client::list_executions_request::execution_function_filter::Scope::FunctionName(value)
    } else if value.contains('/') {
        grpc_client::list_executions_request::execution_function_filter::Scope::InterfaceName(value)
    } else {
        grpc_client::list_executions_request::execution_function_filter::Scope::PackageName(value)
    };

    grpc_client::list_executions_request::ExecutionFunctionFilter { scope: Some(scope) }
}

#[derive(Clone, PartialEq)]
struct FunctionPrefixPickerData {
    packages: Vec<String>,
    interfaces_by_package: BTreeMap<String, Vec<String>>,
    functions_by_interface: BTreeMap<String, Vec<String>>,
}

fn build_function_prefix_picker_data(app_state: &AppState) -> FunctionPrefixPickerData {
    let mut packages = HashSet::new();
    let mut interfaces_by_package: HashMap<String, HashSet<String>> = HashMap::new();
    let mut functions_by_interface: HashMap<String, HashSet<String>> = HashMap::new();

    for ffqn in app_state.ffqns_to_details.keys() {
        if ffqn.ifc_fqn.pkg_fqn.is_extension() {
            continue;
        }

        let package = ffqn.ifc_fqn.pkg_fqn.to_string();
        let interface = ffqn.ifc_fqn.to_string();
        let function = ffqn.to_string();

        packages.insert(package.clone());
        interfaces_by_package
            .entry(package)
            .or_default()
            .insert(interface.clone());
        functions_by_interface
            .entry(interface)
            .or_default()
            .insert(function);
    }

    let mut packages = packages.into_iter().collect::<Vec<_>>();
    packages.sort();

    let interfaces_by_package = interfaces_by_package
        .into_iter()
        .map(|(package, interfaces)| {
            let mut interfaces = interfaces.into_iter().collect::<Vec<_>>();
            interfaces.sort();
            (package, interfaces)
        })
        .collect::<BTreeMap<_, _>>();

    let functions_by_interface = functions_by_interface
        .into_iter()
        .map(|(interface, functions)| {
            let mut functions = functions.into_iter().collect::<Vec<_>>();
            functions.sort();
            (interface, functions)
        })
        .collect::<BTreeMap<_, _>>();

    FunctionPrefixPickerData {
        packages,
        interfaces_by_package,
        functions_by_interface,
    }
}

#[derive(Properties, PartialEq)]
struct FunctionPrefixInputProps {
    pub value: String,
    pub on_apply: Callback<Option<String>>,
}

#[function_component(FunctionPrefixInput)]
fn function_prefix_input(props: &FunctionPrefixInputProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let picker_data = build_function_prefix_picker_data(&app_state);

    let is_modal_open = use_state(|| false);
    let draft_value = use_state(|| props.value.clone());
    let selected_package = use_state(|| None::<String>);
    let selected_interface = use_state(|| None::<String>);

    {
        let draft_value = draft_value.clone();
        let selected_package = selected_package.clone();
        let selected_interface = selected_interface.clone();
        let value = props.value.clone();
        let picker_data = picker_data.clone();
        use_effect_with(props.value.clone(), move |_| {
            draft_value.set(value.clone());

            let package_match = picker_data
                .packages
                .iter()
                .find(|package| *package == &value)
                .cloned();
            let interface_match =
                picker_data
                    .interfaces_by_package
                    .iter()
                    .find_map(|(package, interfaces)| {
                        interfaces
                            .iter()
                            .find(|interface| *interface == &value)
                            .map(|interface| (package.clone(), interface.clone()))
                    });
            let function_match =
                picker_data
                    .functions_by_interface
                    .iter()
                    .find_map(|(interface, functions)| {
                        functions
                            .iter()
                            .find(|function| *function == &value)
                            .map(|_| interface.clone())
                    });

            if let Some(package) = package_match {
                selected_package.set(Some(package));
                selected_interface.set(None);
            } else if let Some((package, interface)) = interface_match {
                selected_package.set(Some(package));
                selected_interface.set(Some(interface));
            } else if let Some(interface) = function_match {
                let package =
                    picker_data
                        .interfaces_by_package
                        .iter()
                        .find_map(|(package, interfaces)| {
                            interfaces
                                .iter()
                                .find(|candidate| *candidate == &interface)
                                .map(|_| package.clone())
                        });
                selected_package.set(package);
                selected_interface.set(Some(interface));
            } else {
                selected_package.set(None);
                selected_interface.set(None);
            }

            || ()
        });
    }

    let open_modal_on_focus = {
        let is_modal_open = is_modal_open.clone();
        Callback::from(move |_: FocusEvent| is_modal_open.set(true))
    };
    let open_modal_on_click = {
        let is_modal_open = is_modal_open.clone();
        Callback::from(move |_: MouseEvent| is_modal_open.set(true))
    };
    let close_modal = {
        let is_modal_open = is_modal_open.clone();
        Callback::from(move |_| is_modal_open.set(false))
    };
    let close_modal_on_click = {
        let close_modal = close_modal.clone();
        Callback::from(move |_: MouseEvent| close_modal.emit(()))
    };
    let on_draft_input = {
        let draft_value = draft_value.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            draft_value.set(input.value());
        })
    };
    let on_clear = {
        let draft_value = draft_value.clone();
        let selected_package = selected_package.clone();
        let selected_interface = selected_interface.clone();
        let on_apply = props.on_apply.clone();
        let is_modal_open = is_modal_open.clone();
        Callback::from(move |_| {
            draft_value.set(String::new());
            selected_package.set(None);
            selected_interface.set(None);
            on_apply.emit(None);
            is_modal_open.set(false);
        })
    };
    let on_apply = {
        let draft_value = draft_value.clone();
        let on_apply = props.on_apply.clone();
        let is_modal_open = is_modal_open.clone();
        Callback::from(move |_| {
            let value = (*draft_value).clone();
            on_apply.emit((!value.is_empty()).then_some(value));
            is_modal_open.set(false);
        })
    };

    {
        let close_modal = close_modal.clone();
        let is_modal_open = *is_modal_open;
        use_effect_with(is_modal_open, move |is_open| {
            let listener = if *is_open {
                let closure = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(
                    move |e: web_sys::KeyboardEvent| {
                        if e.key() == "Escape" {
                            close_modal.emit(());
                        }
                    },
                );
                let window = web_sys::window().expect("window should exist");
                window
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                    .expect("failed to add keydown listener");
                Some((window, closure))
            } else {
                None
            };

            move || {
                if let Some((window, closure)) = listener {
                    window
                        .remove_event_listener_with_callback(
                            "keydown",
                            closure.as_ref().unchecked_ref(),
                        )
                        .expect("failed to remove keydown listener");
                }
            }
        });
    }
    let selected_package_value = (*selected_package).clone();
    let selected_interface_value = (*selected_interface).clone();
    let interfaces = selected_package_value
        .as_ref()
        .and_then(|package| picker_data.interfaces_by_package.get(package))
        .cloned()
        .unwrap_or_default();
    let functions = selected_interface_value
        .as_ref()
        .and_then(|interface| picker_data.functions_by_interface.get(interface))
        .cloned()
        .unwrap_or_default();

    html! {
        <>
            <input
                type="text"
                class="function-prefix-trigger"
                placeholder="Package, Interface, or Function..."
                readonly=true
                value={props.value.clone()}
                onfocus={open_modal_on_focus}
                onclick={open_modal_on_click}
            />

            if *is_modal_open {
                <div class="modal-overlay" onclick={close_modal_on_click.clone()}>
                    <div
                        class="modal-window function-prefix-modal-window"
                        onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
                    >
                        <div class="modal-header">
                            <h3>{"Function Filter"}</h3>
                            <button class="modal-dismiss" type="button" onclick={close_modal_on_click}>
                                {"×"}
                            </button>
                        </div>

                        <div class="function-prefix-modal-body">
                            <div class="function-prefix-modal-manual">
                                <label for="function-prefix-manual">{"Manual filter"}</label>
                                <input
                                    id="function-prefix-manual"
                                    type="text"
                                    value={(*draft_value).clone()}
                                    oninput={on_draft_input}
                                    placeholder="namespace:package[@version], namespace:package/interface[@version], or namespace:package/interface[@version].function"
                                />
                            </div>

                            <div class="function-prefix-modal-columns">
                                <div class="function-prefix-modal-column">
                                    <div class="function-prefix-modal-column-title">{"Packages"}</div>
                                    <div class="function-prefix-modal-list">
                                        {for picker_data.packages.iter().map(|package| {
                                            let package_value = package.clone();
                                            let selected_package = selected_package.clone();
                                            let selected_interface = selected_interface.clone();
                                            let draft_value = draft_value.clone();
                                            let is_selected = selected_package_value.as_ref() == Some(package)
                                                && selected_interface_value.is_none()
                                                && *draft_value == *package;
                                            let onclick = Callback::from(move |_| {
                                                selected_package.set(Some(package_value.clone()));
                                                selected_interface.set(None);
                                                draft_value.set(package_value.clone());
                                            });
                                            html! {
                                                <button
                                                    type="button"
                                                    class={classes!("function-prefix-option", is_selected.then_some("selected"))}
                                                    {onclick}
                                                >
                                                    {package}
                                                </button>
                                            }
                                        })}
                                    </div>
                                </div>

                                <div class="function-prefix-modal-column">
                                    <div class="function-prefix-modal-column-title">{"Interfaces"}</div>
                                    <div class="function-prefix-modal-list">
                                        {for interfaces.iter().map(|interface| {
                                            let interface_value = interface.clone();
                                            let selected_interface = selected_interface.clone();
                                            let draft_value = draft_value.clone();
                                            let is_selected = selected_interface_value.as_ref() == Some(interface)
                                                && *draft_value == *interface;
                                            let onclick = Callback::from(move |_| {
                                                selected_interface.set(Some(interface_value.clone()));
                                                draft_value.set(interface_value.clone());
                                            });
                                            html! {
                                                <button
                                                    type="button"
                                                    class={classes!("function-prefix-option", is_selected.then_some("selected"))}
                                                    {onclick}
                                                >
                                                    {interface}
                                                </button>
                                            }
                                        })}
                                    </div>
                                </div>

                                <div class="function-prefix-modal-column">
                                    <div class="function-prefix-modal-column-title">{"Functions"}</div>
                                    <div class="function-prefix-modal-list">
                                        {for functions.iter().map(|function| {
                                            let function_value = function.clone();
                                            let draft_value = draft_value.clone();
                                            let function_name = FunctionFqn::from_str(function)
                                                .map(|ffqn| ffqn.function_name)
                                                .unwrap_or_else(|_| function.clone());
                                            let is_selected = *draft_value == *function;
                                            let onclick = Callback::from(move |_| {
                                                draft_value.set(function_value.clone());
                                            });
                                            html! {
                                                <button
                                                    type="button"
                                                    class={classes!("function-prefix-option", is_selected.then_some("selected"))}
                                                    {onclick}
                                                >
                                                    {function_name}
                                                </button>
                                            }
                                        })}
                                    </div>
                                </div>
                            </div>
                        </div>

                        <div class="modal-footer">
                            <button type="button" onclick={on_clear}>{"Clear"}</button>
                            <button type="button" onclick={on_apply}>{"Apply Filter"}</button>
                        </div>
                    </div>
                </div>
            }
        </>
    }
}

#[component(ExecutionListPage)]
pub fn execution_list_page() -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    let location = use_location().expect("should be called inside a router");
    let navigator = use_navigator().expect("should be called inside a router");

    // Deserialize query from URL or use default
    let query = location.query::<ExecutionQuery>().unwrap_or_default();

    // State to hold the API response
    let response_state = use_state(|| None);

    let refresh_counter_state = use_state(|| 0); // Force calling use_effect

    // Cache status to persist across pagination/refresh
    let status_cache = use_reducer_eq(StatusState::default);

    let prefix_ref = use_node_ref();
    let deployment_id_ref = use_node_ref();
    let component_digest_ref = use_node_ref();
    let ffqn_prefix_state = use_state(|| query.ffqn_prefix.clone().unwrap_or_default());

    // Effect: Fetch data when the URL query changes
    {
        let query = query.clone();
        let response_state = response_state.clone();
        let prefix_ref = prefix_ref.clone();
        let deployment_id_ref = deployment_id_ref.clone();
        let component_digest_ref = component_digest_ref.clone();
        let refresh_counter_state = refresh_counter_state.clone();
        let notifications = notifications.clone();
        let ffqn_prefix_state = ffqn_prefix_state.clone();

        use_effect_with((query, *refresh_counter_state), move |(query_params, _)| {
            let query_params = query_params.clone();

            spawn_local(async move {
                // Attempt to sync text values from the actual filter into text boxes.
                if let Some(input) = prefix_ref.cast::<HtmlInputElement>() {
                    input.set_value(
                        query_params
                            .execution_id_prefix
                            .as_deref()
                            .unwrap_or_default(),
                    )
                }
                if let Some(input) = deployment_id_ref.cast::<HtmlInputElement>() {
                    input.set_value(query_params.deployment_id.as_deref().unwrap_or_default())
                }
                if let Some(input) = component_digest_ref.cast::<HtmlInputElement>() {
                    input.set_value(query_params.component_digest.as_deref().unwrap_or_default())
                }
                ffqn_prefix_state.set(query_params.ffqn_prefix.clone().unwrap_or_default());

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

                // Send request
                #[allow(deprecated)]
                let req = grpc_client::ListExecutionsRequest {
                    function_name_prefix: None,
                    top_level_only: !query_params.show_derived,
                    pagination,
                    hide_finished: query_params.hide_finished,
                    function_filter: query_params.ffqn_prefix.map(grpc_execution_function_filter),
                    execution_id_prefix: query_params.execution_id_prefix.filter(|s| !s.is_empty()),
                    component_digest: query_params
                        .component_digest
                        .filter(|s| !s.is_empty())
                        .map(grpc_client::ContentDigest::from),
                    deployment_id: query_params
                        .deployment_id
                        .filter(|s| !s.is_empty())
                        .map(grpc_client::DeploymentId::from),
                    state_filters: query_params
                        .status
                        .iter()
                        .flat_map(|list| list.0.iter())
                        .map(|status| status.to_grpc() as i32)
                        .collect(),
                };
                debug!("Fetching executions with query: {req:?}");
                let response = execution_client.list_executions(req).await;

                match response {
                    Ok(resp) => response_state.set(Some(resp.into_inner())),
                    Err(e) => {
                        error!("Failed to list executions: {:?}", e);
                        notifications.push(Notification::error(format!(
                            "Failed to list executions: {}",
                            e.message()
                        )));
                    }
                }
            })
        });
    }

    // Clicked on "Filter / Refresh"
    let on_apply_filters = {
        let navigator = navigator.clone();
        let query = query.clone();
        let prefix_ref = prefix_ref.clone();
        let deployment_id_ref = deployment_id_ref.clone();
        let component_digest_ref = component_digest_ref.clone();
        let refresh_counter_state = refresh_counter_state.clone();
        let ffqn_prefix_state = ffqn_prefix_state.clone();
        Callback::from(move |_| {
            let mut new_query = query.clone();
            // Reset cursor when changing filters to start from top
            new_query.cursor = None;
            new_query.direction = None;
            new_query.include_cursor = false;

            let ffqn = (*ffqn_prefix_state).clone();
            new_query.ffqn_prefix = (!ffqn.is_empty()).then_some(ffqn);

            let prefix = prefix_ref.cast::<HtmlInputElement>().unwrap().value();
            new_query.execution_id_prefix = (!prefix.is_empty()).then_some(prefix);

            let deployment_id = deployment_id_ref
                .cast::<HtmlInputElement>()
                .unwrap()
                .value();
            new_query.deployment_id = (!deployment_id.is_empty()).then_some(deployment_id);

            let component_digest = component_digest_ref
                .cast::<HtmlInputElement>()
                .unwrap()
                .value();
            new_query.component_digest = (!component_digest.is_empty()).then_some(component_digest);

            refresh_counter_state.set(*refresh_counter_state + 1);
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
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };
    let on_toggle_show_details = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            let mut new_query = query.clone();
            new_query.show_details = input.checked();
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };
    let on_status_change = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let mut new_query = query.clone();
            new_query.status = StatusFilterList::from_str(&select.value()).ok();
            // Reset cursor when changing filters to start from top
            new_query.cursor = None;
            new_query.direction = None;
            new_query.include_cursor = false;
            let _ = navigator.push_with_query(&Route::ExecutionList, &new_query);
        })
    };
    let on_apply_ffqn_prefix = {
        let navigator = navigator.clone();
        let query = query.clone();
        let prefix_ref = prefix_ref.clone();
        let deployment_id_ref = deployment_id_ref.clone();
        let component_digest_ref = component_digest_ref.clone();
        let refresh_counter_state = refresh_counter_state.clone();
        let ffqn_prefix_state = ffqn_prefix_state.clone();
        Callback::from(move |ffqn_prefix: Option<String>| {
            ffqn_prefix_state.set(ffqn_prefix.clone().unwrap_or_default());

            let mut new_query = query.clone();
            new_query.cursor = None;
            new_query.direction = None;
            new_query.include_cursor = false;
            new_query.ffqn_prefix = ffqn_prefix;

            let prefix = prefix_ref.cast::<HtmlInputElement>().unwrap().value();
            new_query.execution_id_prefix = (!prefix.is_empty()).then_some(prefix);

            let deployment_id = deployment_id_ref
                .cast::<HtmlInputElement>()
                .unwrap()
                .value();
            new_query.deployment_id = (!deployment_id.is_empty()).then_some(deployment_id);

            let component_digest = component_digest_ref
                .cast::<HtmlInputElement>()
                .unwrap()
                .value();
            new_query.component_digest = (!component_digest.is_empty()).then_some(component_digest);

            refresh_counter_state.set(*refresh_counter_state + 1);
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
            let deployment_id = execution.deployment_id.clone().expect("deployment_id missing").id;
            let component_digest = execution.component_digest.as_ref().expect("component_digest missing").digest.as_str();

            let play = if app_state.ffqns_to_details.contains_key(&ffqn) {
                html!{
                    <Link<Route> to={Route::ExecutionSubmit { ffqn: ffqn.clone() } }>
                        { Html::from(Icon::Play) }
                    </Link<Route>>
                }
            } else {
                Html::from(".")
            };

            let created_at: DateTime<Utc> = execution.created_at.expect("`created_at` is sent").into();
            let durated = if let Some( grpc_client::ExecutionStatus{ status: Some(status),..}) = &execution.current_status
                && let grpc_client::execution_status::Status::Finished(finished) = status

             {
                Some(DateTime::from(finished.finished_at.unwrap()) - DateTime::from(execution.first_scheduled_at.unwrap()))
            } else {
                None
            };
            let now = Utc::now();
            html! {
                <tr key={execution_id.id.clone()}>
                    <td>
                        // Execution id column
                        <Link<Route> to={Route::ExecutionTrace { execution_id: execution_id.clone() }}>
                            {execution_id.to_string()}
                        </Link<Route>>
                        if query.show_details {
                            <div title="Deployment ID">
                                { deployment_id }
                            </div>
                            <div title="Component Digest">
                                { component_digest }
                            </div>
                        }
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
                        <ExecutionStatus {status} {execution_id} />
                    </td>
                    <td>
                        // Created At column
                        <div title={created_at.to_string()}>
                            {"Created "}
                            if query.show_details {
                                { created_at }
                            } else {
                                {relative_time(created_at, now, TimeGranularity::Coarse)}{" ago"}
                            }
                        </div>
                        // Duration
                        if let Some(durated) = durated {
                            <div title={durated.to_string()}>
                                {"Took "}
                                {human_formatted_timedelta(durated, TimeGranularity::Fine)}
                            </div>
                        }
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

        let newer_page_query = if let Some(exe) = response.executions.first() {
            let mut query = query.clone();
            query.cursor = Some(ExecutionsCursor::from_summary(exe, cursor_type));
            query.direction = Some(Direction::Newer);
            query.include_cursor = false;
            Some(query)
        } else if let Some(direction) = query.direction
            && direction == Direction::Older
            && !query.include_cursor
            && query.cursor.is_some()
        {
            Some(query.clone().flip(direction))
        } else {
            None
        };
        let older_page_query = if let Some(exe) = response.executions.last() {
            let mut query = query.clone();
            query.cursor = Some(ExecutionsCursor::from_summary(exe, cursor_type));
            query.direction = Some(Direction::Older);
            query.include_cursor = false;
            Some(query)
        } else if let Some(direction) = query.direction
            && direction == Direction::Newer
            && !query.include_cursor
            && query.cursor.is_some()
        {
            Some(query.clone().flip(direction))
        } else {
            None
        };
        let on_page_change = {
            let navigator = navigator.clone();
            Callback::from(move |query: ExecutionQuery| {
                let _ = navigator.push_with_query(&Route::ExecutionList, &query);
            })
        };
        html! {
            <ContextProvider<StatusCacheContext> context={status_cache}>
                <h3>{"Executions"}</h3>

                <div class="executions-filter">
                    <div class="checkboxes">
                        <label>
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
                        <label>
                            <input
                                type="checkbox"
                                checked={query.show_details}
                                onchange={on_toggle_show_details}
                            />
                            {" Show Details"}
                        </label>
                    </div>
                    <div class="inputs">
                        <input
                            type="text"
                            ref={prefix_ref.clone()}
                            placeholder="Execution ID Prefix..."
                            value={(query.execution_id_prefix).clone()}
                        />
                        <FunctionPrefixInput
                            value={(*ffqn_prefix_state).clone()}
                            on_apply={on_apply_ffqn_prefix}
                        />
                        <input
                            type="text"
                            ref={deployment_id_ref.clone()}
                            placeholder="Deployment ID..."
                            value={(query.deployment_id).clone()}
                        />
                        <input
                            type="text"
                            ref={component_digest_ref.clone()}
                            placeholder="Component Digest..."
                            value={(query.component_digest).clone()}
                        />
                        <select onchange={on_status_change} title="Filter by execution status">
                            {{
                                let current = query.status.as_ref().map(ToString::to_string).unwrap_or_default();
                                let in_progress = StatusFilterList::in_progress().to_string();
                                html! {<>
                                    <option value="" selected={current.is_empty()}>{"Any status"}</option>
                                    <option
                                        value={in_progress.clone()}
                                        selected={current == in_progress}
                                        title="Locked, pending or blocked"
                                    >
                                        {"In progress"}
                                    </option>
                                    { for StatusFilter::ALL.into_iter().map(|status| html! {
                                        <option
                                            value={status.as_str()}
                                            selected={current == status.as_str()}
                                        >
                                            {status.label()}
                                        </option>
                                    })}
                                </>}
                            }}
                        </select>

                        <button onclick={&on_apply_filters}>{"Filter / Refresh"}</button>

                        if query != ExecutionQuery::default() {
                            <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={Some(ExecutionQuery::default())}>
                                {"Clear Filters"}
                            </Link<Route, ExecutionQuery>>
                        }
                    </div>
                </div>

                <table class="execution_list">
                    <tr>
                        <th>{"Execution ID"}</th>
                        <th>{"Function"}</th>
                        <th>{"Status"}</th>
                        <th>{"Timing"}</th>
                    </tr>
                    { rows }
                </table>

                <div class="pagination">
                    <button onclick={&on_apply_filters}>
                        {"<< Latest"}
                    </button>

                    if let Some(query) = newer_page_query {
                        <button onclick={
                            let on_page_change = on_page_change.clone();
                            move |_| on_page_change.emit(query.clone())
                        }>
                            {"< Newer"}
                        </button>
                    } else {
                        <button disabled={true}>
                            {"< Newer"}
                        </button>
                    }

                    if let Some(query) = older_page_query {
                        <button onclick={
                            let on_page_change = on_page_change.clone();
                            move |_| on_page_change.emit(query.clone())
                        }>
                            {"Older >"}
                        </button>
                    } else {
                        <button disabled={true}>
                            {"Older >"}
                        </button>
                    }
                </div>

            </ContextProvider<StatusCacheContext>>
        }
    } else {
        html! { <p>{"Loading..."}</p> }
    }
}
