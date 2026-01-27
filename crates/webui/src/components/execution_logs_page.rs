use crate::{
    BASE_URL,
    components::execution_header::{ExecutionHeader, ExecutionLink},
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
    Save {
        response: grpc_client::ListLogsResponse,
    },
    FetchNextPage,
    FetchError,
}

#[derive(Default, Clone, PartialEq)]
struct LogsState {
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
            LogsAction::Save { mut response } => {
                debug!("Saving {response:?}");
                let mut this = self.as_ref().clone();
                this.all_responses.logs.append(&mut response.logs);
                // Update the token for the next call
                this.all_responses.next_page_token = response.next_page_token;
                this.fetch_state = LogsFetchState::RequestFinished;
                Rc::new(this)
            }
            LogsAction::FetchNextPage => {
                let mut this = self.as_ref().clone();
                // Resetting to Requested triggers the use_effect
                this.fetch_state = LogsFetchState::Requested;
                Rc::new(this)
            }
            LogsAction::FetchError => {
                let mut this = self.as_ref().clone();
                this.fetch_state = LogsFetchState::RequestFinished;
                Rc::new(this)
            }
        }
    }
}

#[function_component(LogsPage)]
pub fn execution_log_page(LogsPageProps { execution_id }: &LogsPageProps) -> Html {
    let logs_state = use_reducer_eq(LogsState::default);

    let show_run_id = use_state(|| false);

    use_effect_with(
        (execution_id.clone(), logs_state.clone()),
        |(execution_id, logs_state)| on_state_change(execution_id, logs_state),
    );

    let on_load_more = {
        let logs_state = logs_state.clone();
        Callback::from(move |_| {
            logs_state.dispatch(LogsAction::FetchNextPage);
        })
    };

    let on_toggle_run_id = {
        let show_run_id = show_run_id.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_run_id.set(input.checked());
        })
    };

    let has_more_pages = !logs_state.all_responses.next_page_token.is_empty();
    let is_loading = matches!(logs_state.fetch_state, LogsFetchState::Pending);

    html! {
         <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Logs} />

            <div class="logs-options">
                <label class="run-id">
                    <input
                        type="checkbox"
                        checked={*show_run_id}
                        onchange={on_toggle_run_id}
                    />
                    { "Show Run ID" }
                </label>
            </div>

            <div class="logs-list">
                {
                    for logs_state.all_responses.logs.iter().map(|entry| {
                        render_log_entry(entry, *show_run_id)
                    })
                }

                if logs_state.all_responses.logs.is_empty() && !is_loading {
                    <div style="color: gray; padding: 10px;">{ "No logs found." }</div>
                }
            </div>

            <div class="logs-controls">
                if has_more_pages || is_loading {
                    <button
                        onclick={on_load_more}
                        disabled={is_loading}
                    >
                        if is_loading {
                            { "Loading..." }
                        } else {
                            { "Load More" }
                        }
                    </button>
                }
            </div>
        </>
    }
}

/// Helper to render individual log entries
fn render_log_entry(entry: &grpc_client::list_logs_response::LogEntry, show_run_id: bool) -> Html {
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
                    { run_id_html }
                    <span class={classes!("kind", log_row_class)}>{ format!("[{}]", level_str) }</span>
                    <span class="payload">{ &log_variant.message }</span>
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
            let payload_str = String::from_utf8_lossy(&stream_variant.payload);

            html! {
                <div class="log-row">
                     <span class="log-row-time">{ format!("[{}]", time_str) }</span>
                     { run_id_html }
                     <span class={classes!("kind", log_row_class)}>{ format!("[{}]", stream_prefix) }</span>
                     <span class="payload">{ payload_str }</span>
                </div>
            }
        }
        None => html! { <div>{ "Invalid Log Entry" }</div> },
    }
}

fn on_state_change(execution_id: &ExecutionId, logs_state: &UseReducerHandle<LogsState>) {
    trace!("Triggered on_state_change");
    if matches!(logs_state.fetch_state, LogsFetchState::Requested) {
        logs_state.dispatch(LogsAction::SetPending);
        let execution_id = execution_id.clone();
        let logs_state = logs_state.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let mut execution_client =
                grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                    tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                );
            debug!("Requesting `{}`", logs_state.all_responses.next_page_token);
            let result = execution_client
                .list_logs(grpc_client::ListLogsRequest {
                    execution_id: Some(execution_id.clone()),
                    page_size: 20,
                    page_token: logs_state.all_responses.next_page_token.clone(),
                    show_logs: true,
                    show_streams: true,
                    levels: Vec::new(),
                    stream_types: Vec::new(),
                })
                .await;

            match result {
                Ok(response) => {
                    logs_state.dispatch(LogsAction::Save {
                        response: response.into_inner(),
                    });
                }
                Err(err) => {
                    log::error!("Failed to fetch logs: {err:?}");
                    logs_state.dispatch(LogsAction::FetchError);
                }
            }
        });
    }
}
