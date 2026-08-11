use crate::{
    app::{Route, query::BacktraceVersionsPath},
    components::{
        code::syntect_code_block::{
            DEFAULT_CONTEXT_LINES, SyntectCodeBlock, highlight_code_line_by_line,
        },
        debugger::version_slider::VersionSlider,
        execution_detail::utils::{compute_join_next_to_response, event_to_detail},
        execution_header::{ExecutionHeader, ExecutionLink},
        notification::{Notification, NotificationContext},
        trace::highlight::{BacktraceJump, TraceHighlightJump},
        trace::trace_view::{PAGE, SLEEP_MILLIS, compute_submit_await_version_groups},
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
    Waiting,
    Finished,
}

enum DebuggerStateAction {
    AddExecutionId(ExecutionId),
    Reload,
    SetPending {
        execution_id: ExecutionId,
        generation: u64,
    },
    SavePage {
        execution_id: ExecutionId,
        new_events: Vec<ExecutionEvent>,
        new_responses: Vec<ResponseWithCursor>,
        is_finished: bool,
        generation: u64,
    },
    RequestNextPage {
        execution_id: ExecutionId,
        cursors: Cursors,
        generation: u64,
    },
}

#[derive(Default, Clone, PartialEq)]
struct DebuggerState {
    fetch_generation: u64,
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
            DebuggerStateAction::Reload => {
                let mut this = self.as_ref().clone();
                this.fetch_generation = this.fetch_generation.wrapping_add(1);
                for state in this.execution_ids_to_fetch_state.values_mut() {
                    *state = ExecutionFetchState::Requested(Cursors::default());
                }
                this.events.clear();
                this.responses.clear();
                Rc::from(this)
            }
            DebuggerStateAction::SetPending {
                execution_id,
                generation,
            } => {
                if generation != self.fetch_generation
                    || !matches!(
                        self.execution_ids_to_fetch_state.get(&execution_id),
                        Some(ExecutionFetchState::Requested(_))
                    )
                {
                    return self;
                }
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Pending);
                Rc::from(this)
            }
            DebuggerStateAction::RequestNextPage {
                execution_id,
                cursors,
                generation,
            } => {
                if generation != self.fetch_generation
                    || !matches!(
                        self.execution_ids_to_fetch_state.get(&execution_id),
                        Some(ExecutionFetchState::Waiting)
                    )
                {
                    return self;
                }
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
                generation,
            } => {
                if generation != self.fetch_generation
                    || !matches!(
                        self.execution_ids_to_fetch_state.get(&execution_id),
                        Some(ExecutionFetchState::Pending)
                    )
                {
                    return self;
                }
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
                    ExecutionFetchState::Waiting
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
enum BacktracesStateAction {
    Clear,
    Set {
        key: (ExecutionId, VersionType),
        value: Result<GetBacktraceResponse, BacktraceError>,
        trace_id: Rc<str>,
    },
}
impl Reducible for BacktracesState {
    type Action = BacktracesStateAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            BacktracesStateAction::Clear => Self::default().into(),
            BacktracesStateAction::Set {
                key,
                value,
                trace_id,
            } => {
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
    }
}

#[component(DebuggerView)]
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
    // Expansion state for SyntectCodeBlock instances, keyed by "{exec_id}:{file}:{line}".
    // Persists across backtrace loading (which remounts the child component).
    let expansion_map = use_state(HashMap::<String, (usize, usize)>::new);
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
    let backtraces_reload = use_state(|| 0_u32);

    // 4. Fetch backtraces for ALL items in the ancestry chain
    use_effect_with((ancestry.clone(), *backtraces_reload), {
        let backtraces_state = backtraces_state.clone();
        let sources_state = sources_state.clone();
        let notifications = notifications.clone();
        let hook_id = trace_id();
        move |(ancestry, _reload)| {
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
                            crate::auth::client(),
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
                    backtraces_state.dispatch(BacktracesStateAction::Set {
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
                            crate::auth::client(),
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
    let submit_await_version_groups =
        compute_submit_await_version_groups(leaf_events, leaf_responses);

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
            let detail = event_to_detail(
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
            );
            let trace_jump = submit_await_version_groups
                .contains_key(&event.version)
                .then(|| {
                    html! {
                        <TraceHighlightJump
                            execution_id={execution_id.clone()}
                            version={event.version}
                        />
                    }
                });
            let backtrace_jump = event.backtrace_id.map(|version| {
                html! {
                    <BacktraceJump execution_id={execution_id.clone()} {version} />
                }
            });

            if trace_jump.is_some() || backtrace_jump.is_some() {
                html! {
                    <div class="trace-detail-event">
                        <div class="trace-detail-actions">
                            {trace_jump}
                            {backtrace_jump}
                        </div>
                        {detail}
                    </div>
                }
            } else {
                detail
            }
        })
        .collect::<Vec<_>>();

    // 5. Render Backtrace Stack (Iterate ancestry from Specific -> Parent -> Grandparent)
    let backtrace_view = {
        let mut htmls = Vec::new();

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
                let mut step_into_shown = false;
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
                            step_into_shown = true;
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
                                step_into_shown = true;
                            }
                        }
                        _ => {}
                    }
                }
                if !step_into_shown {
                    step_buttons.push(html! { <span class="disabled">{"Step Into"}</span> });
                }
            } else {
                // If backtrace not loaded yet, placeholder buttons
                step_buttons.push(html! { <span class="disabled">{"Step Prev"}</span> });
                step_buttons.push(html! { <span class="disabled">{"Step Next"}</span> });
                step_buttons.push(html! { <span class="disabled">{"Step Into"}</span> });
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

                    let frame_count = wasm_backtrace.frames.len();
                    html! {
                        wasm_backtrace.frames.iter().enumerate().map(|(i, frame)| {
                            // Index from the outermost (last) frame so that
                            // the index is stable when new inner frames are added (recursion).
                            let frame_idx = frame_count - 1 - i;
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

                                if let (Some(file), Some(line)) = (&symbol.file, symbol.line)
                                    && let Some(SourceCodeState::Found(source)) = sources_state
                                            .deref()
                                            .0
                                            .get(&(component_id.clone(), file.clone()))
                                    {
                                        let map_key = format!("{curr_exec_id}:{file}:{line}:{frame_idx}");
                                        let (cb_lines_above, cb_lines_below) = expansion_map
                                            .get(&map_key)
                                            .copied()
                                            .unwrap_or((
                                                DEFAULT_CONTEXT_LINES,
                                                DEFAULT_CONTEXT_LINES,
                                            ));
                                        let on_expand = {
                                            let expansion_map = expansion_map.clone();
                                            let map_key = map_key.clone();
                                            Callback::from(move |(new_above, new_below): (usize, usize)| {
                                                let mut next = (*expansion_map).clone();
                                                next.insert(map_key.clone(), (new_above, new_below));
                                                expansion_map.set(next);
                                            })
                                        };
                                        frame_html.push(html! {
                                            <SyntectCodeBlock
                                                key={map_key}
                                                source={source.clone()}
                                                focus_line={Some(line as usize)}
                                                lines_above={cb_lines_above}
                                                lines_below={cb_lines_below}
                                                on_expand={on_expand}
                                            />
                                        });
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
                    <div class="debugger-execution-block">
                        <div class="debugger-execution-header">
                            <div class="step debugger-execution-actions">
                                {step_buttons}
                            </div>
                            <div class="debugger-execution-id">
                                {last_id_segment}
                            </div>
                        </div>
                        {step_buttons_content}
                    </div>
            });
        }

        if htmls.is_empty() {
            html! { <p>{"Loading trace..."}</p> }
        } else {
            htmls.into_iter().collect::<Html>()
        }
    };

    let on_advanced = {
        let navigator = navigator.clone();
        let execution_id = execution_id.clone();
        Callback::from(move |version: VersionType| {
            navigator.push(&Route::ExecutionDebuggerWithVersions {
                execution_id: execution_id.clone(),
                versions: BacktraceVersionsPath::from(version),
            });
        })
    };

    let populating_backtraces = use_state(|| false);
    let on_populate_backtraces = {
        let execution_id = execution_id.clone();
        let debugger_state = debugger_state.clone();
        let backtraces_state = backtraces_state.clone();
        let backtraces_reload = backtraces_reload.clone();
        let populating_backtraces = populating_backtraces.clone();
        let notifications = notifications.clone();
        Callback::from(move |_| {
            let execution_id = execution_id.clone();
            let debugger_state = debugger_state.clone();
            let backtraces_state = backtraces_state.clone();
            let backtraces_reload = backtraces_reload.clone();
            let populating_backtraces = populating_backtraces.clone();
            let notifications = notifications.clone();

            populating_backtraces.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let mut client =
                    grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                        crate::auth::client(),
                    );
                match client
                    .persist_execution_backtraces(grpc_client::PersistExecutionBacktracesRequest {
                        execution_id: Some(execution_id.clone()),
                    })
                    .await
                {
                    Ok(response) => {
                        let count = response.into_inner().persisted_backtrace_count;
                        if count == 0 {
                            notifications
                                .push(Notification::info("No new backtraces were persisted"));
                        } else {
                            let suffix = if count == 1 { "" } else { "s" };
                            notifications.push(Notification::success(format!(
                                "Persisted {count} backtrace{suffix}"
                            )));
                        }
                        debugger_state.dispatch(DebuggerStateAction::Reload);
                        backtraces_state.dispatch(BacktracesStateAction::Clear);
                        backtraces_reload.set(backtraces_reload.wrapping_add(1));
                    }
                    Err(err) => {
                        error!("Failed to persist backtraces for {execution_id}: {err:?}");
                        notifications.push(Notification::error(format!(
                            "Failed to populate backtraces: {}",
                            err.message()
                        )));
                    }
                }
                populating_backtraces.set(false);
            });
        })
    };
    let populate_backtraces_action = html! {
        <div class="action-container">
            <button
                class="action-button"
                onclick={on_populate_backtraces}
                disabled={*populating_backtraces}
                title="Replay this execution and persist any missing call-site backtraces"
            >
                if *populating_backtraces {
                    {"Populating..."}
                } else {
                    {"Populate backtraces"}
                }
            </button>
        </div>
    };

    html! {<>
        <ExecutionHeader
            execution_id={execution_id.clone()}
            link={ExecutionLink::Debug}
            on_advanced={on_advanced}
            additional_action={populate_backtraces_action}
        />

        <VersionSlider
            backtrace_versions={leaf_backtrace_versions.clone()}
            selected_version={leaf_version}
            on_version_change={on_version_change}
        />

        <div class="trace-layout-container">
            <div class="trace-view">
                <div class="trace-controls">
                    <input
                        type="checkbox"
                        id="hide-frames"
                        checked={*hide_frames}
                        onclick={on_toggle_frames}
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
            ExecutionFetchState::Pending
            | ExecutionFetchState::Waiting
            | ExecutionFetchState::Finished => None,
        })
    {
        let generation = debugger_state.fetch_generation;
        debugger_state.dispatch(DebuggerStateAction::SetPending {
            execution_id: execution_id.clone(),
            generation,
        });
        let execution_id = execution_id.clone();
        let debugger_state = debugger_state.clone();
        let notifications = notifications.clone();
        wasm_bindgen_futures::spawn_local(async move {
            trace!("list_execution_events {cursors:?}");
            let mut execution_client =
                grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                    crate::auth::client(),
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
                        generation,
                    });
                    if !is_finished {
                        TimeoutFuture::new(SLEEP_MILLIS).await;
                        debugger_state.dispatch(DebuggerStateAction::RequestNextPage {
                            execution_id,
                            cursors,
                            generation,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_page_response_is_ignored() {
        let execution_id = ExecutionId {
            id: "E_test".to_string(),
        };
        let state = Rc::new(DebuggerState::default())
            .reduce(DebuggerStateAction::AddExecutionId(execution_id.clone()))
            .reduce(DebuggerStateAction::SetPending {
                execution_id: execution_id.clone(),
                generation: 0,
            });
        let event = ExecutionEvent {
            version: 2,
            ..Default::default()
        };
        let state = state.reduce(DebuggerStateAction::SavePage {
            execution_id: execution_id.clone(),
            new_events: vec![event.clone()],
            new_responses: Vec::new(),
            is_finished: false,
            generation: 0,
        });
        let state = state.reduce(DebuggerStateAction::SavePage {
            execution_id: execution_id.clone(),
            new_events: vec![event],
            new_responses: Vec::new(),
            is_finished: false,
            generation: 0,
        });

        assert_eq!(state.events[&execution_id].len(), 1);
        assert!(matches!(
            state.execution_ids_to_fetch_state[&execution_id],
            ExecutionFetchState::Waiting
        ));
    }
}
