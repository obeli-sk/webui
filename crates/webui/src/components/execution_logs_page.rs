use crate::{
    BASE_URL,
    components::execution_header::{ExecutionHeader, ExecutionLink},
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{self, ExecutionId},
    util::time::format_date,
};
use chrono::DateTime;
use log::{debug, trace};
use std::rc::Rc;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LogsPageProps {
    pub execution_id: ExecutionId,
}

#[derive(Clone, PartialEq, Default)]
enum LogsFetchState {
    #[default]
    Requested,
    Pending,
    RequestFinished,
}

enum LogsAction {
    SetPending,
    Reset {
        execution_id: ExecutionId,
    },
    Save {
        execution_id: ExecutionId,
        response: grpc_client::ListLogsResponse,
    },
    Refresh,
    FetchError {
        execution_id: ExecutionId,
    },
}

#[derive(Default, Clone, PartialEq)]
struct LogsState {
    execution_id: Option<ExecutionId>,
    fetch_state: LogsFetchState,
    all_responses: grpc_client::ListLogsResponse,
}

impl Reducible for LogsState {
    type Action = LogsAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            LogsAction::SetPending => {
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::Pending;
                Rc::new(this)
            }
            LogsAction::Reset { execution_id } => Rc::new(Self {
                execution_id: Some(execution_id),
                ..Default::default()
            }),
            LogsAction::Save {
                execution_id,
                response,
            } => {
                if this_execution_differs(&self.execution_id, &execution_id) {
                    return self;
                }
                debug!("Saving {response:?}");
                let mut this = self.as_ref().clone();
                this.all_responses = response;
                this.fetch_state = LogsFetchState::RequestFinished;
                Rc::new(this)
            }
            LogsAction::Refresh => {
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::Requested;
                Rc::new(this)
            }
            LogsAction::FetchError { execution_id } => {
                if this_execution_differs(&self.execution_id, &execution_id) {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::RequestFinished;
                Rc::new(this)
            }
        }
    }
}

#[component(LogsPage)]
pub fn execution_log_page(LogsPageProps { execution_id }: &LogsPageProps) -> Html {
    let logs_state = use_reducer_eq(LogsState::default);
    let show_run_id = use_state(|| false);
    let show_derived = use_state(|| true);
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    {
        let logs_state = logs_state.clone();
        use_effect_with(execution_id.clone(), move |execution_id| {
            logs_state.dispatch(LogsAction::Reset {
                execution_id: execution_id.clone(),
            });
        });
    }

    use_effect_with(
        (
            execution_id.clone(),
            logs_state.clone(),
            notifications.clone(),
            *show_derived,
        ),
        |(execution_id, logs_state, notifications, show_derived)| {
            on_state_change(execution_id, logs_state, notifications, *show_derived)
        },
    );

    let on_refresh = {
        let logs_state = logs_state.clone();
        Callback::from(move |_| {
            logs_state.dispatch(LogsAction::Refresh);
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
        let show_derived = show_derived.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_derived.set(input.checked());
            logs_state.dispatch(LogsAction::Refresh);
        })
    };

    let is_loading = matches!(logs_state.fetch_state, LogsFetchState::Pending);

    html! {
         <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Logs} />

            <div class="logs-options">
                <div class="logs-filters">
                    <label>
                        <input
                            type="checkbox"
                            checked={*show_derived}
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

                <button
                    class="logs-refresh"
                    onclick={on_refresh}
                    disabled={is_loading}
                >
                    if is_loading {
                        { "Refreshing..." }
                    } else {
                        { "Refresh" }
                    }
                </button>
            </div>

            <div class="logs-list">
                {
                    for logs_state.all_responses.logs.iter().map(|entry| {
                        render_log_entry(entry, execution_id, *show_run_id)
                    })
                }

                if logs_state.all_responses.logs.is_empty() {
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

fn on_state_change(
    execution_id: &ExecutionId,
    logs_state: &UseReducerHandle<LogsState>,
    notifications: &NotificationContext,
    show_derived: bool,
) {
    trace!("Triggered on_state_change");
    if logs_state.execution_id.as_ref() == Some(execution_id)
        && matches!(logs_state.fetch_state, LogsFetchState::Requested)
    {
        logs_state.dispatch(LogsAction::SetPending);
        let execution_id = execution_id.clone();
        let logs_state = logs_state.clone();
        let notifications = notifications.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let mut execution_client =
                grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                    tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                );
            const PAGE_SIZE: i32 = 200;
            let mut page_token = String::new();
            let mut logs = Vec::new();

            loop {
                debug!("Requesting logs page `{page_token}`");
                let result = execution_client
                    .list_logs(grpc_client::ListLogsRequest {
                        execution_id: Some(execution_id.clone()),
                        page_size: PAGE_SIZE,
                        page_token: page_token.clone(),
                        show_logs: true,
                        show_streams: true,
                        levels: Vec::new(),
                        stream_types: Vec::new(),
                        show_derived,
                    })
                    .await;

                match result {
                    Ok(response) => {
                        let mut response = response.into_inner();
                        let page_len = response.logs.len();
                        logs.append(&mut response.logs);
                        page_token = response.next_page_token;
                        if page_len < PAGE_SIZE as usize {
                            logs_state.dispatch(LogsAction::Save {
                                execution_id,
                                response: grpc_client::ListLogsResponse {
                                    logs,
                                    next_page_token: String::new(),
                                    prev_page_token: None,
                                },
                            });
                            break;
                        }
                    }
                    Err(err) => {
                        log::error!("Failed to fetch logs: {err:?}");
                        notifications.push(Notification::error(format!(
                            "Failed to fetch logs: {}",
                            err.message()
                        )));
                        logs_state.dispatch(LogsAction::FetchError { execution_id });
                        break;
                    }
                }
            }
        });
    }
}

fn this_execution_differs(current: &Option<ExecutionId>, response: &ExecutionId) -> bool {
    current.as_ref() != Some(response)
}
