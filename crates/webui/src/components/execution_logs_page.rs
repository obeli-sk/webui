use crate::{
    app::Route,
    components::execution_header::{ExecutionHeader, ExecutionLink},
    components::notification::{Notification, NotificationContext},
    components::time_display::{TimeDisplayControl, Timestamp},
    rest::{self, sse::SseStream},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use concepts::{ExecutionId, prefixed_ulid::RunId};
use futures::future::{AbortHandle, abortable};
use gloo::net::http::Request;
use log::debug;
use rest_types::logs::{LogEntryRowSer, LogEntrySer, LogLevelSer, LogStreamTypeSer};
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use web_sys::HtmlElement;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LogsPageProps {
    pub execution_id: ExecutionId,
}

const FILTER_LEVELS: [LogLevelSer; 5] = [
    LogLevelSer::Trace,
    LogLevelSer::Debug,
    LogLevelSer::Info,
    LogLevelSer::Warn,
    LogLevelSer::Error,
];
const DEFAULT_LEVELS: [LogLevelSer; 5] = FILTER_LEVELS;
const FILTER_STREAMS: [LogStreamTypeSer; 2] = [LogStreamTypeSer::Stdout, LogStreamTypeSer::Stderr];
const SCROLL_BOTTOM_THRESHOLD_PX: i32 = 40;

#[derive(Clone, PartialEq)]
struct LogFilters {
    levels: Vec<LogLevelSer>,
    streams: Vec<LogStreamTypeSer>,
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
    fn selected_levels(&self) -> Vec<LogLevelSer> {
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

    fn selected_streams(&self) -> Vec<LogStreamTypeSer> {
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
    /// Fetch entries newer than the last one, even if the previous page was not full.
    Poll,
    SetFollow(bool),
    ExecutionFinished,
    PageLoaded {
        execution_id: ExecutionId,
        show_derived: bool,
        request_generation: u64,
        response: DecodedLogs,
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
    logs: Vec<LogEntry>,
    /// Cursor of the last loaded entry, empty when nothing was loaded yet.
    cursor: String,
    has_more: bool,
    /// False until the first page after a reset arrives.
    loaded: bool,
    follow: bool,
    execution_finished: bool,
    /// Set once a page was requested after the execution finished, nothing new can appear after it.
    complete: bool,
    request_generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct LogEntry {
    created_at: DateTime<Utc>,
    execution_id: ExecutionId,
    run_id: RunId,
    kind: LogKind,
}

#[derive(Clone, Debug, PartialEq)]
enum LogKind {
    Log {
        level: LogLevelSer,
        message: String,
    },
    Stream {
        payload: Vec<u8>,
        stream_type: LogStreamTypeSer,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
struct DecodedLogs {
    logs: Vec<LogEntry>,
    cursor: Option<String>,
    has_more: bool,
}

trait LogLevelExt {
    fn as_str(&self) -> &'static str;
    fn label(self) -> &'static str;
    fn css_class(self) -> &'static str;
}

impl LogLevelExt for LogLevelSer {
    fn as_str(&self) -> &'static str {
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

    fn css_class(self) -> &'static str {
        match self {
            Self::Trace => "kind-trace",
            Self::Debug => "kind-debug",
            Self::Info => "kind-info",
            Self::Warn => "kind-warn",
            Self::Error => "kind-error",
        }
    }
}

trait LogStreamTypeExt {
    fn as_str(&self) -> &'static str;
    fn label(self) -> &'static str;
    fn css_class(self) -> &'static str;
}

impl LogStreamTypeExt for LogStreamTypeSer {
    fn as_str(&self) -> &'static str {
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

    fn css_class(self) -> &'static str {
        match self {
            Self::Stdout => "kind-stdout",
            Self::Stderr => "kind-stderr",
        }
    }
}

fn decode_logs(rows: Vec<LogEntryRowSer>, page_size: usize) -> Result<DecodedLogs, String> {
    let has_more = rows.len() == page_size;
    let cursor = rows.last().map(|row| row.cursor.clone());
    let logs = rows
        .into_iter()
        .map(|row| {
            let (created_at, kind) = match row.info {
                LogEntrySer::Log {
                    created_at,
                    level,
                    message,
                } => (created_at, LogKind::Log { level, message }),
                LogEntrySer::Stream {
                    created_at,
                    payload,
                    stream_type,
                } => {
                    let payload = STANDARD
                        .decode(payload)
                        .map_err(|error| error.to_string())?;
                    (
                        created_at,
                        LogKind::Stream {
                            payload,
                            stream_type,
                        },
                    )
                }
            };
            let execution_id = row
                .execution_id
                .parse()
                .map_err(|err| format!("cannot parse execution ID: {err}"))?;
            Ok(LogEntry {
                created_at,
                execution_id,
                run_id: row.run_id,
                kind,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DecodedLogs {
        logs,
        cursor,
        has_more,
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
            cursor: String::new(),
            has_more: false,
            loaded: false,
            follow: true,
            execution_finished: false,
            complete: false,
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
            } => {
                let execution_finished =
                    self.execution_finished && self.execution_id.as_ref() == Some(&execution_id);
                Rc::new(Self {
                    execution_id: Some(execution_id),
                    show_derived,
                    filters,
                    fetch_state: LogsFetchState::Pending,
                    logs: Vec::new(),
                    cursor: String::new(),
                    has_more: false,
                    loaded: false,
                    follow: self.follow,
                    execution_finished,
                    complete: execution_finished,
                    request_generation: self.request_generation.wrapping_add(1),
                })
            }
            LogsAction::LoadMore | LogsAction::Poll => {
                if self.fetch_state == LogsFetchState::Pending
                    || match action {
                        LogsAction::LoadMore => !self.has_more,
                        _ => self.complete,
                    }
                {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::Pending;
                this.complete = this.execution_finished;
                this.request_generation = this.request_generation.wrapping_add(1);
                Rc::new(this)
            }
            LogsAction::ExecutionFinished => {
                if self.execution_finished {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.execution_finished = true;
                Rc::new(this)
            }
            LogsAction::SetFollow(follow) => {
                if self.follow == follow {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.follow = follow;
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
                if let Some(cursor) = response.cursor {
                    this.cursor = cursor;
                }
                this.has_more = response.has_more;
                this.loaded = true;
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
                this.loaded = true;
                // Stop polling so a failing server does not raise a notification every interval.
                this.follow = false;
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
                logs_state.cursor.clone(),
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

    // While following, load the remaining pages right away, then stream new entries until
    // a page requested after the execution finished arrives.
    {
        let logs_state = logs_state.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (
                logs_state.follow,
                logs_state.fetch_state.clone(),
                logs_state.has_more,
                logs_state.execution_finished,
                logs_state.complete,
                logs_state.request_generation,
            ),
            move |(follow, fetch_state, has_more, execution_finished, complete, _)| {
                let stream = (*follow && *fetch_state == LogsFetchState::Idle).then(|| {
                    if *has_more {
                        logs_state.dispatch(LogsAction::LoadMore);
                        None
                    } else if *complete {
                        None
                    } else if *execution_finished {
                        logs_state.dispatch(LogsAction::Poll);
                        None
                    } else {
                        follow_logs(logs_state, notifications)
                    }
                });
                move || {
                    if let Some(Some(stream)) = stream {
                        stream.abort();
                    }
                }
            },
        );
    }

    let on_finished = {
        let logs_state = logs_state.clone();
        Callback::from(move |()| logs_state.dispatch(LogsAction::ExecutionFinished))
    };

    let list_ref = use_node_ref();
    {
        let list_ref = list_ref.clone();
        use_effect_with(
            (logs_state.logs.len(), logs_state.follow),
            move |(_, follow)| {
                if *follow && let Some(element) = list_ref.cast::<HtmlElement>() {
                    element.set_scroll_top(element.scroll_height());
                }
            },
        );
    }

    let on_scroll = {
        let logs_state = logs_state.clone();
        Callback::from(move |event: Event| {
            let element: HtmlElement = event.target_unchecked_into();
            let distance_from_bottom =
                element.scroll_height() - element.client_height() - element.scroll_top();
            let at_bottom = distance_from_bottom <= SCROLL_BOTTOM_THRESHOLD_PX;
            logs_state.dispatch(LogsAction::SetFollow(at_bottom));
            if at_bottom {
                debug!("Dispatching loadmore");
                logs_state.dispatch(LogsAction::LoadMore);
            }
        })
    };

    let on_toggle_follow = {
        let logs_state = logs_state.clone();
        Callback::from(move |_| {
            let follow = !logs_state.follow;
            logs_state.dispatch(LogsAction::SetFollow(follow));
            if follow {
                logs_state.dispatch(LogsAction::Poll);
            }
        })
    };

    let on_toggle_run_id = {
        let show_run_id = show_run_id.clone();
        Callback::from(move |_| {
            show_run_id.set(!*show_run_id);
        })
    };

    let on_toggle_derived = {
        let logs_state = logs_state.clone();
        let execution_id = execution_id.clone();
        Callback::from(move |_| {
            logs_state.dispatch(LogsAction::Reset {
                execution_id: execution_id.clone(),
                show_derived: !logs_state.show_derived,
                filters: logs_state.filters.clone(),
            });
        })
    };

    let toggle_level = |level: LogLevelSer| {
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

    let toggle_stream = |stream: LogStreamTypeSer| {
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

    // Background polls must not flash the empty state or disable controls.
    let is_loading = !logs_state.loaded;

    html! {
         <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Logs} {on_finished} />

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
                    <button
                        class={classes!(logs_state.show_derived.then_some("selected"))}
                        aria-pressed={logs_state.show_derived.to_string()}
                        title="Show derived executions and the Child column"
                        onclick={on_toggle_derived}
                        disabled={is_loading}
                    >
                        { "Derived" }
                    </button>
                    <button
                        class={classes!(logs_state.follow.then_some("selected"))}
                        aria-pressed={logs_state.follow.to_string()}
                        title="Load new entries as they arrive and keep the list scrolled to the bottom"
                        onclick={on_toggle_follow}
                    >
                        { "Follow" }
                    </button>
                    <button
                        class={classes!((*show_run_id).then_some("selected"))}
                        aria-pressed={show_run_id.to_string()}
                        title="Show Run ID column"
                        onclick={on_toggle_run_id}
                    >
                        { "Run ID" }
                    </button>
                </div>

                <TimeDisplayControl />
            </div>

            <div class="logs-list" ref={list_ref} onscroll={on_scroll}>
                <table class="logs-table">
                    <thead>
                        <tr>
                            <th scope="col">{"Date"}</th>
                            <th scope="col">{"Level / Stream"}</th>
                            if logs_state.show_derived {
                                <th scope="col">{"Child"}</th>
                            }
                            if *show_run_id {
                                <th scope="col">{"Run ID"}</th>
                            }
                            <th scope="col">{"Message"}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {
                            for logs_state.logs.iter().map(|entry| {
                                render_log_entry(entry, execution_id, logs_state.show_derived, *show_run_id, &query)
                            })
                        }
                    </tbody>
                </table>

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
    entry: &LogEntry,
    root_execution_id: &ExecutionId,
    show_derived: bool,
    show_run_id: bool,
    query: &LogsQuery,
) -> Html {
    // Format Timestamp
    let timestamp = html! { <Timestamp target={entry.created_at} /> };

    let execution_id_html = Some(&entry.execution_id)
        .filter(|execution_id| *execution_id != root_execution_id)
        .map(|execution_id| {
            let execution_id_str = execution_id.to_string();
            let child_id = execution_id_str
                .split_once('.')
                .map_or(execution_id_str.as_str(), |(_, child_id)| child_id);
            html! {
                <Link<Route, LogsQuery>
                    to={Route::Logs { execution_id: execution_id.clone() }}
                    query={Some(query.clone())}
                >
                    {child_id.to_owned()}
                </Link<Route, LogsQuery>>
            }
        })
        .unwrap_or_default();

    let (kind_class, kind, payload) = match &entry.kind {
        LogKind::Log { level, message } => (level.css_class(), level.to_string(), message.clone()),
        LogKind::Stream {
            payload,
            stream_type,
        } => (
            stream_type.css_class(),
            stream_type.to_string(),
            // Lossy to prevent crashes on binary data
            String::from_utf8_lossy(payload).into_owned(),
        ),
    };

    html! {
        <tr class="log-row">
            <td class="time">{timestamp}</td>
            <td class={classes!("kind", kind_class)}>{kind}</td>
            if show_derived {
                <td class="execution-id">{execution_id_html}</td>
            }
            if show_run_id {
                <td class="run-id">
                    {entry.run_id.to_string()}
                </td>
            }
            <td class="payload">{payload}</td>
        </tr>
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
        let mut query = logs_query(show_derived, &filters, &page_token);
        query.push(("length", PAGE_SIZE.to_string()));
        let result = rest::get::<Vec<LogEntryRowSer>>(
            &format!("/v1/executions/{execution_id}/logs"),
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

fn logs_query(
    show_derived: bool,
    filters: &LogFilters,
    cursor: &str,
) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("direction", "newer".to_string()),
        ("show_derived", show_derived.to_string()),
    ];
    query.extend(filters.query_params());
    if !cursor.is_empty() {
        query.push(("cursor", cursor.to_string()));
    }
    query
}

/// Streams entries after the current cursor until the server ends the stream, which happens
/// once the execution finished. A final `Poll` then marks the logs complete.
fn follow_logs(
    logs_state: UseReducerHandle<LogsState>,
    notifications: NotificationContext,
) -> Option<AbortHandle> {
    let execution_id = logs_state.execution_id.clone()?;
    let show_derived = logs_state.show_derived;
    let request_generation = logs_state.request_generation;
    if logs_state.filters.levels.is_empty() && logs_state.filters.streams.is_empty() {
        return None;
    }
    let mut query = logs_query(show_derived, &logs_state.filters, &logs_state.cursor);
    query.push(("follow", "true".to_string()));
    let (task, handle) = abortable(async move {
        let result = async {
            let request = Request::get(&format!(
                "{}/v1/executions/{execution_id}/logs",
                crate::BASE_URL
            ))
            .query(query.iter().map(|(key, value)| (*key, value.as_str())));
            let mut stream = SseStream::open(request, None::<&()>).await?;
            while let Some(event) = stream.next().await? {
                if event.event == "logs" {
                    let rows: Vec<LogEntryRowSer> =
                        serde_json::from_str(&event.data).map_err(|err| err.to_string())?;
                    logs_state.dispatch(LogsAction::PageLoaded {
                        execution_id: execution_id.clone(),
                        show_derived,
                        request_generation,
                        response: decode_logs(rows, usize::MAX)?,
                    });
                }
            }
            Ok::<_, String>(())
        }
        .await;
        match result {
            Ok(()) => logs_state.dispatch(LogsAction::Poll),
            Err(err) => {
                log::error!("Failed to follow logs: {err}");
                notifications.push(Notification::error(format!("Failed to follow logs: {err}")));
                logs_state.dispatch(LogsAction::FetchError {
                    execution_id,
                    show_derived,
                    request_generation,
                });
            }
        }
    });
    wasm_bindgen_futures::spawn_local(async move {
        let _ = task.await;
    });
    Some(handle)
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

    const EXECUTION_ID: &str = "E_01KABCDEFGHJKMNPQRSTVWXYZ0";

    #[test]
    fn log_query_preserves_levels_streams_and_empty_selection() {
        let levels = [
            LogLevelSer::Debug,
            LogLevelSer::Info,
            LogLevelSer::Warn,
            LogLevelSer::Error,
        ];
        let query = LogsQuery::from_filters(&LogFilters {
            levels: levels.to_vec(),
            streams: vec![LogStreamTypeSer::Stdout],
        });
        let serialized = serde_json::to_string(&query).unwrap();
        let restored: LogsQuery = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.selected_levels(), levels);
        assert_eq!(restored.levels.as_deref(), Some("debug,info,warn,error"));
        assert_eq!(restored.selected_streams(), [LogStreamTypeSer::Stdout]);

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
            streams: vec![LogStreamTypeSer::Stderr],
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
            levels: vec![LogLevelSer::Error],
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
        let execution_id: ExecutionId = EXECUTION_ID.parse().unwrap();
        let state = Rc::new(LogsState::default()).reduce(LogsAction::Reset {
            execution_id: execution_id.clone(),
            show_derived: true,
            filters: LogFilters::default(),
        });
        let old_generation = state.request_generation;
        let response = DecodedLogs {
            logs: vec![LogEntry {
                created_at: Utc::now(),
                execution_id: execution_id.clone(),
                run_id: RunId::generate(),
                kind: LogKind::Log {
                    level: LogLevelSer::Info,
                    message: "hello".into(),
                },
            }],
            cursor: Some("next".into()),
            has_more: true,
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
                levels: vec![LogLevelSer::Error],
                streams: vec![LogStreamTypeSer::Stderr],
            },
        });
        assert!(state.logs.is_empty());
        assert!(state.cursor.is_empty());
        assert_eq!(state.filters.levels, [LogLevelSer::Error]);
        assert_eq!(state.filters.streams, [LogStreamTypeSer::Stderr]);
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
    fn polling_stops_after_a_page_requested_after_finish() {
        let execution_id: ExecutionId = EXECUTION_ID.parse().unwrap();
        let loaded = |state: Rc<LogsState>| {
            let request_generation = state.request_generation;
            state.reduce(LogsAction::PageLoaded {
                execution_id: execution_id.clone(),
                show_derived: true,
                request_generation,
                response: DecodedLogs::default(),
            })
        };
        let state = loaded(Rc::new(LogsState::default()).reduce(LogsAction::Reset {
            execution_id: execution_id.clone(),
            show_derived: true,
            filters: LogFilters::default(),
        }));
        assert!(state.loaded);
        let state = loaded(state.reduce(LogsAction::Poll));
        assert!(!state.complete);
        // The final poll picks up entries written before the execution finished.
        let state = state.reduce(LogsAction::ExecutionFinished);
        let state = state.reduce(LogsAction::Poll);
        assert!(state.fetch_state == LogsFetchState::Pending);
        let state = loaded(state);
        assert!(state.complete);
        let unchanged = state.clone().reduce(LogsAction::Poll);
        assert!(Rc::ptr_eq(&state, &unchanged));
    }

    #[test]
    fn rest_log_page_decodes_entries_and_cursor() {
        let rows: Vec<LogEntryRowSer> = serde_json::from_str(
            r#"[
                {"cursor":"first","run_id":"Run_01KABCDEFGHJKMNPQRSTVWXYZ0","execution_id":"E_01KABCDEFGHJKMNPQRSTVWXYZ0","type":"log","created_at":"2026-09-25T12:00:00Z","level":"warn","message":"warning"},
                {"cursor":"second","run_id":"Run_01KABCDEFGHJKMNPQRSTVWXYZ1","execution_id":"E_01KABCDEFGHJKMNPQRSTVWXYZ0.n:children_1","type":"stream","created_at":"2026-09-25T12:00:01Z","stream_type":"stdout","payload":"aGk="}
            ]"#,
        )
        .unwrap();
        let page = decode_logs(rows, 2).unwrap();
        assert_eq!(page.cursor.as_deref(), Some("second"));
        assert!(page.has_more);
        assert_eq!(
            page.logs[0].run_id.to_string(),
            "Run_01KABCDEFGHJKMNPQRSTVWXYZ0"
        );
        assert!(
            matches!(&page.logs[0].kind, LogKind::Log { level: LogLevelSer::Warn, message } if message == "warning")
        );
        assert!(
            matches!(&page.logs[1].kind, LogKind::Stream { payload, stream_type: LogStreamTypeSer::Stdout } if payload == b"hi")
        );
    }
}
