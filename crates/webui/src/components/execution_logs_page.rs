use crate::{
    app::Route,
    components::execution_header::{ExecutionHeader, ExecutionLink},
    components::notification::{Notification, NotificationContext},
    components::time_display::{TimeDisplayControl, Timestamp},
    grpc::grpc_client::{self, ExecutionId},
    rest,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::DateTime;
use log::debug;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use web_sys::{HtmlElement, HtmlInputElement};
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LogsPageProps {
    pub execution_id: ExecutionId,
}

const FILTER_LEVELS: [RestLogLevel; 5] = [
    RestLogLevel::Trace,
    RestLogLevel::Debug,
    RestLogLevel::Info,
    RestLogLevel::Warn,
    RestLogLevel::Error,
];
const DEFAULT_LEVELS: [RestLogLevel; 5] = FILTER_LEVELS;
const FILTER_STREAMS: [RestStreamType; 2] = [RestStreamType::Stdout, RestStreamType::Stderr];

#[derive(Clone, PartialEq)]
struct LogFilters {
    levels: Vec<RestLogLevel>,
    streams: Vec<RestStreamType>,
}

impl Default for LogFilters {
    fn default() -> Self {
        Self {
            levels: DEFAULT_LEVELS.to_vec(),
            streams: FILTER_STREAMS.to_vec(),
        }
    }
}

impl LogFilters {
    fn query_params(&self) -> Vec<(&'static str, String)> {
        let mut query = vec![
            ("show_logs", (!self.levels.is_empty()).to_string()),
            ("show_streams", (!self.streams.is_empty()).to_string()),
        ];
        query.extend(
            self.levels
                .iter()
                .map(|level| ("level", level.as_str().to_string())),
        );
        query.extend(
            self.streams
                .iter()
                .map(|stream| ("stream_type", stream.as_str().to_string())),
        );
        query
    }
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
struct LogsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    levels: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    streams: Option<String>,
}

impl LogsQuery {
    fn selected_levels(&self) -> Vec<RestLogLevel> {
        match self.levels.as_deref() {
            None => DEFAULT_LEVELS.to_vec(),
            Some("") => Vec::new(),
            Some(levels) => levels
                .split(',')
                .map(|level| {
                    FILTER_LEVELS
                        .into_iter()
                        .find(|candidate| candidate.as_str() == level)
                })
                .collect::<Option<Vec<_>>>()
                .unwrap_or_else(|| DEFAULT_LEVELS.to_vec()),
        }
    }

    fn selected_streams(&self) -> Vec<RestStreamType> {
        match self.streams.as_deref() {
            None => FILTER_STREAMS.to_vec(),
            Some("") => Vec::new(),
            Some(streams) => streams
                .split(',')
                .map(|stream| {
                    FILTER_STREAMS
                        .into_iter()
                        .find(|candidate| candidate.as_str() == stream)
                })
                .collect::<Option<Vec<_>>>()
                .unwrap_or_else(|| FILTER_STREAMS.to_vec()),
        }
    }

    fn filters(&self) -> LogFilters {
        LogFilters {
            levels: self.selected_levels(),
            streams: self.selected_streams(),
        }
    }

    fn from_filters(filters: &LogFilters) -> Self {
        let levels = &filters.levels;
        let streams = &filters.streams;
        Self {
            levels: (levels.as_slice() != DEFAULT_LEVELS).then(|| {
                levels
                    .iter()
                    .map(|level| level.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            }),
            streams: (streams.as_slice() != FILTER_STREAMS).then(|| {
                streams
                    .iter()
                    .map(|stream| stream.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            }),
        }
    }
}

#[derive(Clone, PartialEq, Default)]
enum LogsFetchState {
    #[default]
    Pending,
    Idle,
}

enum LogsAction {
    Reset {
        execution_id: ExecutionId,
        show_derived: bool,
        filters: LogFilters,
    },
    LoadMore,
    PageLoaded {
        execution_id: ExecutionId,
        show_derived: bool,
        request_generation: u64,
        response: grpc_client::ListLogsResponse,
    },
    FetchError {
        execution_id: ExecutionId,
        show_derived: bool,
        request_generation: u64,
    },
}

#[derive(Clone, PartialEq)]
struct LogsState {
    execution_id: Option<ExecutionId>,
    show_derived: bool,
    filters: LogFilters,
    fetch_state: LogsFetchState,
    logs: Vec<grpc_client::list_logs_response::LogEntry>,
    next_page_token: String,
    request_generation: u64,
}

#[derive(Deserialize)]
struct RestLogRow {
    cursor: String,
    run_id: String,
    execution_id: String,
    #[serde(flatten)]
    entry: RestLogEntry,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RestLogEntry {
    Log {
        created_at: DateTime<chrono::Utc>,
        level: RestLogLevel,
        message: String,
    },
    Stream {
        created_at: DateTime<chrono::Utc>,
        payload: String,
        stream_type: RestStreamType,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RestLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl RestLogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Trace => "Trace",
            Self::Debug => "Debug",
            Self::Info => "Info",
            Self::Warn => "Warning",
            Self::Error => "Error",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RestStreamType {
    Stdout,
    Stderr,
}

impl RestStreamType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "Stdout",
            Self::Stderr => "Stderr",
        }
    }
}

fn decode_logs(
    rows: Vec<RestLogRow>,
    page_size: usize,
) -> Result<grpc_client::ListLogsResponse, String> {
    use grpc_client::list_logs_response::{LogEntry, log_entry::Entry};

    let next_page_token = if rows.len() == page_size {
        rows.last()
            .map(|row| row.cursor.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let logs = rows
        .into_iter()
        .map(|row| {
            let (created_at, entry) = match row.entry {
                RestLogEntry::Log {
                    created_at,
                    level,
                    message,
                } => {
                    let level = match level {
                        RestLogLevel::Trace => 1,
                        RestLogLevel::Debug => 2,
                        RestLogLevel::Info => 3,
                        RestLogLevel::Warn => 4,
                        RestLogLevel::Error => 5,
                    };
                    (
                        created_at,
                        Entry::Log(grpc_client::list_logs_response::log_entry::LogVariant {
                            level,
                            message,
                        }),
                    )
                }
                RestLogEntry::Stream {
                    created_at,
                    payload,
                    stream_type,
                } => {
                    let payload = STANDARD
                        .decode(payload)
                        .map_err(|error| error.to_string())?;
                    let stream_type = match stream_type {
                        RestStreamType::Stdout => grpc_client::LogStreamType::Stdout as i32,
                        RestStreamType::Stderr => grpc_client::LogStreamType::Stderr as i32,
                    };
                    (
                        created_at,
                        Entry::Stream(grpc_client::list_logs_response::log_entry::StreamVariant {
                            payload,
                            stream_type,
                        }),
                    )
                }
            };
            Ok(LogEntry {
                created_at: Some(created_at.into()),
                entry: Some(entry),
                run_id: Some(grpc_client::RunId { id: row.run_id }),
                execution_id: Some(ExecutionId {
                    id: row.execution_id,
                }),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(grpc_client::ListLogsResponse {
        logs,
        next_page_token,
        prev_page_token: None,
    })
}

impl Default for LogsState {
    fn default() -> Self {
        Self {
            execution_id: None,
            show_derived: true,
            filters: LogFilters::default(),
            fetch_state: LogsFetchState::Pending,
            logs: Vec::new(),
            next_page_token: String::new(),
            request_generation: 0,
        }
    }
}

impl Reducible for LogsState {
    type Action = LogsAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            LogsAction::Reset {
                execution_id,
                show_derived,
                filters,
            } => Rc::new(Self {
                execution_id: Some(execution_id),
                show_derived,
                filters,
                fetch_state: LogsFetchState::Pending,
                logs: Vec::new(),
                next_page_token: String::new(),
                request_generation: self.request_generation.wrapping_add(1),
            }),
            LogsAction::LoadMore => {
                if self.fetch_state == LogsFetchState::Pending || self.next_page_token.is_empty() {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::Pending;
                this.request_generation = this.request_generation.wrapping_add(1);
                Rc::new(this)
            }
            LogsAction::PageLoaded {
                execution_id,
                show_derived,
                request_generation,
                mut response,
            } => {
                if !request_matches(&self, &execution_id, show_derived, request_generation) {
                    return self;
                }
                debug!("Appending {response:?}");
                let mut this = self.as_ref().clone();
                this.logs.append(&mut response.logs);
                this.next_page_token = response.next_page_token;
                this.fetch_state = LogsFetchState::Idle;
                Rc::new(this)
            }
            LogsAction::FetchError {
                execution_id,
                show_derived,
                request_generation,
            } => {
                if !request_matches(&self, &execution_id, show_derived, request_generation) {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::Idle;
                Rc::new(this)
            }
        }
    }
}

#[component(LogsPage)]
pub fn execution_log_page(LogsPageProps { execution_id }: &LogsPageProps) -> Html {
    let navigator = use_navigator().unwrap();
    let location = use_location().unwrap();
    let query = location.query::<LogsQuery>().unwrap_or_default();
    let filters = query.filters();
    let logs_state = use_reducer_eq(LogsState::default);
    let show_run_id = use_state(|| false);
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    {
        let logs_state = logs_state.clone();
        let show_derived = logs_state.show_derived;
        use_effect_with(
            (execution_id.clone(), filters),
            move |(execution_id, filters)| {
                logs_state.dispatch(LogsAction::Reset {
                    execution_id: execution_id.clone(),
                    show_derived,
                    filters: filters.clone(),
                });
            },
        );
    }

    {
        let logs_state = logs_state.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (
                logs_state.execution_id.clone(),
                logs_state.fetch_state.clone(),
                logs_state.show_derived,
                logs_state.filters.clone(),
                logs_state.next_page_token.clone(),
                logs_state.request_generation,
            ),
            move |(
                execution_id,
                fetch_state,
                show_derived,
                filters,
                page_token,
                request_generation,
            )| {
                if *fetch_state == LogsFetchState::Pending
                    && let Some(execution_id) = execution_id.clone()
                {
                    fetch_logs_page(
                        execution_id,
                        *show_derived,
                        filters.clone(),
                        page_token.clone(),
                        *request_generation,
                        logs_state.clone(),
                        notifications.clone(),
                    );
                }
            },
        );
    }

    let on_scroll = {
        let logs_state = logs_state.clone();
        Callback::from(move |event: Event| {
            let element: HtmlElement = event.target_unchecked_into();
            const LOAD_MORE_THRESHOLD_PX: i32 = 40;
            let distance_from_bottom =
                element.scroll_height() - element.client_height() - element.scroll_top();
            if distance_from_bottom <= LOAD_MORE_THRESHOLD_PX {
                debug!("Dispatching loadmore");
                logs_state.dispatch(LogsAction::LoadMore);
            }
        })
    };

    let on_toggle_run_id = {
        let show_run_id = show_run_id.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_run_id.set(input.checked());
        })
    };

    let on_toggle_derived = {
        let logs_state = logs_state.clone();
        let execution_id = execution_id.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            logs_state.dispatch(LogsAction::Reset {
                execution_id: execution_id.clone(),
                show_derived: input.checked(),
                filters: logs_state.filters.clone(),
            });
        })
    };

    let toggle_level = |level: RestLogLevel| {
        let logs_state = logs_state.clone();
        let execution_id = execution_id.clone();
        let navigator = navigator.clone();
        Callback::from(move |_| {
            let mut filters = logs_state.filters.clone();
            filters.levels = FILTER_LEVELS
                .into_iter()
                .filter(|candidate| filters.levels.contains(candidate) != (*candidate == level))
                .collect();
            let _ = navigator.push_with_query(
                &Route::Logs {
                    execution_id: execution_id.clone(),
                },
                &LogsQuery::from_filters(&filters),
            );
        })
    };

    let toggle_stream = |stream: RestStreamType| {
        let logs_state = logs_state.clone();
        let execution_id = execution_id.clone();
        let navigator = navigator.clone();
        Callback::from(move |_| {
            let mut filters = logs_state.filters.clone();
            filters.streams = FILTER_STREAMS
                .into_iter()
                .filter(|candidate| filters.streams.contains(candidate) != (*candidate == stream))
                .collect();
            let _ = navigator.push_with_query(
                &Route::Logs {
                    execution_id: execution_id.clone(),
                },
                &LogsQuery::from_filters(&filters),
            );
        })
    };

    let is_loading = logs_state.fetch_state == LogsFetchState::Pending;

    html! {
         <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Logs} />

            <div class="logs-options">
                <div class="logs-levels" aria-label="Log levels and streams">
                    {for FILTER_LEVELS.into_iter().map(|level| html! {
                        <button
                            class={classes!(logs_state.filters.levels.contains(&level).then_some("selected"))}
                            aria-pressed={logs_state.filters.levels.contains(&level).to_string()}
                            onclick={toggle_level(level)}
                        >
                            {level.label()}
                        </button>
                    })}
                    {for FILTER_STREAMS.into_iter().map(|stream| html! {
                        <button
                            class={classes!(logs_state.filters.streams.contains(&stream).then_some("selected"))}
                            aria-pressed={logs_state.filters.streams.contains(&stream).to_string()}
                            onclick={toggle_stream(stream)}
                        >
                            {stream.label()}
                        </button>
                    })}
                </div>
                <div class="logs-filters">
                    <label>
                        <input
                            type="checkbox"
                            checked={logs_state.show_derived}
                            onchange={on_toggle_derived}
                            disabled={is_loading}
                        />
                        { "Show derived executions" }
                    </label>

                    <label>
                        <input
                            type="checkbox"
                            checked={*show_run_id}
                            onchange={on_toggle_run_id}
                        />
                        { "Show Run ID" }
                    </label>
                </div>

                <TimeDisplayControl />
            </div>

            <div class="logs-list" onscroll={on_scroll}>
                {
                    for logs_state.logs.iter().map(|entry| {
                        render_log_entry(entry, execution_id, *show_run_id, &query)
                    })
                }

                if logs_state.logs.is_empty() {
                    <div class="logs-empty">
                        if is_loading {
                            { "Loading..." }
                        } else {
                            { "No logs found." }
                        }
                    </div>
                }
            </div>
        </>
    }
}

/// Helper to render individual log entries
fn render_log_entry(
    entry: &grpc_client::list_logs_response::LogEntry,
    root_execution_id: &ExecutionId,
    show_run_id: bool,
    query: &LogsQuery,
) -> Html {
    // Format Timestamp
    let timestamp = if let Some(ts) = &entry.created_at {
        let date_time = DateTime::from(*ts);
        html! { <Timestamp target={date_time} /> }
    } else {
        html! { {"Unknown time"} }
    };

    let run_id_html = if show_run_id {
        if let Some(run_id) = &entry.run_id {
            html! { <span class="run-id">{ format!("[{}]", run_id.id) }</span> }
        } else {
            html! {}
        }
    } else {
        html! {}
    };

    let execution_id_html = entry
        .execution_id
        .as_ref()
        .filter(|execution_id| *execution_id != root_execution_id)
        .map(|execution_id| {
            let child_id = execution_id
                .id
                .split_once('.')
                .map_or(execution_id.id.as_str(), |(_, child_id)| child_id);
            html! {
                <span class="execution-id">
                    <Link<Route, LogsQuery>
                        to={Route::Logs { execution_id: execution_id.clone() }}
                        query={Some(query.clone())}
                    >
                        {format!("[{child_id}]")}
                    </Link<Route, LogsQuery>>
                </span>
            }
        })
        .unwrap_or_default();

    // Access the 'oneof' entry
    match &entry.entry {
        Some(grpc_client::list_logs_response::log_entry::Entry::Log(log_variant)) => {
            let log_row_class = match log_variant.level {
                1 => "kind-trace",
                2 => "kind-debug",
                3 => "kind-info",
                4 => "kind-warn",
                5 => "kind-error",
                _ => "kind-unknown",
            };

            // Map int enum to string manually or via generated derived Debug/Display
            let level_str = match log_variant.level {
                1 => "TRACE",
                2 => "DEBUG",
                3 => "INFO",
                4 => "WARN",
                5 => "ERROR",
                _ => "UNKNOWN",
            };

            html! {
                <div class="log-row">
                    <span class="time">{"["}{timestamp}{"]"}</span>
                    { execution_id_html }
                    { run_id_html }
                    <span class={classes!("kind", log_row_class)}>{ format!("[{}]", level_str) }</span>
                    <span class="payload">{ log_variant.message.clone() }</span>
                </div>
            }
        }
        Some(grpc_client::list_logs_response::log_entry::Entry::Stream(stream_variant)) => {
            let (stream_prefix, log_row_class) = match stream_variant.stream_type() {
                grpc_client::LogStreamType::Unspecified => ("UNKNOWN", "kind-unknown"),
                grpc_client::LogStreamType::Stdout => ("STDOUT", "kind-stdout"),
                grpc_client::LogStreamType::Stderr => ("STDERR", "kind-stderr"),
            };

            // Convert bytes to UTF-8 string (lossy to prevent crashes on binary data)
            let payload_str = String::from_utf8_lossy(&stream_variant.payload).into_owned();

            html! {
                <div class="log-row">
                     <span class="time">{"["}{timestamp}{"]"}</span>
                     { execution_id_html }
                     { run_id_html }
                     <span class={classes!("kind", log_row_class)}>{ format!("[{}]", stream_prefix) }</span>
                     <span class="payload">{ payload_str }</span>
                </div>
            }
        }
        None => html! { <div>{ "Invalid Log Entry" }</div> },
    }
}

fn fetch_logs_page(
    execution_id: ExecutionId,
    show_derived: bool,
    filters: LogFilters,
    page_token: String,
    request_generation: u64,
    logs_state: UseReducerHandle<LogsState>,
    notifications: NotificationContext,
) {
    if filters.levels.is_empty() && filters.streams.is_empty() {
        logs_state.dispatch(LogsAction::PageLoaded {
            execution_id,
            show_derived,
            request_generation,
            response: Default::default(),
        });
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        const PAGE_SIZE: usize = 200;
        debug!("Requesting logs page `{page_token}`");
        let mut query = vec![
            ("length", PAGE_SIZE.to_string()),
            ("direction", "newer".to_string()),
            ("show_derived", show_derived.to_string()),
        ];
        query.extend(filters.query_params());
        if !page_token.is_empty() {
            query.push(("cursor", page_token));
        }
        let result = rest::get::<Vec<RestLogRow>>(
            &format!("/v1/executions/{}/logs", execution_id.id),
            &query,
        )
        .await
        .and_then(|rows| decode_logs(rows, PAGE_SIZE));

        match result {
            Ok(response) => {
                logs_state.dispatch(LogsAction::PageLoaded {
                    execution_id,
                    show_derived,
                    request_generation,
                    response,
                });
            }
            Err(err) => {
                log::error!("Failed to fetch logs: {err:?}");
                notifications.push(Notification::error(format!(
                    "Failed to fetch logs: {}",
                    err
                )));
                logs_state.dispatch(LogsAction::FetchError {
                    execution_id,
                    show_derived,
                    request_generation,
                });
            }
        }
    });
}

fn request_matches(
    state: &LogsState,
    execution_id: &ExecutionId,
    show_derived: bool,
    request_generation: u64,
) -> bool {
    state.execution_id.as_ref() == Some(execution_id)
        && state.show_derived == show_derived
        && state.request_generation == request_generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_client::list_logs_response::log_entry::Entry;

    #[test]
    fn log_query_preserves_levels_streams_and_empty_selection() {
        let levels = [
            RestLogLevel::Debug,
            RestLogLevel::Info,
            RestLogLevel::Warn,
            RestLogLevel::Error,
        ];
        let query = LogsQuery::from_filters(&LogFilters {
            levels: levels.to_vec(),
            streams: vec![RestStreamType::Stdout],
        });
        let serialized = serde_json::to_string(&query).unwrap();
        let restored: LogsQuery = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.selected_levels(), levels);
        assert_eq!(restored.levels.as_deref(), Some("debug,info,warn,error"));
        assert_eq!(restored.selected_streams(), [RestStreamType::Stdout]);

        let query = LogsQuery::from_filters(&LogFilters {
            levels: Vec::new(),
            streams: Vec::new(),
        });
        let serialized = serde_json::to_string(&query).unwrap();
        let restored: LogsQuery = serde_json::from_str(&serialized).unwrap();
        assert!(restored.selected_levels().is_empty());
        assert!(restored.selected_streams().is_empty());
        assert_eq!(LogsQuery::default().selected_levels(), DEFAULT_LEVELS);
        assert_eq!(LogsQuery::default().selected_streams(), FILTER_STREAMS);
    }

    #[test]
    fn stream_filters_disable_unselected_log_sources() {
        let params = LogFilters {
            levels: Vec::new(),
            streams: vec![RestStreamType::Stderr],
        }
        .query_params();
        assert_eq!(
            params,
            vec![
                ("show_logs", "false".to_string()),
                ("show_streams", "true".to_string()),
                ("stream_type", "stderr".to_string()),
            ]
        );
        let params = LogFilters {
            levels: vec![RestLogLevel::Error],
            streams: Vec::new(),
        }
        .query_params();
        assert_eq!(
            params,
            vec![
                ("show_logs", "true".to_string()),
                ("show_streams", "false".to_string()),
                ("level", "error".to_string()),
            ]
        );
    }

    #[test]
    fn changing_filters_resets_pagination_and_rejects_stale_pages() {
        let execution_id = ExecutionId { id: "E_1".into() };
        let state = Rc::new(LogsState::default()).reduce(LogsAction::Reset {
            execution_id: execution_id.clone(),
            show_derived: true,
            filters: LogFilters::default(),
        });
        let old_generation = state.request_generation;
        let response = grpc_client::ListLogsResponse {
            logs: vec![Default::default()],
            next_page_token: "next".into(),
            prev_page_token: None,
        };
        let state = state.reduce(LogsAction::PageLoaded {
            execution_id: execution_id.clone(),
            show_derived: true,
            request_generation: old_generation,
            response: response.clone(),
        });
        assert_eq!(state.logs.len(), 1);
        let state = state.reduce(LogsAction::Reset {
            execution_id: execution_id.clone(),
            show_derived: true,
            filters: LogFilters {
                levels: vec![RestLogLevel::Error],
                streams: vec![RestStreamType::Stderr],
            },
        });
        assert!(state.logs.is_empty());
        assert!(state.next_page_token.is_empty());
        assert_eq!(state.filters.levels, [RestLogLevel::Error]);
        assert_eq!(state.filters.streams, [RestStreamType::Stderr]);
        assert!(state.fetch_state == LogsFetchState::Pending);
        let unchanged = state.clone().reduce(LogsAction::PageLoaded {
            execution_id,
            show_derived: true,
            request_generation: old_generation,
            response,
        });
        assert!(Rc::ptr_eq(&state, &unchanged));
    }

    #[test]
    fn rest_log_page_decodes_entries_and_cursor() {
        let rows: Vec<RestLogRow> = serde_json::from_str(
            r#"[
                {"cursor":"first","run_id":"Run_1","execution_id":"E_1","type":"log","created_at":"2026-09-25T12:00:00Z","level":"warn","message":"warning"},
                {"cursor":"second","run_id":"Run_2","execution_id":"E_1.0","type":"stream","created_at":"2026-09-25T12:00:01Z","stream_type":"stdout","payload":"aGk="}
            ]"#,
        )
        .unwrap();
        let page = decode_logs(rows, 2).unwrap();
        assert_eq!(page.next_page_token, "second");
        assert_eq!(page.logs[0].run_id.as_ref().unwrap().id, "Run_1");
        assert!(
            matches!(&page.logs[0].entry, Some(Entry::Log(log)) if log.level == 4 && log.message == "warning")
        );
        assert!(
            matches!(&page.logs[1].entry, Some(Entry::Stream(stream)) if stream.payload == b"hi" && stream.stream_type == grpc_client::LogStreamType::Stdout as i32)
        );
    }
}
