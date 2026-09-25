use crate::{
    components::execution_header::{ExecutionHeader, ExecutionLink},
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{self, ExecutionId},
    rest,
    util::time::format_date,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::DateTime;
use log::debug;
use serde::Deserialize;
use std::rc::Rc;
use web_sys::{HtmlElement, HtmlInputElement};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LogsPageProps {
    pub execution_id: ExecutionId,
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RestLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RestStreamType {
    Stdout,
    Stderr,
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
            } => Rc::new(Self {
                execution_id: Some(execution_id),
                show_derived,
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
    let logs_state = use_reducer_eq(LogsState::default);
    let show_run_id = use_state(|| false);
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    {
        let logs_state = logs_state.clone();
        use_effect_with(execution_id.clone(), move |execution_id| {
            logs_state.dispatch(LogsAction::Reset {
                execution_id: execution_id.clone(),
                show_derived: true,
            });
        });
    }

    {
        let logs_state = logs_state.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (
                logs_state.execution_id.clone(),
                logs_state.fetch_state.clone(),
                logs_state.show_derived,
                logs_state.next_page_token.clone(),
                logs_state.request_generation,
            ),
            move |(execution_id, fetch_state, show_derived, page_token, request_generation)| {
                if *fetch_state == LogsFetchState::Pending
                    && let Some(execution_id) = execution_id.clone()
                {
                    fetch_logs_page(
                        execution_id,
                        *show_derived,
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
            });
        })
    };

    let is_loading = logs_state.fetch_state == LogsFetchState::Pending;

    html! {
         <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Logs} />

            <div class="logs-options">
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

            </div>

            <div class="logs-list" onscroll={on_scroll}>
                {
                    for logs_state.logs.iter().map(|entry| {
                        render_log_entry(entry, execution_id, *show_run_id)
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
) -> Html {
    // Format Timestamp
    let time_str = if let Some(ts) = &entry.created_at {
        let date_time = DateTime::from(*ts);
        format_date(date_time)
    } else {
        "Unknown Time".to_string()
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
                    { ExecutionLink::Logs.link(execution_id.clone(), &format!("[{child_id}]")) }
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
                    <span class="time">{ format!("[{}]", time_str) }</span>
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
                     <span class="time">{ format!("[{}]", time_str) }</span>
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
    page_token: String,
    request_generation: u64,
    logs_state: UseReducerHandle<LogsState>,
    notifications: NotificationContext,
) {
    wasm_bindgen_futures::spawn_local(async move {
        const PAGE_SIZE: usize = 200;
        debug!("Requesting logs page `{page_token}`");
        let mut query = vec![
            ("length", PAGE_SIZE.to_string()),
            ("direction", "newer".to_string()),
            ("show_derived", show_derived.to_string()),
        ];
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
