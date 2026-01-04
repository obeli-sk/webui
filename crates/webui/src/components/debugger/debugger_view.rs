use crate::{
    BASE_URL,
    app::{Route, query::BacktraceVersionsPath},
    components::{
        code::syntect_code_block::{SyntectCodeBlock, highlight_code_line_by_line},
        execution_detail::utils::{compute_join_next_to_response, event_to_detail},
        execution_header::{ExecutionHeader, ExecutionLink},
        trace::trace_view::{PAGE, SLEEP_MILLIS},
    },
    grpc::{
        grpc_client::{
            self, ComponentId, ExecutionEvent, ExecutionId, GetBacktraceResponse,
            GetBacktraceSourceRequest, JoinSetId, JoinSetResponseEvent, ResponseWithCursor,
            execution_event::{self, history_event},
            get_backtrace_request, join_set_response_event,
        },
        version::VersionType,
    },
    util::trace_id,
};
use gloo::timers::future::TimeoutFuture;
use hashbrown::HashMap;
use log::{debug, trace};
use std::{collections::BTreeSet, ops::Deref as _, path::PathBuf, rc::Rc};
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq)]
pub struct DebuggerViewProps {
    pub execution_id: grpc_client::ExecutionId,
    pub versions: BacktraceVersionsPath,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cursors {
    version_from: u32,
    responses_cursor_from: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum ExecutionFetchState {
    Requested(Cursors),
    Pending,
    Finished,
}

enum DebuggerStateAction {
    AddExecutionId(ExecutionId),
    SetPending(ExecutionId),
    SavePage {
        execution_id: ExecutionId,
        new_events: Vec<ExecutionEvent>,
        new_responses: Vec<ResponseWithCursor>,
        is_finished: bool,
    },
    RequestNextPage {
        execution_id: ExecutionId,
        cursors: Cursors,
    },
}

#[derive(Default, Clone, PartialEq)]
struct DebuggerState {
    execution_ids_to_fetch_state: HashMap<ExecutionId, ExecutionFetchState>,
    events: HashMap<ExecutionId, Vec<ExecutionEvent>>,
    responses: HashMap<ExecutionId, HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
}

impl Reducible for DebuggerState {
    type Action = DebuggerStateAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            DebuggerStateAction::AddExecutionId(execution_id) => {
                if !self
                    .execution_ids_to_fetch_state
                    .contains_key(&execution_id)
                {
                    let mut this = self.as_ref().clone();
                    this.execution_ids_to_fetch_state.insert(
                        execution_id,
                        ExecutionFetchState::Requested(Cursors::default()),
                    );
                    Rc::from(this)
                } else {
                    self
                }
            }
            DebuggerStateAction::SetPending(execution_id) => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Pending);
                Rc::from(this)
            }
            DebuggerStateAction::RequestNextPage {
                execution_id,
                cursors,
            } => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Requested(cursors));
                Rc::from(this)
            }
            DebuggerStateAction::SavePage {
                execution_id,
                new_events,
                new_responses,
                is_finished,
            } => {
                let mut this = self.as_ref().clone();
                this.events
                    .entry(execution_id.clone())
                    .or_default()
                    .extend(new_events);

                let join_set_to_resps = this.responses.entry(execution_id.clone()).or_default();
                for response in new_responses {
                    let response = response
                        .event
                        .expect("`event` is sent in `ResponseWithCursor`");
                    let join_set_id = response
                        .join_set_id
                        .clone()
                        .expect("`join_set_id` is sent in `JoinSetResponseEvent`");
                    let execution_responses = join_set_to_resps.entry(join_set_id).or_default();
                    execution_responses.push(response);
                }
                let new_fetch_state = if is_finished {
                    ExecutionFetchState::Finished
                } else {
                    ExecutionFetchState::Pending
                    // Will be followed by ExecutionFetchState::Requested
                };
                this.execution_ids_to_fetch_state
                    .insert(execution_id, new_fetch_state);
                Rc::from(this)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SourceCodeState {
    Added,
    InFlight,
    Found(Rc<[(Html, usize /* line */)]>), // Array of lines + line numbers
    NotFoundOrErr,
}

type SourceKey = (ComponentId, String /* file name */);

#[derive(Default, PartialEq)]
struct SourcesState(HashMap<SourceKey, SourceCodeState>);
struct SourcesStateAction {
    key: SourceKey,
    value: SourceCodeState,
    trace_id: Rc<str>,
}
impl Reducible for SourcesState {
    type Action = SourcesStateAction;

    fn reduce(
        self: Rc<Self>,
        SourcesStateAction {
            key,
            value,
            trace_id,
        }: Self::Action,
    ) -> Rc<Self> {
        if value == SourceCodeState::Added && self.0.contains_key(&key) {
            // Do not readd the same entry.
            return self;
        }
        let mut next_map = self.0.clone();
        let old = next_map.insert(key.clone(), value.clone());
        debug!("[{trace_id}] Updated {key:?} from {old:?} to {value:?}");
        Self(next_map).into()
    }
}

#[function_component(DebuggerView)]
pub fn debugger_view(
    DebuggerViewProps {
        execution_id,
        versions,
    }: &DebuggerViewProps,
) -> Html {
    let debugger_state = use_reducer_eq(DebuggerState::default);

    // Fill the current execution id and its parent
    use_effect_with(execution_id.clone(), {
        let debugger_state = debugger_state.clone();
        move |execution_id| {
            debugger_state.dispatch(DebuggerStateAction::AddExecutionId(execution_id.clone()));
            if let Some(parent_id) = execution_id.parent_id() {
                debugger_state.dispatch(DebuggerStateAction::AddExecutionId(parent_id));
            }
        }
    });

    use_effect_with(debugger_state.clone(), on_state_change);

    let backtraces_state: UseStateHandle<
        HashMap<(ExecutionId, VersionType), GetBacktraceResponse>,
    > = use_state(Default::default);
    let sources_state = use_reducer_eq(SourcesState::default);
    let version = versions.last();
    use_effect_with((execution_id.clone(), version), {
        let hook_id = trace_id();
        let backtraces_state = backtraces_state.clone();
        let sources_state = sources_state.clone(); // Write a request to obtain the sources.
        move |(execution_id, version)| {
            let execution_id = execution_id.clone();
            let version = *version;
            if backtraces_state.contains_key(&(execution_id.clone(), version)) {
                trace!("[{hook_id}] Prevented GetBacktrace fetch");
                return;
            }
            wasm_bindgen_futures::spawn_local(async move {
                trace!("[{hook_id}] GetBacktraceRequest {execution_id} {version:?}");
                let mut execution_client =
                    grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                        tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                    );
                let backtrace_response = execution_client
                    .get_backtrace(tonic::Request::new(grpc_client::GetBacktraceRequest {
                        execution_id: Some(execution_id.clone()),
                        filter: Some(if version > 0 {
                            get_backtrace_request::Filter::Specific(
                                get_backtrace_request::Specific { version },
                            )
                        } else {
                            get_backtrace_request::Filter::First(get_backtrace_request::First {})
                        }),
                    }))
                    .await;
                let backtrace_response = match backtrace_response {
                    Err(status) if status.code() == tonic::Code::NotFound => return,
                    Ok(ok) => ok.into_inner(),
                    err @ Err(_) => panic!("{err:?}"),
                };
                trace!("[{hook_id}] Got backtrace_response {backtrace_response:?}");
                let component_id = backtrace_response
                    .component_id
                    .clone()
                    .expect("GetBacktraceResponse.component_id is sent");
                for file in backtrace_response
                    .wasm_backtrace
                    .as_ref()
                    .expect("GetBacktraceResponse.wasm_backtrace is sent")
                    .frames
                    .iter()
                    .flat_map(|frame| frame.symbols.iter())
                    .filter_map(|frame_symbol| frame_symbol.file.as_ref())
                {
                    let key = (component_id.clone(), file.clone());
                    sources_state.dispatch(SourcesStateAction {
                        key,
                        value: SourceCodeState::Added,
                        trace_id: hook_id.clone(),
                    });
                }

                let mut backtraces: HashMap<_, _> = backtraces_state.deref().clone();
                backtraces.insert((execution_id, version), backtrace_response);
                backtraces_state.set(backtraces);
            });
        }
    });

    use_effect_with(sources_state.clone(), {
        move |sources_state| {
            let hook_id = trace_id();
            debug!("[{hook_id}] sources_state hook started");

            for (key, _state) in sources_state
                .deref()
                .0
                .iter()
                .filter(|(_key, state)| **state == SourceCodeState::Added)
            {
                sources_state.dispatch(SourcesStateAction {
                    key: key.clone(),
                    value: SourceCodeState::InFlight,
                    trace_id: hook_id.clone(),
                });
                let trace_id = Rc::from(format!("{hook_id} {}", trace_id()));
                let component_id = key.0.clone();
                let file = key.1.clone();
                let sources_state = sources_state.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    trace!("[{trace_id}] `GetBacktraceSourceRequest` start {component_id} {file}");
                    let mut execution_client =
                        grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                        );
                    let backtrace_src_response = execution_client
                        .get_backtrace_source(tonic::Request::new(GetBacktraceSourceRequest {
                            component_id: Some(component_id.clone()),
                            file: file.clone(),
                        }))
                        .await;
                    let source_code_state = match backtrace_src_response {
                        Err(err) => {
                            log::warn!("[{trace_id}] Cannot obtain source `{file}` - {err:?}");
                            SourceCodeState::NotFoundOrErr
                        }
                        Ok(ok) => {
                            let language = PathBuf::from(&file)
                                .extension()
                                .map(|e| e.to_string_lossy().to_string());
                            SourceCodeState::Found(Rc::from(highlight_code_line_by_line(
                                &ok.into_inner().content,
                                language.as_deref(),
                            )))
                        }
                    };
                    debug!(
                        "[{trace_id}] GetBacktraceSourceRequest inserting {component_id}  {file} = {source_code_state:?}",
                    );
                    sources_state.dispatch(SourcesStateAction {
                        key: (component_id, file),
                        value: source_code_state,
                        trace_id,
                    });
                });
            }
        }
    });

    let dummy_events = Vec::new();
    let events = debugger_state
        .events
        .get(execution_id)
        .unwrap_or(&dummy_events);
    let dummy_response_map = HashMap::new();
    let responses = debugger_state
        .responses
        .get(execution_id)
        .unwrap_or(&dummy_response_map);

    let join_next_version_to_response = compute_join_next_to_response(events, responses);
    let backtrace_response = backtraces_state
        .deref()
        .get(&(execution_id.clone(), version));
    let execution_log = events
        .iter()
        .filter(|event| {
            let event_inner = event.event.as_ref().expect("event is sent by the server");
            matches!(
                event_inner,
                execution_event::Event::Created(_) | execution_event::Event::Finished(_)
            ) || event.backtrace_id.is_some()
        })
        .map(|event| {
            event_to_detail(
                execution_id,
                event,
                &join_next_version_to_response,
                ExecutionLink::Debug,
                // is_selected
                backtrace_response
                    .and_then(|br| br.wasm_backtrace.as_ref())
                    .map(|b| {
                        b.version_min_including <= event.version
                            && b.version_max_excluding > event.version
                    })
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    let is_finished = matches!(
        events.last(),
        Some(ExecutionEvent {
            event: Some(execution_event::Event::Finished(_)),
            ..
        })
    );

    // Step Out Calculation
    let step_out = if let Some(parent_id) = execution_id.parent_id() {
        let (parent_version_created, parent_version_consumed) =
            get_parent_execution_bounds(&debugger_state, &parent_id, execution_id);

        let parent_versions_path = versions.step_out().unwrap_or_default();
        let requested_parent_version = parent_versions_path.last();
        match (parent_version_created, parent_version_consumed) {
            (Some(start), Some(end)) if start + 1 == end => {
                // Merge into one button
                html! {
                    <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path.change(start) }}>
                        {"Step Out"}
                    </Link<Route>>
                }
            }
            (Some(start), maybe_end) => {
                html! {<>
                    <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path.change(start) }}
                            classes={if start == requested_parent_version { "bold" } else { "" }}
                    >
                        {"Step Out (Start)"}
                    </Link<Route>>
                    if let Some(end) = maybe_end {
                        <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path.change(end) }}
                                classes={if end == requested_parent_version { "bold" } else { "" }}
                        >
                            {"Step Out (End)"}
                        </Link<Route>>
                    }
                </>}
            }
            _ => {
                // Use the backtrace path
                html! {
                    <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path }}>
                        {"Step Out"}
                    </Link<Route>>
                }
            }
        }
    } else {
        html! {
            <span class="disabled">
                {"Step Out"}
            </span>
        }
    };

    let backtrace = if let Some(backtrace_response) = backtrace_response {
        let mut htmls = Vec::new();
        let mut seen_positions = hashbrown::HashSet::new();
        let wasm_backtrace = backtrace_response
            .wasm_backtrace
            .as_ref()
            .expect("`wasm_backtrace` is sent");
        let component_id = backtrace_response
            .component_id
            .as_ref()
            .expect("`GetBacktraceResponse.component_id` is sent");

        // Add Step Prev, Next, Into
        let backtrace_versions: BTreeSet<VersionType> = events
            .iter()
            .filter_map(|event| event.backtrace_id)
            .collect();
        htmls.push(if let Some(backtrace_prev) = backtrace_versions
            .range(..wasm_backtrace.version_min_including)
            .next_back()
            .copied()
        {
            let versions = versions.change(backtrace_prev);
            html! {
                <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: execution_id.clone(), versions } }>
                    {"Step Prev"}
                </Link<Route>>
            }
        } else {
            html! {
                <span class="disabled">
                 {"Step Prev"}
                </span>
            }
        });
        htmls.push(if let Some(backtrace_next) = backtrace_versions
            .range(wasm_backtrace.version_max_excluding..)
            .next()
            .copied()
        {
            let versions = versions.change(backtrace_next);
            html! {
                <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: execution_id.clone(), versions } }>
                    {"Step Next"}
                </Link<Route>>
            }
        } else {
            html! {
                <span class="disabled">
                    {"Step Next"}
                </span>
            }
        });

        // Step Into
        let version_child_request =
            if wasm_backtrace.version_max_excluding - wasm_backtrace.version_min_including == 3 {
                // only happens on one-off join sets where 3 events share the same backtrace.
                wasm_backtrace.version_min_including + 1
            } else {
                wasm_backtrace.version_min_including
            };
        htmls.push(match events.get(usize::try_from(version_child_request).expect("u32 must be convertible to usize")) {
                Some(ExecutionEvent {
                    event:
                        Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                            event: Some(
                                history_event::Event::JoinSetRequest(history_event::JoinSetRequest{join_set_request: Some(history_event::join_set_request::JoinSetRequest::ChildExecutionRequest(
                                    history_event::join_set_request::ChildExecutionRequest{child_execution_id: Some(child_execution_id)}
                                )
                            ), ..
                            })),
                        })),
                    ..
                }) => {
                    let versions = versions.step_into();
                    html!{
                        <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: child_execution_id.clone(), versions } }>
                            {"Step Into"}
                        </Link<Route>>
                    }
                },

                Some(event@ExecutionEvent {
                    event:
                        Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                            event: Some(
                                history_event::Event::JoinNext(..)),
                        })),
                    ..
                }) => {
                    if let Some(JoinSetResponseEvent { response: Some(join_set_response_event::Response::ChildExecutionFinished(join_set_response_event::ChildExecutionFinished{
                        child_execution_id: Some(child_execution_id), ..
                    })), .. }) = join_next_version_to_response.get(&event.version) {
                        let versions = versions.step_into();
                        html!{
                            <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: child_execution_id.clone(), versions } }>
                               {"Step Into"}
                            </Link<Route>>
                        }
                    } else {
                        Html::default()
                    }
                }

                _ => Html::default()
            }
        );

        htmls.push(html!{
            <p>
                {"Backtrace version: "}
                if wasm_backtrace.version_min_including == wasm_backtrace.version_max_excluding - 1 {
                    {wasm_backtrace.version_min_including}
                } else {
                    {wasm_backtrace.version_min_including}{"-"}{wasm_backtrace.version_max_excluding - 1}
                }
            </p>
        });

        for (i, frame) in wasm_backtrace.frames.iter().enumerate() {
            let mut frame_html = Vec::new();
            frame_html.push(html! {
                {format!("{i}: {}, function: {}", frame.module, frame.func_name)}
            });

            for symbol in &frame.symbols {
                // Print location.
                let location = match (&symbol.file, symbol.line, symbol.col) {
                    (Some(file), Some(line), Some(col)) => format!("{file}:{line}:{col}"),
                    (Some(file), Some(line), None) => format!("{file}:{line}"),
                    (Some(file), None, None) => file.clone(),
                    _ => "unknown location".to_string(),
                };
                let mut line = format!("at {location}");

                // Print function name if it's different from frameinfo
                match &symbol.func_name {
                    Some(func_name) if *func_name != frame.func_name => {
                        line.push_str(&format!(" - {func_name}"));
                    }
                    _ => {}
                }
                frame_html.push(html! {<>
                    <br/>
                    {line}
                </>});

                // Print source file.
                if let (Some(file), Some(line)) = (&symbol.file, symbol.line) {
                    let new_position = seen_positions.insert((file.clone(), line));
                    if new_position
                        && let Some(SourceCodeState::Found(source)) = sources_state
                            .deref()
                            .0
                            .get(&(component_id.clone(), file.clone()))
                    {
                        frame_html.push(html! {<>
                                <br/>
                                <SyntectCodeBlock source={source.clone()} focus_line={Some(line as usize)}/>
                            </>});
                    }
                }
            }
            htmls.push(html! {
                <p>
                    {frame_html}
                </p>
            });
        }
        htmls.to_html()
    } else if is_finished {
        html! {
            <p>
                {"No backtrace found for this execution"}
            </p>
        }
    } else {
        html! {
            <p>
                {"Loading..."}
            </p>
        }
    };

    html! {<>
        <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Debug} />

        <div class="trace-layout-container">
            <div class="trace-view">
                {step_out}
                {backtrace}
            </div>
            <div class="trace-detail">
                {execution_log}
            </div>
        </div>

    </>}
}

fn on_state_change(debugger_state: &UseReducerHandle<DebuggerState>) {
    trace!("Triggered use_effects");
    for (execution_id, cursors) in debugger_state
        .execution_ids_to_fetch_state
        .iter()
        .filter_map(|(id, state)| match state {
            ExecutionFetchState::Requested(cursors) => Some((id, *cursors)),
            ExecutionFetchState::Pending | ExecutionFetchState::Finished => None,
        })
    {
        debugger_state.dispatch(DebuggerStateAction::SetPending(execution_id.clone()));
        let execution_id = execution_id.clone();
        let debugger_state = debugger_state.clone();
        wasm_bindgen_futures::spawn_local(async move {
            trace!("list_execution_events {cursors:?}");
            let mut execution_client =
                grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                    tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                );
            let server_resp = execution_client
                .list_execution_events_and_responses(
                    grpc_client::ListExecutionEventsAndResponsesRequest {
                        execution_id: Some(execution_id.clone()),
                        version_from: cursors.version_from,
                        events_length: PAGE,
                        responses_cursor_from: cursors.responses_cursor_from,
                        responses_length: PAGE,
                        responses_including_cursor: cursors.responses_cursor_from == 0,
                        include_backtrace_id: true,
                    },
                )
                .await
                .unwrap()
                .into_inner();
            debug!(
                "{execution_id} Got {} events, {} responses",
                server_resp.events.len(),
                server_resp.responses.len()
            );

            let last_event = server_resp.events.last();
            let is_finished = matches!(
                last_event.and_then(|e| e.event.as_ref()),
                Some(execution_event::Event::Finished(_))
            );
            let cursors = Cursors {
                version_from: last_event
                    .map(|e| e.version + 1)
                    .unwrap_or(cursors.version_from),
                responses_cursor_from: server_resp
                    .responses
                    .last()
                    .map(|resp| resp.cursor)
                    .unwrap_or(cursors.responses_cursor_from),
            };
            debugger_state.dispatch(DebuggerStateAction::SavePage {
                execution_id: execution_id.clone(),
                new_events: server_resp.events,
                new_responses: server_resp.responses,
                is_finished,
            });
            if !is_finished {
                TimeoutFuture::new(SLEEP_MILLIS).await;
                debugger_state.dispatch(DebuggerStateAction::RequestNextPage {
                    execution_id,
                    cursors,
                });
            };
        });
    }
}

fn get_parent_execution_bounds(
    debugger_state: &DebuggerState,
    parent_id: &ExecutionId,
    execution_id: &ExecutionId,
) -> (Option<u32>, Option<u32>) {
    let parent_events = debugger_state.events.get(parent_id);
    let parent_responses = debugger_state.responses.get(parent_id);

    let (Some(parent_events), Some(parent_responses)) = (parent_events, parent_responses) else {
        return (None, None);
    };

    let join_next_map = compute_join_next_to_response(parent_events, parent_responses);
    let mut start = None;
    let mut end = None;

    for event in parent_events {
        match &event.event {
            // Check Start: JoinSetRequest -> ChildExecutionRequest
            Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                event:
                    Some(history_event::Event::JoinSetRequest(history_event::JoinSetRequest {
                        join_set_request:
                            Some(history_event::join_set_request::JoinSetRequest::ChildExecutionRequest(
                                history_event::join_set_request::ChildExecutionRequest {
                                    child_execution_id: Some(found_id),
                                },
                            )),
                        ..
                    })),
            })) if found_id == execution_id => {
                start = Some(event.version);
            }

            // Check End: JoinNext -> Response == ChildExecutionFinished
            Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                event: Some(history_event::Event::JoinNext(_)),
            })) => {
                if let Some(JoinSetResponseEvent {
                    response:
                        Some(join_set_response_event::Response::ChildExecutionFinished(
                            join_set_response_event::ChildExecutionFinished {
                                child_execution_id: Some(found_id),
                                ..
                            },
                        )),
                    ..
                }) = join_next_map.get(&event.version)
                    && found_id == execution_id {
                        end = Some(event.version);
                        break;
                    }
            }
            _ => {}
        }
    }

    (start, end)
}
