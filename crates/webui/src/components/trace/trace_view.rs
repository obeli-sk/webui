use super::data::{BusyIntervalStatus, TraceData};
use crate::{
    app::Route,
    components::{
        execution_detail::utils::{compute_join_next_to_response, event_to_detail},
        execution_header::{ExecutionHeader, ExecutionLink},
        ffqn_with_links::FfqnWithLinks,
        notification::{Notification, NotificationContext},
        trace::{
            data::{BusyInterval, TraceDataChild, TraceDataRoot, TraceLink},
            execution_trace::{ExecutionTrace, scroll_linked_item, set_linked_highlight},
        },
    },
    grpc::{
        execution_id::EXECUTION_ID_INFIX,
        ffqn::FunctionFqn,
        grpc_client::{
            self, ComponentType, ExecutionEvent, ExecutionId, JoinSetId, JoinSetResponseEvent,
            ResponseWithCursor, SupportedFunctionResult,
            execution_event::{
                self, Finished, TemporarilyFailed, TemporarilyTimedOut,
                history_event::{JoinSetRequest, join_set_request},
            },
            http_client_trace, join_set_response_event, supported_function_result,
        },
        version::VersionType,
    },
    tree::Icon,
};
use assert_matches::assert_matches;
use chrono::{DateTime, Utc};
use gloo::timers::future::TimeoutFuture;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::{ops::Deref as _, rc::Rc};
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq)]
pub struct TraceViewProps {
    pub execution_id: grpc_client::ExecutionId,
}

pub const PAGE: u32 = 500;
pub const SLEEP_MILLIS: u32 = 2500;

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

enum TraceviewStateAction {
    AddExecutionId(ExecutionId),
    SetExpanded {
        node_key: String,
        expanded: bool,
    },
    ToggleExpanded(String),
    // About to fetch the data.
    SetPending(ExecutionId),
    // Got data
    SavePage {
        execution_id: ExecutionId,
        new_events: Vec<ExecutionEvent>,
        new_responses: Vec<ResponseWithCursor>,
        current_status: grpc_client::execution_status::Status,
        is_finished: bool,
    },
    RequestNextPage {
        execution_id: ExecutionId,
        cursors: Cursors,
    },
    SetHideFinished(bool),
    SetShowDelays(bool),
}

#[derive(Clone, PartialEq)]
struct TraceViewState {
    execution_ids_to_fetch_state: HashMap<ExecutionId, ExecutionFetchState>,
    events: HashMap<ExecutionId, Vec<ExecutionEvent>>,
    responses: HashMap<ExecutionId, HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
    statuses: HashMap<ExecutionId, grpc_client::execution_status::Status>,
    expanded_nodes: HashMap<String, bool>,
    hide_finished: bool,
    show_delays: bool,
}
impl Default for TraceViewState {
    fn default() -> Self {
        Self {
            execution_ids_to_fetch_state: HashMap::default(),
            events: HashMap::default(),
            responses: HashMap::default(),
            statuses: HashMap::default(),
            expanded_nodes: HashMap::default(),
            hide_finished: false,
            show_delays: true,
        }
    }
}
impl Reducible for TraceViewState {
    type Action = TraceviewStateAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            TraceviewStateAction::AddExecutionId(execution_id) => {
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
            TraceviewStateAction::SetExpanded { node_key, expanded } => {
                let mut this = self.as_ref().clone();
                this.expanded_nodes.insert(node_key, expanded);
                Rc::from(this)
            }
            TraceviewStateAction::SetPending(execution_id) => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Pending);
                Rc::from(this)
            }
            TraceviewStateAction::RequestNextPage {
                execution_id,
                cursors,
            } => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Requested(cursors));
                Rc::from(this)
            }
            TraceviewStateAction::SavePage {
                execution_id,
                new_events,
                new_responses,
                current_status,
                is_finished: finished,
            } => {
                let mut this = self.as_ref().clone();
                this.statuses.insert(execution_id.clone(), current_status);
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
                let new_fetch_state = if finished {
                    ExecutionFetchState::Finished
                } else {
                    ExecutionFetchState::Pending
                    // Will be followed by ExecutionFetchState::Requested
                };
                this.execution_ids_to_fetch_state
                    .insert(execution_id, new_fetch_state);
                Rc::from(this)
            }
            TraceviewStateAction::ToggleExpanded(node_key) => {
                let mut this = self.as_ref().clone();
                let is_expanded = this.expanded_nodes.get(&node_key).copied().unwrap_or(false);
                this.expanded_nodes.insert(node_key, !is_expanded);
                Rc::from(this)
            }
            TraceviewStateAction::SetHideFinished(hide) => {
                let mut this = self.as_ref().clone();
                this.hide_finished = hide;
                Rc::from(this)
            }
            TraceviewStateAction::SetShowDelays(show) => {
                let mut this = self.as_ref().clone();
                this.show_delays = show;
                Rc::from(this)
            }
        }
    }
}

#[component(TraceView)]
pub fn trace_view(TraceViewProps { execution_id }: &TraceViewProps) -> Html {
    let trace_view_state = use_reducer_eq(TraceViewState::default);
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    // Fill the current execution id
    use_effect_with(execution_id.clone(), {
        let trace_view_state = trace_view_state.clone();
        move |execution_id| {
            trace_view_state.dispatch(TraceviewStateAction::AddExecutionId(execution_id.clone()));
            trace_view_state.dispatch(TraceviewStateAction::SetExpanded {
                node_key: execution_id.to_string(),
                expanded: true,
            });
        }
    });

    use_effect_with(
        (trace_view_state.clone(), notifications.clone()),
        on_state_change,
    );

    let trace_view = trace_view_state.deref();

    // Container to collect IDs that need loading during tree computation
    let missing_executions = use_mut_ref(Vec::new);
    let expandable_missing_children = use_mut_ref(HashMap::<String, Vec<ExecutionId>>::new);
    // Clear previous render's collection
    missing_executions.borrow_mut().clear();
    expandable_missing_children.borrow_mut().clear();

    // Correlate each root submit with the join-next that consumed its result so both the
    // tree node and both detail events can highlight together on hover.
    let version_to_group = {
        let dummy_events = Vec::new();
        let dummy_responses = HashMap::new();
        let events = trace_view.events.get(execution_id).unwrap_or(&dummy_events);
        let responses = trace_view
            .responses
            .get(execution_id)
            .unwrap_or(&dummy_responses);
        compute_version_to_group(events, responses)
    };

    let root_trace = {
        compute_root_trace(
            execution_id,
            true,
            &trace_view.events,
            &trace_view.responses,
            &trace_view.statuses,
            &trace_view_state,
            &mut missing_executions.borrow_mut(),
            &mut expandable_missing_children.borrow_mut(),
            &version_to_group,
        )
    };

    let root_missing_children = expandable_missing_children
        .borrow()
        .get(&execution_id.to_string())
        .cloned()
        .unwrap_or_default();

    use_effect_with(
        (
            is_trace_node_expanded(&trace_view_state, &execution_id.to_string(), false),
            root_missing_children.clone(),
        ),
        {
            let trace_view_state = trace_view_state.clone();
            move |(is_root_expanded, root_missing_children)| {
                if *is_root_expanded {
                    for execution_id in root_missing_children {
                        trace_view_state
                            .dispatch(TraceviewStateAction::AddExecutionId(execution_id.clone()));
                    }
                }
            }
        },
    );

    let execution_log = {
        let all_events = &trace_view.events;
        let dummy_events = Vec::new();
        let events = all_events.get(execution_id).unwrap_or(&dummy_events);
        let dummy_response_map = HashMap::new();
        let responses = &trace_view.responses;
        let responses = responses.get(execution_id).unwrap_or(&dummy_response_map);
        let join_next_version_to_response = compute_join_next_to_response(events, responses);
        // Build map of child execution ID -> Created event from fetched child events
        let child_created_events: hashbrown::HashMap<
            grpc_client::ExecutionId,
            execution_event::Created,
        > = all_events
            .iter()
            .filter(|(id, _)| *id != execution_id)
            .filter_map(|(id, evts)| {
                evts.first().and_then(|e| match &e.event {
                    Some(execution_event::Event::Created(created)) => {
                        Some((id.clone(), created.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        events
            .iter()
            .filter(|event| {
                let event_inner = event.event.as_ref().expect("event is sent by the server");
                !matches!(
                    event_inner,
                    execution_event::Event::Locked(..)
                        | execution_event::Event::Unlocked(..)
                        | execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                            event: Some(execution_event::history_event::Event::JoinSetCreated(_))
                        })
                )
            })
            .map(|event| {
                let detail = event_to_detail(
                    execution_id,
                    event,
                    &join_next_version_to_response,
                    &child_created_events,
                    ExecutionLink::Trace,
                    false,
                );
                // A submit (JoinSetRequest) and its consuming JoinNext both get an id +
                // hover handlers linking them to the node in the trace tree on the left.
                if let Some(group) = version_to_group.get(&event.version) {
                    let version = event.version;
                    let on_enter = {
                        let group = group.clone();
                        let starting_version = group.first().copied();
                        Callback::from(move |_: MouseEvent| {
                            set_linked_highlight(&group, true);
                            if let Some(starting_version) = starting_version {
                                scroll_linked_item(
                                    "trace-tree-pane",
                                    &format!("trace-node-{starting_version}"),
                                );
                            }
                        })
                    };
                    let on_leave = {
                        let group = group.clone();
                        Callback::from(move |_: MouseEvent| set_linked_highlight(&group, false))
                    };
                    html! {
                        <div
                            id={format!("trace-event-{version}")}
                            class="trace-detail-event"
                            onmouseenter={on_enter}
                            onmouseleave={on_leave}
                        >
                            {detail}
                        </div>
                    }
                } else {
                    detail
                }
            })
            .collect::<Vec<_>>()
    };

    let on_hide_finished_change = {
        let trace_view_state = trace_view_state.clone();
        Callback::from(move |e: Event| {
            let target: HtmlInputElement = e.target_unchecked_into();
            trace_view_state.dispatch(TraceviewStateAction::SetHideFinished(!target.checked()));
        })
    };

    let on_show_delays_change = {
        let trace_view_state = trace_view_state.clone();
        Callback::from(move |e: Event| {
            let target: HtmlInputElement = e.target_unchecked_into();
            trace_view_state.dispatch(TraceviewStateAction::SetShowDelays(target.checked()));
        })
    };

    let on_toggle_trace_node = {
        let trace_view_state = trace_view_state.clone();
        let expandable_missing_children = expandable_missing_children.clone();
        Callback::from(move |node_key: String| {
            let should_expand = !is_trace_node_expanded(&trace_view_state, &node_key, false);
            if should_expand {
                let missing_children = expandable_missing_children.borrow();
                if let Some(execution_ids) = missing_children.get(&node_key) {
                    for execution_id in execution_ids {
                        trace_view_state
                            .dispatch(TraceviewStateAction::AddExecutionId(execution_id.clone()));
                    }
                }
            }
            trace_view_state.dispatch(TraceviewStateAction::ToggleExpanded(node_key));
        })
    };

    html! {<>
        <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Trace} />

        <div class="trace-layout-container">
            <div id="trace-tree-pane" class="trace-view">
                <div class="trace-controls" style="margin-bottom: 10px; display: flex; gap: 15px;">
                    <label style="cursor: pointer; user-select: none;">
                        <input
                            type="checkbox"
                            checked={!trace_view.hide_finished}
                            onchange={on_hide_finished_change}
                            style="margin-right: 5px;"
                        />
                        {"Show finished"}
                    </label>
                    <label style="cursor: pointer; user-select: none;">
                        <input
                            type="checkbox"
                            checked={trace_view.show_delays}
                            onchange={on_show_delays_change}
                            style="margin-right: 5px;"
                        />
                        {"Show delays"}
                    </label>
                </div>
                if let Some(root_trace) = root_trace {
                    <ExecutionTrace
                        root_scheduled_at={root_trace.scheduled_at}
                        root_last_event_at={root_trace.last_event_at}
                        data={TraceData::Root(root_trace)}
                        on_toggle={on_toggle_trace_node}
                    />
                } else {
                    {"Loading..."}
                }
            </div>
            <div id="trace-detail-pane" class="trace-detail">
                {execution_log}
            </div>
        </div>

    </>}
}

fn on_state_change(
    (trace_view_state, notifications): &(UseReducerHandle<TraceViewState>, NotificationContext),
) {
    trace!("Triggered use_effects");
    for (execution_id, cursors) in trace_view_state
        .execution_ids_to_fetch_state
        .iter()
        .filter_map(|(id, state)| match state {
            ExecutionFetchState::Requested(cursors) => Some((id, *cursors)),
            ExecutionFetchState::Pending | ExecutionFetchState::Finished => None,
        })
    {
        trace_view_state.dispatch(TraceviewStateAction::SetPending(execution_id.clone()));
        let execution_id = execution_id.clone();
        let trace_view_state = trace_view_state.clone();
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
                    trace_view_state.dispatch(TraceviewStateAction::SavePage {
                        execution_id: execution_id.clone(),
                        new_events: server_resp.events,
                        new_responses: server_resp.responses,
                        current_status: server_resp
                            .current_status
                            .expect("`current_status` is sent")
                            .status
                            .expect("`status` is sent"),
                        is_finished,
                    });
                    if !is_finished {
                        TimeoutFuture::new(SLEEP_MILLIS).await;
                        trace_view_state.dispatch(TraceviewStateAction::RequestNextPage {
                            execution_id,
                            cursors,
                        });
                    }
                }
                Err(e) => {
                    error!("Failed to list execution events: {:?}", e);
                    notifications.push(Notification::error(format!(
                        "Failed to load trace data: {}",
                        e.message()
                    )));
                }
            }
        });
    }
}

/// Group each correlated `Submit`/`JoinNext` version pair (keyed by both versions) so a
/// direct child/delay node and both of its detail events can highlight together on hover.
/// A submit without a resolved join-next maps to a singleton group of just itself.
fn compute_version_to_group(
    events: &[ExecutionEvent],
    responses: &HashMap<JoinSetId, Vec<JoinSetResponseEvent>>,
) -> HashMap<VersionType, Vec<VersionType>> {
    let mut submit_of_child: HashMap<&ExecutionId, VersionType> = HashMap::new();
    let mut submit_of_delay: HashMap<&grpc_client::DelayId, VersionType> = HashMap::new();
    for event in events {
        if let Some(execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
            event:
                Some(execution_event::history_event::Event::JoinSetRequest(JoinSetRequest {
                    join_set_request: Some(join_set_request),
                    ..
                })),
        })) = &event.event
        {
            match join_set_request {
                join_set_request::JoinSetRequest::ChildExecutionRequest(child_req) => {
                    if let Some(child_execution_id) = &child_req.child_execution_id {
                        submit_of_child.insert(child_execution_id, event.version);
                    }
                }
                join_set_request::JoinSetRequest::DelayRequest(delay_req) => {
                    if let Some(delay_id) = &delay_req.delay_id {
                        submit_of_delay.insert(delay_id, event.version);
                    }
                }
            }
        }
    }

    let mut groups: HashMap<VersionType, Vec<VersionType>> = HashMap::new();
    for (join_next_version, response) in compute_join_next_to_response(events, responses) {
        let submit_version = match response.response.as_ref() {
            Some(join_set_response_event::Response::ChildExecutionFinished(
                join_set_response_event::ChildExecutionFinished {
                    child_execution_id: Some(child_execution_id),
                    ..
                },
            )) => submit_of_child.get(child_execution_id).copied(),
            Some(join_set_response_event::Response::DelayFinished(
                join_set_response_event::DelayFinished {
                    delay_id: Some(delay_id),
                    ..
                },
            )) => submit_of_delay.get(delay_id).copied(),
            _ => None,
        };
        if let Some(submit_version) = submit_version {
            let group = vec![submit_version, join_next_version];
            groups.insert(submit_version, group.clone());
            groups.insert(join_next_version, group);
        }
    }
    // Submits still waiting on their join-next self-highlight only.
    for submit_version in submit_of_child
        .values()
        .chain(submit_of_delay.values())
        .copied()
    {
        groups
            .entry(submit_version)
            .or_insert_with(|| vec![submit_version]);
    }
    groups
}

/// Return `None` if there are no events yet associated with the requested execution.
#[allow(clippy::too_many_arguments)]
fn compute_root_trace(
    execution_id: &ExecutionId,
    is_root: bool,
    events_map: &HashMap<ExecutionId, Vec<ExecutionEvent>>,
    responses_map: &HashMap<ExecutionId, HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
    statuses_map: &HashMap<ExecutionId, grpc_client::execution_status::Status>,
    trace_view_state: &UseReducerHandle<TraceViewState>,
    missing_ids: &mut Vec<ExecutionId>,
    expandable_missing_children: &mut HashMap<String, Vec<ExecutionId>>,
    version_to_group: &HashMap<VersionType, Vec<VersionType>>,
) -> Option<TraceDataRoot> {
    let events = match events_map.get(execution_id) {
        Some(events) if !events.is_empty() => events,
        _ => return None,
    };

    // Only the root's direct children link to the detail panel: deeper descendants belong
    // to a child execution whose events are not shown on the right, and their versions
    // would collide with the root's.
    let make_link = |version: VersionType| -> Option<TraceLink> {
        if is_root {
            version_to_group.get(&version).map(|group| TraceLink {
                version,
                group: group.clone(),
            })
        } else {
            None
        }
    };

    let last_event = events.last().expect("not found is sent as an error");
    let is_finished = matches!(last_event.event, Some(execution_event::Event::Finished(_)));
    let responses = responses_map.get(execution_id);
    let mut last_event_at = compute_last_event_at(last_event, is_finished, responses);

    let create_event = events
        .first()
        .expect("not found is sent as an error")
        .event
        .as_ref()
        .expect("`event` is sent by the server");
    let create_event = assert_matches!(
        create_event,
        grpc_client::execution_event::Event::Created(created) => created
    );
    let execution_scheduled_at = DateTime::from(
        create_event
            .scheduled_at
            .expect("`scheduled_at` is sent by the server"),
    );

    let component_type = create_event
        .component_id
        .as_ref()
        .map(|component_id| component_id.component_type());
    let is_stub = component_type == Some(ComponentType::ActivityStub);

    let node_key = execution_id.to_string();
    let child_ids_to_results = compute_child_execution_id_to_child_execution_finished(responses);
    let delay_ids_to_finished = compute_delay_id_to_finished(responses);

    let is_expanded = is_trace_node_expanded(trace_view_state, &node_key, false);

    let children: Vec<TraceData> = events
            .iter()
            .filter_map(|event| {
                let event_created_at = DateTime::from(event.created_at.expect("event.created_at is sent"));
                let event_inner = event.event.as_ref().expect("event is sent by the server");
                match event_inner {
                    // Add HTTP Client traces
                    execution_event::Event::TemporarilyFailed(TemporarilyFailed {
                        http_client_traces,
                        ..
                    })
                    | execution_event::Event::TemporarilyTimedOut(TemporarilyTimedOut{
                        http_client_traces,
                        ..
                    })
                    | execution_event::Event::Finished(Finished {
                        http_client_traces, ..
                    }) => {
                        let children: Vec<_> = http_client_traces
                            .iter()
                            .enumerate()
                            .map(|(idx, trace)| {
                                let name = format!(
                                    "{method} {uri}",
                                    method = trace.method,
                                    uri = trace.uri,
                                );
                                let status = match trace.result {
                                    Some(http_client_trace::Result::Status(status_code)) => BusyIntervalStatus::HttpTraceFinished(status_code),
                                    Some(http_client_trace::Result::Error(_)) => BusyIntervalStatus::HttpTraceError,
                                    None => BusyIntervalStatus::HttpTraceNotResponded,
                                };
                                let node_key = format!("{execution_id}:event:{}:http:{idx}", event.version);
                                TraceData::Child(TraceDataChild {
                                    node_key: node_key.clone(),
                                    is_expanded: is_trace_node_expanded(trace_view_state, &node_key, false),
                                    can_expand: trace.result.is_some(),
                                    name: Html::from(name.clone()),
                                    title: name,
                                    busy: vec![BusyInterval {
                                        started_at: DateTime::from(trace.sent_at.expect("sent_at is sent")),
                                        finished_at: Some(trace.finished_at.map(DateTime::from).unwrap_or(event_created_at)),
                                        title: None,
                                        status,
                                    }],
                                    children: match &trace.result {
                                        Some(http_client_trace::Result::Status(status_code)) => {
                                            let name = format!("Status code: {status_code}");
                                            vec![
                                                TraceData::Child(TraceDataChild {
                                                    node_key: format!("{node_key}:status"),
                                                    is_expanded: false,
                                                    can_expand: false,
                                                    name: Html::from(name.clone()),
                                                    title: name,
                                                    busy: vec![],
                                                    children: vec![],
                                                    load_button: None,
                                                    link: None,
                                                })
                                            ]
                                        },
                                        Some(http_client_trace::Result::Error(error)) => {
                                            let name = format!("Failed: `{error}`");
                                            vec![
                                                TraceData::Child(TraceDataChild {
                                                    node_key: format!("{node_key}:error"),
                                                    is_expanded: false,
                                                    can_expand: false,
                                                    name: Html::from(name.clone()),
                                                    title: name,
                                                    busy: vec![],
                                                    children: vec![],
                                                    load_button: None,
                                                    link: None,
                                                })
                                            ]
                                        },
                                        None => {
                                            vec![]
                                        }
                                    },
                                    load_button: None,
                                    link: None,
                                })
                            })
                            .collect();
                        Some(children)
                    }
                    // Add child executions
                    execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                        event:
                            Some(execution_event::history_event::Event::JoinSetRequest(
                                JoinSetRequest {
                                    join_set_request: Some(join_set_request::JoinSetRequest::ChildExecutionRequest(
                                        join_set_request::ChildExecutionRequest{child_execution_id: Some(child_execution_id), ..})),
                                    ..
                                },
                            )),
                    }) => {
                        let name = if let Some(suffix) =  child_execution_id.id.strip_prefix(&format!("{execution_id}{EXECUTION_ID_INFIX}")) {
                            suffix.to_string()
                        } else {
                            child_execution_id.to_string()
                        };

                        // Based on responses to parent execution.
                        let is_finished = child_ids_to_results.contains_key(child_execution_id);
                        if trace_view_state.hide_finished && is_finished {
                            return None;
                        }

                        if !trace_view_state.deref().execution_ids_to_fetch_state.contains_key(child_execution_id) {
                            missing_ids.push(child_execution_id.clone());
                            expandable_missing_children
                                .entry(node_key.clone())
                                .or_default()
                                .push(child_execution_id.clone());
                            expandable_missing_children
                                .entry(child_execution_id.to_string())
                                .or_default()
                                .push(child_execution_id.clone());
                        }

                        if let Some(mut child_root) = compute_root_trace(
                            child_execution_id,
                            false,
                            events_map,
                            responses_map,
                            statuses_map,
                            trace_view_state,
                            missing_ids,
                            expandable_missing_children,
                            version_to_group,
                        ) {
                            last_event_at = last_event_at.max(child_root.last_event_at);
                            child_root.link = make_link(event.version);
                            Some(vec![TraceData::Root(child_root)])
                        } else {
                            // Child execution has no events loaded yet.
                            let started_at = DateTime::from(event.created_at.expect("event.created_at must be sent"));
                            let (interval_title, status, finished_at) =
                                if let Some((result_detail_value, finished_at)) = child_ids_to_results.get(child_execution_id) {
                                    let status = BusyIntervalStatus::from(result_detail_value);
                                    let duration = (*finished_at - started_at).to_std().expect("started_at must be <= finished_at");
                                    (format!("{status} in {duration:?}"), status, Some(*finished_at))
                                } else {
                                    let status = BusyIntervalStatus::ExecutionUnfinishedWithoutPendingState; // We don't know the pending state yet.
                                    (status.to_string(), status, None)
                                };
                            Some(vec![
                                TraceData::Child(TraceDataChild {
                                    node_key: child_execution_id.to_string(),
                                    is_expanded: false,
                                    can_expand: true,
                                    name: html!{<>
                                        <Link<Route> to={Route::ExecutionTrace { execution_id: child_execution_id.clone() }}>
                                            {name}
                                        </Link<Route>>
                                    </>},
                                    title: child_execution_id.to_string(),
                                    busy: vec![BusyInterval {
                                        started_at,
                                        finished_at,
                                        title: Some(interval_title),
                                        status
                                    }],
                                    children: Vec::new(),
                                    load_button: None,
                                    link: make_link(event.version),
                                })
                            ])
                        }
                    },
                    // Add persistent delays
                    execution_event::Event::HistoryVariant(execution_event::HistoryEvent {
                        event:
                            Some(execution_event::history_event::Event::JoinSetRequest(
                                JoinSetRequest {
                                    join_set_request: Some(join_set_request::JoinSetRequest::DelayRequest(delay_req)),
                                    ..
                                },
                            )),
                    }) => {
                        if !trace_view_state.show_delays {
                            return None;
                        }
                        let delay_id = delay_req.delay_id.as_ref().expect("`delay_id` is sent in `DelayRequest`");
                        let expires_at = DateTime::from(delay_req.expires_at.expect("`expires_at` is sent in `DelayRequest`"));
                        let started_at = event_created_at;

                        // "Show finished" governs finished (OK/cancelled) delays too.
                        let delay_finished = delay_ids_to_finished.get(delay_id);
                        if trace_view_state.hide_finished && delay_finished.is_some() {
                            return None;
                        }

                        let (status, finished_at, interval_title) = match delay_finished {
                            Some((success, finished_at)) => {
                                let status = if *success {
                                    BusyIntervalStatus::DelayOk
                                } else {
                                    BusyIntervalStatus::DelayCancelled
                                };
                                // A delay's recorded finish (its expiry) can predate the
                                // request event's persisted timestamp for zero/near-zero
                                // delays; clamp to keep the interval non-negative.
                                let finished_at = (*finished_at).max(started_at);
                                let duration = (finished_at - started_at).to_std().expect("clamped to be >= started_at");
                                let title = format!("{status} in {duration:?}");
                                (status, Some(finished_at), title)
                            }
                            // Not finished yet: paused, or still counting down to expiry.
                            None if delay_req.paused => (
                                BusyIntervalStatus::DelayPaused,
                                None,
                                BusyIntervalStatus::DelayPaused.to_string(),
                            ),
                            None => (
                                BusyIntervalStatus::DelayInProgress,
                                None,
                                format!("{} until {expires_at}", BusyIntervalStatus::DelayInProgress),
                            ),
                        };

                        // Shorten the delay id like child executions: keep only the trailing
                        // join-set part after the owning execution prefix.
                        let short_name = delay_id
                            .id
                            .rsplit_once(EXECUTION_ID_INFIX)
                            .map(|(_, suffix)| suffix)
                            .unwrap_or(delay_id.id.as_str());
                        let node_key = format!("{execution_id}:delay:{}", delay_id.id);
                        Some(vec![
                            TraceData::Child(TraceDataChild {
                                node_key,
                                is_expanded: false,
                                can_expand: false,
                                name: html!{
                                    <span class="step-delay-name">
                                        <span class="step-type-icon">{Html::from(Icon::Time)}</span>
                                        {short_name}
                                    </span>
                                },
                                title: delay_id.id.clone(),
                                busy: vec![BusyInterval {
                                    started_at,
                                    finished_at,
                                    title: Some(interval_title),
                                    status,
                                }],
                                children: Vec::new(),
                                load_button: None,
                                link: make_link(event.version),
                            })
                        ])
                    },
                    _ => None,
                }
            })
            .flatten()
            .collect();
    let last_event_at = last_event_at; // drop mut

    let mut current_locked_at: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    let mut busy = vec![BusyInterval {
        started_at: execution_scheduled_at,
        finished_at: Some(last_event_at),
        title: None,
        status: BusyIntervalStatus::ExecutionSinceScheduled,
    }];
    for event in events {
        let event_inner = event.event.as_ref().unwrap();
        match event_inner {
            execution_event::Event::Locked(locked) => {
                if let Some((locked_at, lock_expires_at)) = current_locked_at.take() {
                    // if the created_at..expires_at includes the current lock's created_at, we are extending the lock
                    let duration = (lock_expires_at - locked_at)
                        .to_std()
                        .expect("locked_at must be <= expires_at");
                    busy.push(BusyInterval {
                        started_at: locked_at,
                        finished_at: Some(lock_expires_at),
                        title: Some(format!("Locked for {duration:?}")),
                        status: BusyIntervalStatus::ExecutionLocked,
                    });
                }
                let locked_at =
                    DateTime::from(event.created_at.expect("event.created_at is always sent"));
                let expires_at = DateTime::from(
                    locked
                        .lock_expires_at
                        .expect("Locked.lock_expires_at is sent"),
                );
                current_locked_at = Some((locked_at, expires_at));
            }
            execution_event::Event::TemporarilyFailed(..)
            | execution_event::Event::Unlocked(..)
            | execution_event::Event::TemporarilyTimedOut(..)
            | execution_event::Event::Finished(..) => {
                let started_at = current_locked_at
                    .take()
                    .map(|(locked_at, _)| locked_at)
                    .unwrap_or(execution_scheduled_at); // webhooks have no locks
                let finished_at =
                    DateTime::from(event.created_at.expect("event.created_at is always sent"));
                let duration = (finished_at - started_at)
                    .to_std()
                    .expect("started_at must be <= finished_at");
                let status = match event_inner {
                    execution_event::Event::TemporarilyFailed(..) => {
                        BusyIntervalStatus::ExecutionErrorTemporary
                    }
                    execution_event::Event::Unlocked(..) => BusyIntervalStatus::ExecutionLocked,
                    execution_event::Event::TemporarilyTimedOut(..) => {
                        BusyIntervalStatus::ExecutionTimeoutTemporary
                    }
                    execution_event::Event::Finished(Finished {
                        value:
                            Some(SupportedFunctionResult {
                                value: Some(result_detail_value),
                                ..
                            }),
                        ..
                    }) => BusyIntervalStatus::from(result_detail_value),
                    _ => unreachable!("unexpected {event_inner:?}"),
                };
                let title = format!("{status} in {duration:?}");
                busy.push(BusyInterval {
                    started_at,
                    finished_at: Some(finished_at),
                    title: Some(title),
                    status,
                });
            }
            _ => {}
        }
    }
    // If there is locked without unlocked, add the unfinished interval.
    // Ignore the lock_expires_at as it might be in the future or beyond the last seen event.
    if let Some((locked_at, _lock_expires_at)) = current_locked_at {
        let status = BusyIntervalStatus::ExecutionUnfinishedWithoutPendingState;
        busy.push(BusyInterval {
            started_at: locked_at,
            finished_at: None,
            title: Some(status.to_string()),
            status,
        });
    }

    let ffqn = {
        let first = events
            .first()
            .expect("checked that events is not empty")
            .event
            .as_ref()
            .expect("event.event is sent");
        let fn_name = assert_matches!(first,
            execution_event::Event::Created(execution_event::Created{function_name: Some(fn_name), ..}) => fn_name);
        FunctionFqn::from(fn_name.clone())
    };

    let name = html! {
        <>
            <span class="step-execution-id">
                if !is_root {
                    <span class="step-type-icon">{Html::from(Icon::Function)}</span>
                }
                {execution_id.render_execution_parts(true, ExecutionLink::Trace)}
            </span>
            <span class="step-ffqn">
                <FfqnWithLinks ffqn={ffqn.clone()} fully_qualified={true} hide_submit={true} />
                if is_stub {
                    {" "}<span class="stub-indicator">{"(stub)"}</span>
                }
            </span>
        </>
    };
    Some(TraceDataRoot {
        node_key,
        is_expanded,
        can_expand: !children.is_empty(),
        name,
        title: format!("{execution_id} {ffqn}"),
        scheduled_at: execution_scheduled_at,
        last_event_at,
        busy,
        children,
        load_button: None,
        current_status: statuses_map.get(execution_id).cloned(),
        link: None,
    })
}

fn is_trace_node_expanded(
    trace_view_state: &UseReducerHandle<TraceViewState>,
    node_key: &str,
    default: bool,
) -> bool {
    trace_view_state
        .expanded_nodes
        .get(node_key)
        .copied()
        .unwrap_or(default)
}

fn compute_child_execution_id_to_child_execution_finished(
    responses: Option<&HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
) -> HashMap<ExecutionId, (supported_function_result::Value, DateTime<Utc>)> {
    responses
        .into_iter()
        .flat_map(|map| {
            map.values().flatten().filter_map(|resp| {
                if let JoinSetResponseEvent {
                    response:
                        Some(join_set_response_event::Response::ChildExecutionFinished(
                            join_set_response_event::ChildExecutionFinished {
                                child_execution_id: Some(child_execution_id),
                                value:
                                    Some(SupportedFunctionResult {
                                        value: Some(result_detail_value),
                                        ..
                                    }),
                            },
                        )),
                    ..
                } = resp
                {
                    let created_at =
                        DateTime::from(resp.created_at.expect("response.created_at is sent"));
                    Some((
                        child_execution_id.clone(),
                        (result_detail_value.clone(), created_at),
                    ))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn compute_delay_id_to_finished(
    responses: Option<&HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
) -> HashMap<grpc_client::DelayId, (bool, DateTime<Utc>)> {
    responses
        .into_iter()
        .flat_map(|map| {
            map.values().flatten().filter_map(|resp| {
                if let JoinSetResponseEvent {
                    response:
                        Some(join_set_response_event::Response::DelayFinished(
                            join_set_response_event::DelayFinished {
                                delay_id: Some(delay_id),
                                success,
                            },
                        )),
                    ..
                } = resp
                {
                    let created_at =
                        DateTime::from(resp.created_at.expect("response.created_at is sent"));
                    Some((delay_id.clone(), (*success, created_at)))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn compute_last_event_at(
    last_event: &ExecutionEvent,
    is_finished: bool,
    responses: Option<&HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
) -> DateTime<Utc> {
    let candidate = DateTime::from(last_event.created_at.expect("event.created_at is sent"));

    match responses {
        Some(responses) if !is_finished => responses
            .values()
            .filter_map(|vec| vec.last())
            .map(|e| DateTime::from(e.created_at.expect("event.created_at is sent")))
            .chain(std::iter::once(candidate))
            .max()
            .expect("chained with last_event so cannot be empty"),
        _ => candidate,
    }
}
