use crate::{
    BASE_URL,
    app::{Route, query::BacktraceVersionsPath},
    components::{
        code::syntect_code_block::{SyntectCodeBlock, highlight_code_line_by_line},
        debugger::version_slider::VersionSlider,
        execution_detail::utils::{compute_join_next_to_response, event_to_detail},
        execution_header::{ExecutionHeader, ExecutionLink},
        notification::{Notification, NotificationContext},
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
use log::{debug, error, info, trace};
use std::{collections::BTreeSet, ops::Deref as _, path::PathBuf, rc::Rc};
use yew::prelude::*;
use yew_router::prelude::{Link, use_navigator};

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
                    info!(" {execution_id} is being requested");
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
                    info!("{execution_id} is finished loading events and responses");
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
    Requested,
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
        if value == SourceCodeState::Requested && self.0.contains_key(&key) {
            trace!("[{trace_id}] Skipping {key:?}");
            // Do not readd the same entry.
            return self;
        }
        let mut next_map = self.0.clone();
        let old = next_map.insert(key.clone(), value.clone());
        debug!("[{trace_id}] Updated from {old:?} to {value:?} key {key:?}");
        Self(next_map).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BacktraceError {
    NotFound,
    Other,
}

#[derive(Default, PartialEq)]
struct BacktracesState(
    HashMap<(ExecutionId, VersionType), Result<GetBacktraceResponse, BacktraceError>>,
);
struct BacktracesStateAction {
    key: (ExecutionId, VersionType),
    value: Result<GetBacktraceResponse, BacktraceError>,
    trace_id: Rc<str>,
}
impl Reducible for BacktracesState {
    type Action = BacktracesStateAction;

    fn reduce(
        self: Rc<Self>,
        BacktracesStateAction {
            key,
            value,
            trace_id,
        }: Self::Action,
    ) -> Rc<Self> {
        if self.0.contains_key(&key) {
            trace!("[{trace_id}] Skipping {key:?}");
            // Do not readd the same entry.
            return self;
        }
        let mut next_map = self.0.clone();
        let old = next_map.insert(key.clone(), value.clone());
        debug!("[{trace_id}] Updated from {old:?} to {value:?} key {key:?}");
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
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    // 1. Toggle for hiding frame locations
    let hide_frames = use_state(|| true);
    let on_toggle_frames = {
        let hide_frames = hide_frames.clone();
        Callback::from(move |_| hide_frames.set(!*hide_frames))
    };

    // 2. Calculate ancestry chain: [(ExecutionId, VersionType)]
    // Order: Leaf (Current) -> Parent -> Grandparent -> ... -> Root
    let ancestry = {
        let mut curr_id = execution_id.clone();
        let mut curr_ver_path = versions.clone();
        let mut list = vec![(curr_id.clone(), curr_ver_path.clone())];

        while let (Some(id), Some(ver_path)) = (curr_id.parent_id(), curr_ver_path.step_out()) {
            list.push((id.clone(), ver_path.clone()));
            curr_id = id;
            curr_ver_path = ver_path;
        }
        list
    };

    // 3. Register current execution ID + parent (for two step out buttons)
    use_effect_with(execution_id.clone(), {
        let debugger_state = debugger_state.clone();
        move |execution_id| {
            debugger_state.dispatch(DebuggerStateAction::AddExecutionId(execution_id.clone()));
            if let Some(parent_id) = execution_id.parent_id() {
                debugger_state.dispatch(DebuggerStateAction::AddExecutionId(parent_id));
            }
        }
    });

    use_effect_with(
        (debugger_state.clone(), notifications.clone()),
        on_state_change,
    );

    let backtraces_state = use_reducer_eq(BacktracesState::default);
    let sources_state = use_reducer_eq(SourcesState::default);

    // 4. Fetch backtraces for ALL items in the ancestry chain
    use_effect_with(ancestry.clone(), {
        let backtraces_state = backtraces_state.clone();
        let sources_state = sources_state.clone();
        let notifications = notifications.clone();
        let hook_id = trace_id();
        move |ancestry| {
            for (execution_id, versions) in ancestry.iter() {
                let execution_id = execution_id.clone();
                let version = versions.last();

                let backtraces_state = backtraces_state.clone();
                let sources_state = sources_state.clone();
                let notifications = notifications.clone();
                let hook_id = hook_id.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let hook_id: Rc<str> = Rc::from(format!("{hook_id} {}", trace_id()));
                    info!("[{hook_id}] GetBacktraceRequest {execution_id} {version:?}");
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
                                get_backtrace_request::Filter::First(
                                    get_backtrace_request::First {},
                                )
                            }),
                        }))
                        .await;
                    trace!("[{hook_id}] Got backtrace_response {backtrace_response:?}");
                    let backtrace_response = backtrace_response
                        .map(|resp| resp.into_inner())
                        .map_err(|err| {
                            if err.code() == tonic::Code::NotFound {
                                BacktraceError::NotFound
                            } else {
                                error!("Failed to get backtrace: {:?}", err);
                                notifications.push(Notification::error(format!(
                                    "Failed to load backtrace: {}",
                                    err.message()
                                )));
                                BacktraceError::Other
                            }
                        });
                    if let Ok(backtrace_response) = &backtrace_response {
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
                            trace!("[{hook_id}] Requesting file {file}");
                            let key = (component_id.clone(), file.clone());
                            sources_state.dispatch(SourcesStateAction {
                                key,
                                value: SourceCodeState::Requested,
                                trace_id: hook_id.clone(),
                            });
                        }
                    }
                    backtraces_state.dispatch(BacktracesStateAction {
                        key: (execution_id, version),
                        value: backtrace_response,
                        trace_id: hook_id.clone(),
                    });
                });
            }
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
                .filter(|(_key, state)| **state == SourceCodeState::Requested)
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
                            log::info!("[{trace_id}] Cannot obtain source `{file}` - {err:?}");
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
                    sources_state.dispatch(SourcesStateAction {
                        key: (component_id, file),
                        value: source_code_state,
                        trace_id,
                    });
                });
            }
        }
    });

    // Data for the detailed log (Leaf execution only)
    let dummy_events = Vec::new();
    let leaf_events = debugger_state
        .events
        .get(execution_id)
        .unwrap_or(&dummy_events);
    let dummy_response_map = HashMap::new();
    let leaf_responses = debugger_state
        .responses
        .get(execution_id)
        .unwrap_or(&dummy_response_map);
    let join_next_version_to_response = compute_join_next_to_response(leaf_events, leaf_responses);

    // Determine highlighting logic for log based on Leaf backtrace
    let leaf_version = versions.last();
    let leaf_backtrace_response = backtraces_state
        .deref()
        .0
        .get(&(execution_id.clone(), leaf_version));

    // Compute backtrace versions for the slider (leaf execution only)
    let leaf_backtrace_versions: BTreeSet<VersionType> = leaf_events
        .iter()
        .filter_map(|event| event.backtrace_id)
        .collect();

    // Setup navigator for slider navigation
    let navigator = use_navigator().expect("navigator should be available");
    let on_version_change = {
        let navigator = navigator.clone();
        let execution_id = execution_id.clone();
        let versions = versions.clone();
        Callback::from(move |new_version: VersionType| {
            let new_versions = versions.change(new_version);
            navigator.push(&Route::ExecutionDebuggerWithVersions {
                execution_id: execution_id.clone(),
                versions: new_versions,
            });
        })
    };

    let execution_log = leaf_events
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
                &hashbrown::HashMap::new(),
                ExecutionLink::Debug,
                // is_selected
                leaf_backtrace_response
                    .and_then(|result| result.as_ref().map(|ok| ok.wasm_backtrace.as_ref()).ok())
                    .flatten()
                    .map(|b| {
                        b.version_min_including <= event.version
                            && b.version_max_excluding > event.version
                    })
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    // 5. Render Backtrace Stack (Iterate ancestry from Specific -> Parent -> Grandparent)
    let backtrace_view = {
        let mut htmls = Vec::new();
        let mut seen_positions = hashbrown::HashSet::new();

        for (index, (curr_exec_id, curr_path)) in ancestry.iter().enumerate() {
            let is_leaf = index == 0;
            let mut curr_version = curr_path.last();
            let events = debugger_state
                .events
                .get(curr_exec_id)
                .unwrap_or(&dummy_events);

            // Generate Buttons for this specific level
            let mut step_buttons = Vec::new();

            // -- Step Out --
            if let Some(parent_id) = curr_exec_id.parent_id() {
                // If it's the Leaf, use the complex logic
                if is_leaf {
                    let (parent_version_created, parent_version_consumed) =
                        get_parent_execution_bounds(&debugger_state, &parent_id, curr_exec_id);

                    let parent_versions_path = curr_path.step_out().unwrap_or_default();
                    let requested_parent_version = parent_versions_path.last();

                    match (parent_version_created, parent_version_consumed) {
                        (Some(start), Some(end)) if start + 1 == end => {
                            step_buttons.push(html! {
                                <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path.change(start) }}>
                                    {"Step Out"}
                                </Link<Route>>
                            });
                        }
                        (Some(start), maybe_end) => {
                            step_buttons.push(html! {<>
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
                            </>});
                        }
                        _ => {
                            step_buttons.push(html! {
                                <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_versions_path }}>
                                    {"Step Out"}
                                </Link<Route>>
                            });
                        }
                    }
                } else {
                    // Parent / Grandparent: Simple Step Out (just pop the path)
                    if let Some(parent_path) = curr_path.step_out() {
                        step_buttons.push(html! {
                            <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: parent_id.clone(), versions: parent_path }}>
                                {"Step Out"}
                            </Link<Route>>
                        });
                    } else {
                        step_buttons.push(html! { <span class="disabled">{"Step Out"}</span> });
                    }
                }
            } else {
                step_buttons.push(html! { <span class="disabled">{"Step Out"}</span> });
            }

            // -- Step Prev/Next/Into --
            if let Some(Ok(backtrace_response)) = backtraces_state
                .deref()
                .0
                .get(&(curr_exec_id.clone(), curr_version))
            {
                let wasm_backtrace = backtrace_response
                    .wasm_backtrace
                    .as_ref()
                    .expect("`wasm_backtrace` is sent");

                let backtrace_versions: BTreeSet<VersionType> = events
                    .iter()
                    .filter_map(|event| event.backtrace_id)
                    .collect();

                // Prev
                if let Some(backtrace_prev) = backtrace_versions
                    .range(..wasm_backtrace.version_min_including)
                    .next_back()
                    .copied()
                {
                    let versions = curr_path.change(backtrace_prev);
                    step_buttons.push(html! {
                        <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: curr_exec_id.clone(), versions } }>
                            {"Step Prev"}
                        </Link<Route>>
                    });
                } else {
                    step_buttons.push(html! { <span class="disabled">{"Step Prev"}</span> });
                }

                // Next
                if let Some(backtrace_next) = backtrace_versions
                    .range(wasm_backtrace.version_max_excluding..)
                    .next()
                    .copied()
                {
                    let versions = curr_path.change(backtrace_next);
                    step_buttons.push(html! {
                        <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: curr_exec_id.clone(), versions } }>
                            {"Step Next"}
                        </Link<Route>>
                    });
                } else {
                    step_buttons.push(html! { <span class="disabled">{"Step Next"}</span> });
                }

                // Into (Only valid for Leaf)
                if is_leaf {
                    let version_child_request = if wasm_backtrace.version_max_excluding
                        - wasm_backtrace.version_min_including
                        == 3
                    {
                        wasm_backtrace.version_min_including + 1
                    } else {
                        wasm_backtrace.version_min_including
                    };

                    match events.get(usize::try_from(version_child_request).unwrap_or(0)) {
                        Some(ExecutionEvent {
                            event: Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                                event: Some(history_event::Event::JoinSetRequest(history_event::JoinSetRequest{
                                    join_set_request: Some(history_event::join_set_request::JoinSetRequest::ChildExecutionRequest(
                                        history_event::join_set_request::ChildExecutionRequest{child_execution_id: Some(child_execution_id), ..}
                                    ))
                                , ..})),
                            })),
                            ..
                        }) => {
                             let versions = curr_path.step_into();
                             step_buttons.push(html!{
                                <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: child_execution_id.clone(), versions } }>
                                    {"Step Into"}
                                </Link<Route>>
                            });
                        },
                        Some(event@ExecutionEvent {
                            event: Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                                    event: Some(history_event::Event::JoinNext(..)),
                            })),
                            ..
                        }) => {
                             if let Some(JoinSetResponseEvent { response: Some(join_set_response_event::Response::ChildExecutionFinished(join_set_response_event::ChildExecutionFinished{
                                child_execution_id: Some(child_execution_id), ..
                            })), .. }) = join_next_version_to_response.get(&event.version) {
                                let versions = curr_path.step_into();
                                step_buttons.push(html!{
                                    <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: child_execution_id.clone(), versions } }>
                                       {"Step Into"}
                                    </Link<Route>>
                                });
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // If backtrace not loaded yet, placeholder buttons
                step_buttons.push(html! { <span class="disabled">{"Step Prev"}</span> });
                step_buttons.push(html! { <span class="disabled">{"Step Next"}</span> });
            }

            let step_buttons_content = match backtraces_state
                .deref()
                .0
                .get(&(curr_exec_id.clone(), curr_version))
            {
                Some(Ok(backtrace_response)) => {
                    let wasm_backtrace = backtrace_response.wasm_backtrace.as_ref().unwrap();
                    let component_id = backtrace_response.component_id.as_ref().unwrap();
                    if curr_version < wasm_backtrace.version_min_including {
                        // Correct for 0, added by Step Into from parent.
                        curr_version = wasm_backtrace.version_min_including;
                    }

                    html! {
                        wasm_backtrace.frames.iter().enumerate().map(|(i, frame)| {
                            let mut frame_html = Vec::new();
                            if !*hide_frames {
                                    frame_html.push(html! {
                                    <div class="frame-info">
                                        {format!("{i}: {}, function: {}", frame.module, frame.func_name)}
                                    </div>
                                });
                            }

                            for symbol in &frame.symbols {
                                if !*hide_frames {
                                        let location = match (&symbol.file, symbol.line, symbol.col) {
                                        (Some(file), Some(line), Some(col)) => format!("{file}:{line}:{col}"),
                                        (Some(file), Some(line), None) => format!("{file}:{line}"),
                                        (Some(file), None, None) => file.clone(),
                                        _ => "unknown location".to_string(),
                                    };
                                    let mut line = format!("at {location}");
                                    match &symbol.func_name {
                                        Some(func_name) if *func_name != frame.func_name => {
                                            line.push_str(&format!(" - {func_name}"));
                                        }
                                        _ => {}
                                    }
                                    frame_html.push(html! {<div class="symbol-info">{line}</div>});
                                }

                                if let (Some(file), Some(line)) = (&symbol.file, symbol.line) {
                                    let new_position = seen_positions.insert((file.clone(), line));
                                    if new_position
                                        && let Some(SourceCodeState::Found(source)) = sources_state
                                            .deref()
                                            .0
                                            .get(&(component_id.clone(), file.clone()))
                                    {
                                        frame_html.push(html! {
                                            <SyntectCodeBlock source={source.clone()} focus_line={Some(line as usize)}/>
                                        });
                                    }
                                }
                            }
                            html! { <div class="frame-container">{frame_html}</div> }
                        }).collect::<Html>()
                    }
                }
                Some(Err(BacktraceError::NotFound)) => {
                    html! {
                        <p>{format!("Backtrace not found")}</p>
                    }
                }
                Some(Err(BacktraceError::Other)) => {
                    html! {
                        <p>{format!("Loading backtrace failed")}</p>
                    }
                }
                None => {
                    html! {
                        <p>{format!("Loading backtrace...", )}</p>
                    }
                }
            };
            let last_id_segment = curr_exec_id
                .as_hierarchy()
                .pop()
                .map(|(segment, _id)| segment)
                .unwrap();
            htmls.push(html! {
                    <div class="execution-block" style="border: 1px solid #ccc; margin-bottom: 20px; padding: 10px; border-radius: 5px;">
                        <div class="execution-header" style="padding: 5px; margin-bottom: 10px; border-bottom: 1px solid #ddd; display: flex; justify-content: space-between; align-items: center;">
                            <div>
                                {last_id_segment}
                                {" | "}<strong>{"Version: "}</strong>{curr_version}
                            </div>
                            <div class="step">
                                {step_buttons}
                            </div>
                        </div>
                        {step_buttons_content}
                    </div>
            });
        }

        if htmls.is_empty() {
            html! { <p>{"Loading trace..."}</p> }
        } else {
            htmls.to_html()
        }
    };

    html! {<>
        <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Debug} />

        <VersionSlider
            backtrace_versions={leaf_backtrace_versions.clone()}
            selected_version={leaf_version}
            on_version_change={on_version_change}
        />

        <div class="trace-layout-container">
            <div class="trace-view">
                <div class="trace-controls" style="margin-bottom: 10px; text-align: right;">
                    <input
                        type="checkbox"
                        id="hide-frames"
                        checked={*hide_frames}
                        onclick={on_toggle_frames}
                        style="margin-right: 5px;"
                    />
                    <label for="hide-frames">{"Hide locations (source only)"}</label>
                </div>
                {backtrace_view}
            </div>
            <div class="trace-detail">
                {execution_log}
            </div>
        </div>
    </>}
}

fn on_state_change(
    (debugger_state, notifications): &(UseReducerHandle<DebuggerState>, NotificationContext),
) {
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
        let notifications = notifications.clone();
        wasm_bindgen_futures::spawn_local(async move {
            trace!("list_execution_events {cursors:?}");
            let mut execution_client =
                grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                    tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                );
            let response = execution_client
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
                .await;

            match response {
                Ok(resp) => {
                    let server_resp = resp.into_inner();
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
                    }
                }
                Err(e) => {
                    error!("Failed to list execution events: {:?}", e);
                    notifications.push(Notification::error(format!(
                        "Failed to load debugger data: {}",
                        e.message()
                    )));
                }
            }
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
                                    ..
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
