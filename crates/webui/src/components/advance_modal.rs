//! Modal for reviewing captured writes and advancing an execution.
//!
//! Layout: vertically split — left pane lists the captured writes,
//! right pane shows the backtrace + source code for the selected write.

use crate::{
    components::{
        code::syntect_code_block::{
            DEFAULT_CONTEXT_LINES, SyntectCodeBlock, highlight_code_line_by_line,
        },
        execution_detail::{
            finished::attach_result_detail, tree_component::TreeComponent, utils::event_to_detail,
        },
        execution_header::ExecutionLink,
        ffqn_with_links::FfqnWithLinks,
        json_tree::{JsonValue, insert_json_into_tree},
    },
    model::FunctionFqnExt as _,
    tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData},
};
use concepts::{
    ComponentId, ExecutionId, SupportedFunctionReturnValue,
    storage::{
        AppendRequest, CreateRequest, Created, ExecutionEvent, ExecutionRequest, HistoryEvent,
        JoinSetRequest, JoinSetResponseEventOuter, Version,
    },
};
use hashbrown::{HashMap, HashSet};
use log::trace;
use rest_types::{
    backtrace::BacktraceInfoSer,
    executions::RetVal,
    replay::{CapturedWriteSer, CreateRequestSer, StubResponseSer},
};
use std::path::PathBuf;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

// ============================================================================
// Helpers: summarise a CapturedWriteSer for the left-pane list
// ============================================================================

/// One-line label for an `ExecutionRequest` variant.
fn event_type_name(event: &ExecutionRequest) -> &'static str {
    match event {
        ExecutionRequest::Created(_) => "Created",
        ExecutionRequest::Locked(_) => "Locked",
        ExecutionRequest::Unlocked { .. } => "Unlocked",
        ExecutionRequest::TemporarilyFailed { .. } => "TemporarilyFailed",
        ExecutionRequest::TemporarilyTimedOut { .. } => "TemporarilyTimedOut",
        ExecutionRequest::Finished { .. } => "Finished",
        ExecutionRequest::Paused => "Paused",
        ExecutionRequest::Unpaused => "Unpaused",
        ExecutionRequest::CancellationRequested => "CancellationRequested",
        ExecutionRequest::ComponentUpgradeFinished { .. } => "ComponentUpgradeFinished",
        ExecutionRequest::HistoryEvent { event } => match event {
            HistoryEvent::Persist { .. } => "Persist",
            HistoryEvent::JoinSetCreate { .. } => "JoinSetCreated",
            HistoryEvent::JoinSetRequest { .. } => "JoinSetRequest",
            HistoryEvent::JoinNext { .. } => "JoinNext",
            HistoryEvent::JoinNextTooMany { .. } => "JoinNextTooMany",
            HistoryEvent::JoinNextTry { .. } => "JoinNextTry",
            HistoryEvent::Schedule { .. } => "Schedule",
            HistoryEvent::Stub { .. } => "Stub",
        },
    }
}

struct WriteSummary {
    kind: &'static str,
    detail: String,
}

fn summarise_write(
    cw: &CapturedWriteSer,
    child_created: &HashMap<ExecutionId, Created>,
) -> WriteSummary {
    match cw {
        CapturedWriteSer::Append { event, .. } => WriteSummary {
            kind: "Append",
            detail: event_type_name(&event.event).to_string(),
        },
        CapturedWriteSer::AppendBatch { events, .. } => WriteSummary {
            kind: "AppendBatch",
            detail: format!("{} events", events.len()),
        },
        CapturedWriteSer::AppendBatchWithDelayResponse { events, .. } => WriteSummary {
            kind: "AppendBatch+DelayResponse",
            detail: format!("{} events", events.len()),
        },
        CapturedWriteSer::AppendBatchCreateNewExecution {
            events,
            child_requests,
            ..
        } => WriteSummary {
            kind: "AppendBatch+Create",
            detail: format!(
                "{} events + {} child exec",
                events.len(),
                child_requests.len()
            ),
        },
        CapturedWriteSer::AppendStubResponse { response, .. } => {
            let fn_name = stub_child_execution_id(response)
                .and_then(|id| child_created.get(&id))
                .map(|created| created.ffqn.short());
            WriteSummary {
                kind: "AppendStubResponse",
                detail: fn_name.map(|n| format!("`{n}`")).unwrap_or_default(),
            }
        }
        CapturedWriteSer::AppendFinished { retval, .. } => WriteSummary {
            kind: "AppendFinished",
            detail: match retval {
                SupportedFunctionReturnValue::Ok(_) => "OK",
                SupportedFunctionReturnValue::Err(_) => "Error",
                SupportedFunctionReturnValue::ExecutionFailure(_) => "Execution Failure",
            }
            .to_string(),
        },
    }
}

fn stub_child_execution_id(response: &StubResponseSer) -> Option<ExecutionId> {
    response.child_execution_id.parse().ok()
}

/// Requests appended by a captured write along with the version of the first one.
fn append_requests(cw: &CapturedWriteSer) -> (u32, &[AppendRequest]) {
    match cw {
        CapturedWriteSer::Append { version, event, .. } => (*version, std::slice::from_ref(event)),
        CapturedWriteSer::AppendBatch {
            version, events, ..
        }
        | CapturedWriteSer::AppendBatchWithDelayResponse {
            version, events, ..
        }
        | CapturedWriteSer::AppendBatchCreateNewExecution {
            version, events, ..
        }
        | CapturedWriteSer::AppendStubResponse {
            version, events, ..
        } => (*version, events),
        CapturedWriteSer::AppendFinished { version, .. } => (*version, &[]),
    }
}

fn append_requests_mut(cw: &mut CapturedWriteSer) -> &mut [AppendRequest] {
    match cw {
        CapturedWriteSer::Append { event, .. } => std::slice::from_mut(event),
        CapturedWriteSer::AppendBatch { events, .. }
        | CapturedWriteSer::AppendBatchWithDelayResponse { events, .. }
        | CapturedWriteSer::AppendBatchCreateNewExecution { events, .. }
        | CapturedWriteSer::AppendStubResponse { events, .. } => events,
        CapturedWriteSer::AppendFinished { .. } => &mut [],
    }
}

/// Execution events within a captured write, versioned as they will be persisted.
fn iter_events(cw: &CapturedWriteSer) -> impl Iterator<Item = ExecutionEvent> + '_ {
    let (version, requests) = append_requests(cw);
    requests
        .iter()
        .zip(version..)
        .map(|(request, version)| ExecutionEvent {
            created_at: request.created_at,
            event: request.event.clone(),
            backtrace_id: None,
            version: Version::new(version),
        })
}

fn is_delay_request(event: &ExecutionRequest) -> bool {
    matches!(
        event,
        ExecutionRequest::HistoryEvent {
            event: HistoryEvent::JoinSetRequest {
                request: JoinSetRequest::DelayRequest { .. },
                ..
            }
        }
    )
}

fn has_delay_requests(writes: &[CapturedWriteSer]) -> bool {
    writes
        .iter()
        .flat_map(|cw| append_requests(cw).1)
        .any(|request| is_delay_request(&request.event))
}

fn has_child_execution_requests(writes: &[CapturedWriteSer]) -> bool {
    writes.iter().any(|cw| {
        matches!(
            cw,
            CapturedWriteSer::AppendBatchCreateNewExecution { child_requests, .. } if !child_requests.is_empty()
        )
    })
}

/// Set `paused = true` on a DelayRequest event if it is one.
fn set_delay_paused(event: &mut ExecutionRequest) {
    if let ExecutionRequest::HistoryEvent {
        event:
            HistoryEvent::JoinSetRequest {
                request: JoinSetRequest::DelayRequest { paused, .. },
                ..
            },
    } = event
    {
        *paused = true;
    }
}

/// Clone captured writes with pause flags applied.
fn apply_pause_flags(
    writes: &[CapturedWriteSer],
    pause_delays: bool,
    pause_executions: bool,
) -> Vec<CapturedWriteSer> {
    let mut writes = writes.to_vec();
    for cw in &mut writes {
        if pause_delays {
            for request in append_requests_mut(cw) {
                set_delay_paused(&mut request.event);
            }
        }
        if pause_executions
            && let CapturedWriteSer::AppendBatchCreateNewExecution { child_requests, .. } = cw
        {
            for req in child_requests {
                req.paused = true;
            }
        }
    }
    writes
}

/// Convert a captured child execution request to its `Created` event.
fn create_req_to_created(req: &CreateRequestSer) -> Option<(ExecutionId, Created)> {
    let req = CreateRequest::try_from(req.clone())
        .inspect_err(|err| trace!("Cannot parse captured child execution request: {err}"))
        .ok()?;
    Some((
        req.execution_id,
        Created {
            ffqn: req.ffqn,
            params: req.params,
            parent: req.parent,
            scheduled_at: req.scheduled_at,
            component_id: req.component_id,
            deployment_id: req.deployment_id,
            metadata: req.metadata,
            scheduled_by: None,
            max_persisted_value_size_bytes: req.max_persisted_value_size_bytes,
        },
    ))
}

/// Build a map from child execution ID to its Created event from captured writes.
fn child_created_from_writes(writes: &[CapturedWriteSer]) -> HashMap<ExecutionId, Created> {
    writes
        .iter()
        .filter_map(|cw| match cw {
            CapturedWriteSer::AppendBatchCreateNewExecution { child_requests, .. } => {
                Some(child_requests)
            }
            _ => None,
        })
        .flatten()
        .filter_map(create_req_to_created)
        .collect()
}

/// Collect child execution IDs referenced by `AppendStubResponse` writes and
/// `ChildExecutionRequest` events. Their `Created` event (function name + params) may
/// have been written in an earlier, already-applied batch, so it won't be present in
/// `child_created_from_writes` for the currently displayed writes.
fn referenced_child_execution_ids(writes: &[CapturedWriteSer]) -> HashSet<ExecutionId> {
    let mut ids = HashSet::new();
    for cw in writes {
        if let CapturedWriteSer::AppendStubResponse { response, .. } = cw
            && let Some(id) = stub_child_execution_id(response)
        {
            ids.insert(id);
        }
        for request in append_requests(cw).1 {
            if let ExecutionRequest::HistoryEvent {
                event:
                    HistoryEvent::JoinSetRequest {
                        request:
                            JoinSetRequest::ChildExecutionRequest {
                                child_execution_id, ..
                            },
                        ..
                    },
            } = &request.event
            {
                ids.insert(ExecutionId::Derived(child_execution_id.clone()));
            }
        }
    }
    ids
}

/// Build a tree displaying AppendStubResponse metadata: function name, params, parent execution.
fn stub_response_tree(
    stub: &StubResponseSer,
    child_created: &HashMap<ExecutionId, Created>,
    app_state: &crate::app::AppState,
    link: ExecutionLink,
) -> TreeData<u32> {
    let mut tree = TreeBuilder::new().build();
    let root_id = tree
        .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
        .unwrap();

    // Function name + params from the child's Created event
    if let Some(created) = stub_child_execution_id(stub).and_then(|id| child_created.get(&id)) {
        let ffqn = created.ffqn.clone();
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Function,
                label: html! { <FfqnWithLinks ffqn={ffqn.clone()} fully_qualified={true} /> },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&root_id),
        )
        .unwrap();

        if let Some(raw_params) = created.params.as_json_values() {
            let params: Vec<(String, serde_json::Value)> = match app_state
                .ffqns_to_details
                .get(&ffqn)
            {
                Some((function_detail, _))
                    if function_detail.parameter_types.len() == raw_params.len() =>
                {
                    function_detail
                        .parameter_types
                        .iter()
                        .zip(raw_params.iter())
                        .map(|(fn_param, param_value)| (fn_param.name.clone(), param_value.clone()))
                        .collect()
                }
                _ => raw_params
                    .iter()
                    .map(|v| ("(unknown)".to_string(), v.clone()))
                    .collect(),
            };
            let params_node_id = tree
                .insert(
                    Node::new(NodeData {
                        icon: Icon::FolderClose,
                        label: "Parameters".into(),
                        has_caret: true,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&root_id),
                )
                .unwrap();
            for (param_name, param_value) in params {
                let param_name_node = tree
                    .insert(
                        Node::new(NodeData {
                            icon: Icon::Function,
                            label: format!("{param_name}: {param_value}").into(),
                            has_caret: true,
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&params_node_id),
                    )
                    .unwrap();
                let _ = insert_json_into_tree(
                    &mut tree,
                    &param_name_node,
                    JsonValue::Parsed(&param_value),
                );
            }
        }
    }

    // Parent execution link
    if let Ok(parent_id) = stub.parent_execution_id.parse::<ExecutionId>() {
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Flows,
                label: html! { <>
                    {"Parent: "}
                    { link.link(parent_id.clone(), &parent_id.to_string()) }
                </> },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&root_id),
        )
        .unwrap();
    }

    TreeData::from(tree)
}

/// Build a tree displaying the finished result value.
fn finished_tree(retval: &SupportedFunctionReturnValue) -> TreeData<u32> {
    let mut tree = TreeBuilder::new().build();
    let root_id = tree
        .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
        .unwrap();
    attach_result_detail(
        &mut tree,
        &root_id,
        &RetVal::from(retval.clone()),
        None,
        false,
    );
    TreeData::from(tree)
}

/// Extract backtraces from a captured write.
fn backtraces_of(cw: &CapturedWriteSer) -> &[BacktraceInfoSer] {
    match cw {
        CapturedWriteSer::Append { backtraces, .. }
        | CapturedWriteSer::AppendBatch { backtraces, .. }
        | CapturedWriteSer::AppendBatchWithDelayResponse { backtraces, .. }
        | CapturedWriteSer::AppendBatchCreateNewExecution { backtraces, .. }
        | CapturedWriteSer::AppendStubResponse { backtraces, .. } => backtraces,
        CapturedWriteSer::AppendFinished { .. } => &[],
    }
}

// ============================================================================
// Source cache (keyed by ComponentId + file path)
// ============================================================================

type SourceKey = (ComponentId, String);

#[derive(Clone, PartialEq)]
enum SourceState {
    Requested,
    InFlight,
    Found(Rc<[(Html, usize)]>),
    NotFound,
}

// ============================================================================
// AdvanceModal component
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct AdvanceModalProps {
    pub execution_id: ExecutionId,
    pub captured_writes: Vec<CapturedWriteSer>,
    pub is_blocked: bool,
    pub on_advance: Callback<Vec<CapturedWriteSer>>,
    pub on_unpause: Callback<()>,
    pub on_close: Callback<()>,
}

#[component(AdvanceModal)]
pub fn advance_modal(props: &AdvanceModalProps) -> Html {
    let selected_idx = use_state(|| 0usize);
    let advancing = use_state(|| false);
    let pause_delays = use_state(|| false);
    let pause_executions = use_state(|| false);

    let app_state = use_context::<crate::app::AppState>()
        .expect("AppState context is set when starting the App");
    let expanded_writes = use_state(HashSet::<usize>::new);

    // Created events for children referenced by this batch (e.g. via `AppendStubResponse` or
    // `ChildExecutionRequest`) but created in an earlier, already-applied batch. Fetched lazily
    // since `child_created_from_writes` only sees children created within the current batch.
    let fetched_child_created = use_state(HashMap::<ExecutionId, Created>::new);
    let child_created_requested = use_state(HashSet::<ExecutionId>::new);

    {
        let fetched_child_created = fetched_child_created.clone();
        let child_created_requested = child_created_requested.clone();
        use_effect_with(props.captured_writes.clone(), move |captured_writes| {
            let local = child_created_from_writes(captured_writes);
            let needed: Vec<ExecutionId> = referenced_child_execution_ids(captured_writes)
                .into_iter()
                .filter(|id| !local.contains_key(id) && !child_created_requested.contains(id))
                .collect();
            if !needed.is_empty() {
                let mut requested = (*child_created_requested).clone();
                for id in &needed {
                    requested.insert(id.clone());
                }
                child_created_requested.set(requested);

                for id in needed {
                    let fetched_child_created = fetched_child_created.clone();
                    spawn_local(async move {
                        let result = crate::rest::events::child_created(&id).await;
                        match result {
                            Ok(created) => {
                                if let Some(created) = created {
                                    let mut next = (*fetched_child_created).clone();
                                    next.insert(id, created);
                                    fetched_child_created.set(next);
                                }
                            }
                            Err(err) => {
                                trace!("Cannot obtain child Created event for {id}: {err:?}");
                            }
                        }
                    });
                }
            }
        });
    }

    let has_delays = has_delay_requests(&props.captured_writes);
    let has_child_execs = has_child_execution_requests(&props.captured_writes);

    // Reset selection and advancing state when captured_writes change (e.g. after re-replay)
    {
        let selected_idx = selected_idx.clone();
        let advancing = advancing.clone();
        use_effect_with(props.captured_writes.clone(), move |_| {
            selected_idx.set(0);
            advancing.set(false);
        });
    }

    // Source cache: ComponentId+file -> highlighted source
    let sources = use_state(HashMap::<SourceKey, SourceState>::new);
    // Expansion state for code blocks, keyed by "component:file"
    let expansion_map = use_state(HashMap::<String, (usize, usize)>::new);

    // Fetch sources for the currently selected write's backtraces
    let selected_backtraces: Vec<BacktraceInfoSer> = props
        .captured_writes
        .get(*selected_idx)
        .map(backtraces_of)
        .unwrap_or(&[])
        .to_vec();

    // Trigger source fetches for any new files in the selected backtraces
    {
        let sources = sources.clone();
        let selected_backtraces = selected_backtraces.clone();
        use_effect_with((*selected_idx, selected_backtraces.clone()), move |_| {
            let mut needed: Vec<(ComponentId, String)> = Vec::new();
            for bt in &selected_backtraces {
                let component_id = &bt.component_id;
                for frame in &bt.wasm_backtrace.frames {
                    for symbol in &frame.symbols {
                        if let Some(file) = &symbol.file {
                            let key = (component_id.clone(), file.clone());
                            if !sources.contains_key(&key) {
                                needed.push(key);
                            }
                        }
                    }
                }
            }

            if !needed.is_empty() {
                // Mark all as Requested
                let mut next = (*sources).clone();
                for key in &needed {
                    next.entry(key.clone()).or_insert(SourceState::Requested);
                }
                sources.set(next);

                // Fetch each
                for (component_id, file) in needed {
                    let sources = sources.clone();
                    spawn_local(async move {
                        // Mark InFlight
                        {
                            let mut next = (*sources).clone();
                            next.insert(
                                (component_id.clone(), file.clone()),
                                SourceState::InFlight,
                            );
                            sources.set(next);
                        }

                        let result =
                            crate::rest::deployments::component_source(&component_id, &file).await;

                        let state = match result {
                            Ok(content) => {
                                let language = PathBuf::from(&file)
                                    .extension()
                                    .map(|e| e.to_string_lossy().to_string());
                                SourceState::Found(Rc::from(highlight_code_line_by_line(
                                    &content,
                                    language.as_deref(),
                                )))
                            }
                            Err(err) => {
                                trace!("Cannot obtain source {file}: {err:?}");
                                SourceState::NotFound
                            }
                        };

                        let mut next = (*sources).clone();
                        next.insert((component_id, file), state);
                        sources.set(next);
                    });
                }
            }
        });
    }

    // Dismiss modal on Escape key
    {
        let on_close = props.on_close.clone();
        use_effect(move || {
            let closure =
                Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                    if e.key() == "Escape" {
                        on_close.emit(());
                    }
                });
            let window = web_sys::window().expect("window should exist");
            window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                .expect("failed to add keydown listener");
            move || {
                window
                    .remove_event_listener_with_callback(
                        "keydown",
                        closure.as_ref().unchecked_ref(),
                    )
                    .expect("failed to remove keydown listener");
            }
        });
    }

    let empty_join_next: HashMap<u32, &JoinSetResponseEventOuter> = HashMap::new();
    // Local batch data takes precedence; fall back to lazily fetched child Created events.
    let child_created: HashMap<ExecutionId, Created> = fetched_child_created
        .iter()
        .map(|(id, created)| (id.clone(), created.clone()))
        .chain(child_created_from_writes(&props.captured_writes))
        .collect();

    // Left pane: list of captured writes
    let write_list = props
        .captured_writes
        .iter()
        .enumerate()
        .map(|(idx, cw)| {
            let summary = summarise_write(cw, &child_created);
            let is_selected = idx == *selected_idx;
            let is_expanded = expanded_writes.contains(&idx);
            let class = classes!("captured-write-item", is_selected.then_some("selected"),);
            let on_click = {
                let selected_idx = selected_idx.clone();
                Callback::from(move |_: MouseEvent| {
                    selected_idx.set(idx);
                })
            };
            let events: Vec<_> = iter_events(cw).collect();
            let has_write_metadata = matches!(
                cw,
                CapturedWriteSer::AppendStubResponse { .. }
                    | CapturedWriteSer::AppendFinished { .. }
            );
            let is_expandable = !events.is_empty() || has_write_metadata;
            let on_toggle_expand = {
                let expanded_writes = expanded_writes.clone();
                Callback::from(move |e: MouseEvent| {
                    e.stop_propagation();
                    let mut next = (*expanded_writes).clone();
                    if next.contains(&idx) {
                        next.remove(&idx);
                    } else {
                        next.insert(idx);
                    }
                    expanded_writes.set(next);
                })
            };
            let event_details = if is_expanded {
                let write_tree = match cw {
                    CapturedWriteSer::AppendStubResponse { response, .. } => {
                        let tree = stub_response_tree(
                            response,
                            &child_created,
                            &app_state,
                            ExecutionLink::ExecutionLog,
                        );
                        html! { <TreeComponent {tree} /> }
                    }
                    CapturedWriteSer::AppendFinished { retval, .. } => {
                        let tree = finished_tree(retval);
                        html! { <TreeComponent {tree} /> }
                    }
                    _ => html! {},
                };
                html! {
                    <div class="captured-write-events">
                        {write_tree}
                        { for events.iter().map(|event| {
                            event_to_detail(
                                event,
                                &empty_join_next,
                                &child_created,
                                ExecutionLink::ExecutionLog,
                                false,
                            )
                        })}
                    </div>
                }
            } else {
                html! {}
            };
            html! {
                <div class="captured-write-wrapper">
                    <div {class} onclick={on_click}>
                        if is_expandable {
                            <span class="captured-write-toggle" onclick={on_toggle_expand}>
                                { if is_expanded { "\u{25BE}" } else { "\u{25B8}" } }
                            </span>
                        }
                        <span class="captured-write-kind">{summary.kind}</span>
                        if !summary.detail.is_empty() {
                            <span class="captured-write-detail">{summary.detail}</span>
                        }
                    </div>
                    {event_details}
                </div>
            }
        })
        .collect::<Html>();

    // Right pane: backtrace + source for selected write
    let backtrace_view = if selected_backtraces.is_empty() {
        html! {
            <div class="advance-backtrace">
                <p class="backtrace-empty">{"No backtraces for this write"}</p>
            </div>
        }
    } else {
        let mut seen_positions = hashbrown::HashSet::new();
        html! {
            <div class="advance-backtrace">
                { for selected_backtraces.iter().map(|bt| {
                    let component_id = &bt.component_id;
                    let wasm_bt = &bt.wasm_backtrace;
                    html! {
                        { for wasm_bt.frames.iter().map(|frame| {
                            let mut frame_html: Vec<Html> = Vec::new();
                            for symbol in &frame.symbols {
                                // Location line
                                let location = match (&symbol.file, symbol.line, symbol.col) {
                                    (Some(file), Some(line), Some(col)) => format!("{file}:{line}:{col}"),
                                    (Some(file), Some(line), None) => format!("{file}:{line}"),
                                    (Some(file), None, None) => file.clone(),
                                    _ => "unknown location".to_string(),
                                };
                                let mut loc_str = format!("at {location}");
                                match &symbol.func_name {
                                    Some(func_name) if *func_name != frame.func_name => {
                                        loc_str.push_str(&format!(" - {func_name}"));
                                    }
                                    _ => {}
                                }
                                frame_html.push(html! { <div class="symbol-info">{loc_str}</div> });

                                // Source code block
                                if let (Some(file), Some(line)) = (&symbol.file, symbol.line)
                                {
                                    let new_position = seen_positions.insert((file.clone(), line));
                                    if new_position {
                                        let key = (component_id.clone(), file.clone());
                                        if let Some(SourceState::Found(source)) = sources.get(&key)
                                        {
                                            let map_key = format!("{}:{}", component_id.name, file);
                                            let (cb_above, cb_below) = expansion_map
                                                .get(&map_key)
                                                .copied()
                                                .unwrap_or((
                                                    DEFAULT_CONTEXT_LINES,
                                                    DEFAULT_CONTEXT_LINES,
                                                ));
                                            let on_expand = {
                                                let expansion_map = expansion_map.clone();
                                                let map_key = map_key.clone();
                                                Callback::from(
                                                    move |(new_above, new_below): (
                                                        usize,
                                                        usize,
                                                    )| {
                                                        let mut next =
                                                            (*expansion_map).clone();
                                                        next.insert(
                                                            map_key.clone(),
                                                            (new_above, new_below),
                                                        );
                                                        expansion_map.set(next);
                                                    },
                                                )
                                            };
                                            frame_html.push(html! {
                                                <SyntectCodeBlock
                                                    key={map_key}
                                                    source={source.clone()}
                                                    focus_line={Some(line as usize)}
                                                    lines_above={cb_above}
                                                    lines_below={cb_below}
                                                    on_expand={on_expand}
                                                />
                                            });
                                        }
                                    }
                                }
                            }
                            html! { <div class="frame-container">{frame_html}</div> }
                        })}
                    }
                })}
            </div>
        }
    };

    let on_advance = {
        let captured_writes = props.captured_writes.clone();
        let on_advance = props.on_advance.clone();
        let advancing = advancing.clone();
        let pause_delays = pause_delays.clone();
        let pause_executions = pause_executions.clone();
        Callback::from(move |_: MouseEvent| {
            advancing.set(true);
            let writes = apply_pause_flags(&captured_writes, *pause_delays, *pause_executions);
            on_advance.emit(writes);
        })
    };

    // Close on overlay click (but not on modal body click)
    let on_overlay_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |e: MouseEvent| {
            // Only close if the click target is the overlay itself
            if let Some(target) = e.target_dyn_into::<web_sys::Element>()
                && target
                    .get_attribute("class")
                    .unwrap_or_default()
                    .contains("modal-overlay")
            {
                on_close.emit(());
            }
        })
    };

    let is_advancing = *advancing;

    let on_dismiss = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            on_close.emit(());
        })
    };

    html! {
        <div class="modal-overlay" onclick={on_overlay_click}>
            <div class="modal-window">
                <div class="modal-header">
                    <h3>
                        if props.is_blocked {
                            {"Advance Execution — Blocked"}
                        } else {
                            {format!("Advance Execution — {} writes", props.captured_writes.len())}
                        }
                    </h3>
                    <button class="modal-dismiss" onclick={on_dismiss}>{"×"}</button>
                </div>
                if props.is_blocked {
                    <div class="modal-body">
                        <div class="modal-blocked-status">
                            <p>{"Execution is blocked. Polling for updates..."}</p>
                        </div>
                    </div>
                } else {
                    <div class="modal-body">
                        <div class="modal-pane-left">
                            {write_list}
                        </div>
                        <div class="modal-pane-right">
                            {backtrace_view}
                        </div>
                    </div>
                }
                if !props.is_blocked {
                    <div class="modal-footer">
                        <div class="modal-footer-left">
                            <label class={classes!("modal-checkbox", (!has_delays).then_some("disabled"))}>
                                <input
                                    type="checkbox"
                                    checked={*pause_delays}
                                    disabled={!has_delays}
                                    onchange={
                                        let pause_delays = pause_delays.clone();
                                        Callback::from(move |e: Event| {
                                            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
                                            pause_delays.set(input.checked());
                                        })
                                    }
                                />
                                {" Pause delays"}
                            </label>
                            <label class={classes!("modal-checkbox", (!has_child_execs).then_some("disabled"))}>
                                <input
                                    type="checkbox"
                                    checked={*pause_executions}
                                    disabled={!has_child_execs}
                                    onchange={
                                        let pause_executions = pause_executions.clone();
                                        Callback::from(move |e: Event| {
                                            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
                                            pause_executions.set(input.checked());
                                        })
                                    }
                                />
                                {" Pause executions"}
                            </label>
                        </div>
                        <button
                            class="action-button unpause-button"
                            onclick={
                                let on_unpause = props.on_unpause.clone();
                                Callback::from(move |_: MouseEvent| {
                                    on_unpause.emit(());
                                })
                            }
                            disabled={is_advancing}
                        >
                            {"Unpause"}
                        </button>
                        <button
                            class="action-button advance-button"
                            onclick={on_advance}
                            disabled={is_advancing}
                        >
                            if is_advancing {
                                {"Advancing..."}
                            } else {
                                {"Advance"}
                            }
                        </button>
                    </div>
                }
            </div>
        </div>
    }
}
