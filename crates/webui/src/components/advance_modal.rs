//! Modal for reviewing captured writes and advancing an execution.
//!
//! Layout: vertically split — left pane lists the captured writes,
//! right pane shows the backtrace + source code for the selected write.

use crate::{
    BASE_URL,
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
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{
            self, CapturedBacktrace, CapturedWrite, ComponentId, CreateExecutionRequest,
            ExecutionId, GetBacktraceSourceRequest, JoinSetResponseEvent, captured_write,
            execution_event::{self, history_event},
            execution_repository_client::ExecutionRepositoryClient,
        },
    },
    tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData},
};
use hashbrown::{HashMap, HashSet};
use log::trace;
use std::path::PathBuf;
use std::rc::Rc;
use tonic_web_wasm_client::Client;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

// ============================================================================
// Helpers: summarise a CapturedWrite for the left-pane list
// ============================================================================

/// One-line label for an `ExecutionEvent` variant.
fn event_type_name(event: &grpc_client::ExecutionEvent) -> &'static str {
    match &event.event {
        Some(execution_event::Event::Created(_)) => "Created",
        Some(execution_event::Event::Locked(_)) => "Locked",
        Some(execution_event::Event::Unlocked(_)) => "Unlocked",
        Some(execution_event::Event::TemporarilyFailed(_)) => "TemporarilyFailed",
        Some(execution_event::Event::TemporarilyTimedOut(_)) => "TemporarilyTimedOut",
        Some(execution_event::Event::Finished(_)) => "Finished",
        Some(execution_event::Event::Paused(_)) => "Paused",
        Some(execution_event::Event::Unpaused(_)) => "Unpaused",
        Some(execution_event::Event::ComponentUpgraded(_)) => "ComponentUpgraded",
        Some(execution_event::Event::HistoryVariant(h)) => match &h.event {
            Some(history_event::Event::Persist(_)) => "Persist",
            Some(history_event::Event::JoinSetCreated(_)) => "JoinSetCreated",
            Some(history_event::Event::JoinSetRequest(_)) => "JoinSetRequest",
            Some(history_event::Event::JoinNext(_)) => "JoinNext",
            Some(history_event::Event::JoinNextTooMany(_)) => "JoinNextTooMany",
            Some(history_event::Event::JoinNextTry(_)) => "JoinNextTry",
            Some(history_event::Event::Schedule(_)) => "Schedule",
            Some(history_event::Event::Stub(_)) => "Stub",
            None => "History(?)",
        },
        None => "(?)",
    }
}

struct WriteSummary {
    kind: &'static str,
    detail: String,
}

fn summarise_write(
    cw: &CapturedWrite,
    child_created: &HashMap<ExecutionId, execution_event::Created>,
) -> WriteSummary {
    match &cw.write {
        Some(captured_write::Write::Append(a)) => {
            let name = a.event.as_ref().map(event_type_name).unwrap_or("(empty)");
            WriteSummary {
                kind: "Append",
                detail: name.to_string(),
            }
        }
        Some(captured_write::Write::AppendBatch(b)) => WriteSummary {
            kind: "AppendBatch",
            detail: format!("{} events", b.events.len()),
        },
        Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => WriteSummary {
            kind: "AppendBatch+Create",
            detail: format!(
                "{} events + {} child exec",
                b.events.len(),
                b.child_requests.len()
            ),
        },
        Some(captured_write::Write::AppendStubResponse(s)) => {
            let fn_name = s
                .child_execution_id
                .as_ref()
                .and_then(|id| child_created.get(id))
                .and_then(|c| c.function_name.as_ref())
                .map(|f| FunctionFqn::from(f.clone()).short().to_string());
            WriteSummary {
                kind: "AppendStubResponse",
                detail: fn_name.map(|n| format!("`{n}`")).unwrap_or_default(),
            }
        }
        Some(captured_write::Write::AppendFinished(captured_write::AppendFinished {
            event,
            ..
        })) => WriteSummary {
            kind: "AppendFinished",
            detail: match event
                .as_ref()
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .value
                .as_ref()
                .expect("`value` is sent in `execution_event::Finished` message")
            {
                grpc_client::supported_function_result::Value::Ok(_) => "OK",
                grpc_client::supported_function_result::Value::Error(_) => "Error",
                grpc_client::supported_function_result::Value::ExecutionFailure(_) => {
                    "Execution Failure"
                }
            }
            .to_string(),
        },
        None => unreachable!("write is always sent for CapturedWrite"),
    }
}

/// Iterate all execution events within a captured write.
fn iter_events(cw: &CapturedWrite) -> Box<dyn Iterator<Item = &grpc_client::ExecutionEvent> + '_> {
    match &cw.write {
        Some(captured_write::Write::Append(a)) => Box::new(a.event.iter()),
        Some(captured_write::Write::AppendBatch(b)) => Box::new(b.events.iter()),
        Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => Box::new(b.events.iter()),
        Some(captured_write::Write::AppendStubResponse(s)) => Box::new(s.events.iter()),
        _ => Box::new(std::iter::empty()),
    }
}

/// Check whether an event is a DelayRequest.
fn is_delay_request_event(event: &grpc_client::ExecutionEvent) -> bool {
    matches!(
        &event.event,
        Some(execution_event::Event::HistoryVariant(h))
            if matches!(&h.event, Some(history_event::Event::JoinSetRequest(jsr))
                if matches!(&jsr.join_set_request, Some(history_event::join_set_request::JoinSetRequest::DelayRequest(_))))
    )
}

fn has_delay_requests(writes: &[CapturedWrite]) -> bool {
    writes
        .iter()
        .flat_map(iter_events)
        .any(is_delay_request_event)
}

fn has_child_execution_requests(writes: &[CapturedWrite]) -> bool {
    writes.iter().any(|cw| {
        matches!(
            &cw.write,
            Some(captured_write::Write::AppendBatchCreateNewExecution(b)) if !b.child_requests.is_empty()
        )
    })
}

/// Set `paused = true` on a DelayRequest event if it is one.
fn set_delay_paused(event: &mut grpc_client::ExecutionEvent) {
    if let Some(execution_event::Event::HistoryVariant(h)) = &mut event.event
        && let Some(history_event::Event::JoinSetRequest(jsr)) = &mut h.event
        && let Some(history_event::join_set_request::JoinSetRequest::DelayRequest(dr)) =
            &mut jsr.join_set_request
    {
        dr.paused = true;
    }
}

/// Clone captured writes with pause flags applied.
fn apply_pause_flags(
    writes: &[CapturedWrite],
    pause_delays: bool,
    pause_executions: bool,
) -> Vec<CapturedWrite> {
    let mut writes = writes.to_vec();
    for cw in &mut writes {
        match &mut cw.write {
            Some(captured_write::Write::Append(a)) => {
                if pause_delays && let Some(event) = &mut a.event {
                    set_delay_paused(event);
                }
            }
            Some(captured_write::Write::AppendBatch(b)) if pause_delays => {
                for event in &mut b.events {
                    set_delay_paused(event);
                }
            }
            Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => {
                if pause_delays {
                    for event in &mut b.events {
                        set_delay_paused(event);
                    }
                }
                if pause_executions {
                    for req in &mut b.child_requests {
                        req.paused = true;
                    }
                }
            }
            _ => {}
        }
    }
    writes
}

/// Convert a `CreateExecutionRequest` to an `execution_event::Created`.
fn create_req_to_created(req: &CreateExecutionRequest) -> execution_event::Created {
    execution_event::Created {
        function_name: req.function_name.clone(),
        params: req.params.clone(),
        scheduled_at: req.scheduled_at,
        component_id: req.component_id.clone(),
        scheduled_by: None,
        deployment_id: req.deployment_id.clone(),
        parent_execution_id: req.parent_execution_id.clone(),
        parent_join_set_id: req.parent_join_set_id.clone(),
        metadata: req.metadata.clone(),
    }
}

/// Build a map from child execution ID to its Created event from captured writes.
fn child_created_from_writes(
    writes: &[CapturedWrite],
) -> HashMap<ExecutionId, execution_event::Created> {
    let mut map = HashMap::new();
    for cw in writes {
        if let Some(captured_write::Write::AppendBatchCreateNewExecution(b)) = &cw.write {
            for req in &b.child_requests {
                if let Some(id) = &req.execution_id {
                    map.insert(id.clone(), create_req_to_created(req));
                }
            }
        }
    }
    map
}

/// Build a tree displaying AppendStubResponse metadata: function name, params, parent execution.
fn stub_response_tree(
    stub: &captured_write::AppendStubResponse,
    child_created: &HashMap<ExecutionId, execution_event::Created>,
    app_state: &crate::app::AppState,
    link: ExecutionLink,
) -> TreeData<u32> {
    let mut tree = TreeBuilder::new().build();
    let root_id = tree
        .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
        .unwrap();

    // Function name + params from the child's Created event
    if let Some(created) = stub
        .child_execution_id
        .as_ref()
        .and_then(|id| child_created.get(id))
        && let Some(fn_name) = &created.function_name
    {
        let ffqn = FunctionFqn::from(fn_name.clone());
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

        if let Some(params_any) = &created.params
            && let Ok(raw_params) =
                serde_json::from_slice::<Vec<serde_json::Value>>(&params_any.value)
        {
            let params: Vec<(String, serde_json::Value)> = match app_state
                .ffqns_to_details
                .get(&ffqn)
            {
                Some((function_detail, _)) if function_detail.params.len() == raw_params.len() => {
                    function_detail
                        .params
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
    if let Some(parent_id) = &stub.parent_execution_id {
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
fn finished_tree(finished: &execution_event::Finished) -> TreeData<u32> {
    let mut tree = TreeBuilder::new().build();
    let root_id = tree
        .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
        .unwrap();
    if let Some(result) = &finished.value {
        attach_result_detail(&mut tree, &root_id, result, None, false);
    }
    TreeData::from(tree)
}

/// Extract backtraces from a captured write.
fn backtraces_of(cw: &CapturedWrite) -> &[CapturedBacktrace] {
    match &cw.write {
        Some(captured_write::Write::Append(a)) => &a.backtraces,
        Some(captured_write::Write::AppendBatch(b)) => &b.backtraces,
        Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => &b.backtraces,
        Some(captured_write::Write::AppendStubResponse(s)) => &s.backtraces,
        _ => &[],
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
    pub captured_writes: Vec<CapturedWrite>,
    pub is_blocked: bool,
    pub on_advance: Callback<Vec<CapturedWrite>>,
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
    let selected_backtraces: Vec<CapturedBacktrace> = props
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
                let component_id = match &bt.component_id {
                    Some(id) => id.clone(),
                    None => continue,
                };
                let wasm_bt = match &bt.wasm_backtrace {
                    Some(b) => b,
                    None => continue,
                };
                for frame in &wasm_bt.frames {
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

                        let mut client =
                            ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));
                        let result = client
                            .get_backtrace_source(tonic::Request::new(GetBacktraceSourceRequest {
                                component_id: Some(component_id.clone()),
                                file: file.clone(),
                            }))
                            .await;

                        let state = match result {
                            Ok(resp) => {
                                let language = PathBuf::from(&file)
                                    .extension()
                                    .map(|e| e.to_string_lossy().to_string());
                                SourceState::Found(Rc::from(highlight_code_line_by_line(
                                    &resp.into_inner().content,
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

    let empty_join_next: HashMap<u32, &JoinSetResponseEvent> = HashMap::new();
    let child_created = child_created_from_writes(&props.captured_writes);

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
            let events: Vec<_> = iter_events(cw).cloned().collect();
            let has_write_metadata = matches!(
                &cw.write,
                Some(
                    captured_write::Write::AppendStubResponse(_)
                        | captured_write::Write::AppendFinished(_)
                )
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
                let execution_id = &props.execution_id;
                let write_tree = match &cw.write {
                    Some(captured_write::Write::AppendStubResponse(stub)) => {
                        let tree = stub_response_tree(
                            stub,
                            &child_created,
                            &app_state,
                            ExecutionLink::ExecutionLog,
                        );
                        html! { <TreeComponent {tree} /> }
                    }
                    Some(captured_write::Write::AppendFinished(f)) => {
                        if let Some(finished) = &f.event {
                            let tree = finished_tree(finished);
                            html! { <TreeComponent {tree} /> }
                        } else {
                            html! {}
                        }
                    }
                    _ => html! {},
                };
                html! {
                    <div class="captured-write-events">
                        {write_tree}
                        { for events.iter().map(|event| {
                            event_to_detail(
                                execution_id,
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
                    let component_id = bt.component_id.as_ref();
                    let wasm_bt = match &bt.wasm_backtrace {
                        Some(b) => b,
                        None => return html! { <p class="backtrace-empty">{"No backtrace data"}</p> },
                    };
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
                                if let (Some(file), Some(line), Some(comp_id)) =
                                    (&symbol.file, symbol.line, component_id)
                                {
                                    let new_position = seen_positions.insert((file.clone(), line));
                                    if new_position {
                                        let key = (comp_id.clone(), file.clone());
                                        if let Some(SourceState::Found(source)) = sources.get(&key)
                                        {
                                            let map_key = format!("{}:{}", comp_id.name, file);
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
