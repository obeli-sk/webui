use crate::components::execution_detail::utils::{compute_join_next_to_response, event_to_detail};
use crate::components::execution_header::{ExecutionHeader, ExecutionLink};
use crate::components::notification::{Notification, NotificationContext};
use crate::components::time_display::{TimeDisplayControl, Timestamp};
use crate::components::trace::highlight::{BacktraceJump, TraceHighlightJump};
use crate::components::trace::trace_view::{PAGE, compute_submit_await_version_groups};
use crate::model::{ColorExt as _, ExecutionIdExt as _};
use crate::status_hub::StatusHub;
use crate::util::time::{
    TimeGranularity, format_date, human_formatted_timedelta, relative_time_if_significant,
};
use assert_matches::assert_matches;
use concepts::{
    ExecutionId, JoinSetId,
    storage::{
        Created, ExecutionEvent, ExecutionRequest, HistoryEvent, JoinSetRequest,
        JoinSetResponseEventOuter, ResponseWithCursor,
    },
};
use hashbrown::HashMap;
use log::{error, trace};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ExecutionLogPageProps {
    pub execution_id: ExecutionId,
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

enum ExecutionLogAction {
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
    /// Mark a direct child execution's `Created` event as being fetched.
    RequestChildCreated(ExecutionId),
    /// Store a fetched child execution's `Created` event.
    SaveChildCreated {
        execution_id: ExecutionId,
        created: Box<Created>,
    },
}

#[derive(Default, Clone, PartialEq)]
struct ExecutionLogState {
    execution_ids_to_fetch_state: HashMap<ExecutionId, ExecutionFetchState>,
    events: HashMap<ExecutionId, Vec<ExecutionEvent>>,
    responses: HashMap<ExecutionId, HashMap<JoinSetId, Vec<JoinSetResponseEventOuter>>>,
    /// Cache of direct child executions' `Created` (version 0) events, used to show the
    /// function name and parameters on the "Submit child" rows. Kept separate from `events`
    /// so that navigating into a child still performs a full fetch. `None` = fetch in-flight.
    child_created: HashMap<ExecutionId, Option<Created>>,
}

impl Reducible for ExecutionLogState {
    type Action = ExecutionLogAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            ExecutionLogAction::AddExecutionId(execution_id) => {
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
            ExecutionLogAction::SetPending(execution_id) => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Pending);
                Rc::from(this)
            }
            ExecutionLogAction::RequestNextPage {
                execution_id,
                cursors,
            } => {
                let mut this = self.as_ref().clone();
                this.execution_ids_to_fetch_state
                    .insert(execution_id, ExecutionFetchState::Requested(cursors));
                Rc::from(this)
            }
            ExecutionLogAction::SavePage {
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
                    let response = response.event;
                    let join_set_id = response.event.join_set_id.clone();
                    let execution_responses = join_set_to_resps.entry(join_set_id).or_default();
                    execution_responses.push(response);
                }
                let new_fetch_state = if is_finished {
                    ExecutionFetchState::Finished
                } else {
                    ExecutionFetchState::Pending
                };
                this.execution_ids_to_fetch_state
                    .insert(execution_id, new_fetch_state);
                Rc::from(this)
            }
            ExecutionLogAction::RequestChildCreated(execution_id) => {
                if self.child_created.contains_key(&execution_id) {
                    self
                } else {
                    let mut this = self.as_ref().clone();
                    this.child_created.insert(execution_id, None);
                    Rc::from(this)
                }
            }
            ExecutionLogAction::SaveChildCreated {
                execution_id,
                created,
            } => {
                let mut this = self.as_ref().clone();
                this.child_created.insert(execution_id, Some(*created));
                Rc::from(this)
            }
        }
    }
}

#[component(ExecutionLogPage)]
pub fn execution_log_page(ExecutionLogPageProps { execution_id }: &ExecutionLogPageProps) -> Html {
    let log_state = use_reducer_eq(ExecutionLogState::default);
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    use_effect_with(execution_id.clone(), {
        let log_state = log_state.clone();
        move |execution_id| {
            log_state.dispatch(ExecutionLogAction::AddExecutionId(execution_id.clone()));
        }
    });

    // Fetch each direct child execution's Created (version 0) event so the "Submit child"
    // rows can show the child's function name and parameters, matching the trace view. This
    // is cached separately from `events` so navigating into a child still fetches its full log.
    use_effect_with(
        (
            log_state.clone(),
            execution_id.clone(),
            notifications.clone(),
        ),
        |(log_state, execution_id, notifications): &(
            UseReducerHandle<ExecutionLogState>,
            ExecutionId,
            NotificationContext,
        )| {
            let dummy = Vec::new();
            let events = log_state.events.get(execution_id).unwrap_or(&dummy);
            for event in events {
                if let ExecutionRequest::HistoryEvent {
                    event:
                        HistoryEvent::JoinSetRequest {
                            request:
                                JoinSetRequest::ChildExecutionRequest {
                                    child_execution_id, ..
                                },
                            ..
                        },
                } = &event.event
                    && let child_id = &ExecutionId::Derived(child_execution_id.clone())
                    && !log_state.child_created.contains_key(child_id)
                {
                    log_state.dispatch(ExecutionLogAction::RequestChildCreated(child_id.clone()));
                    fetch_child_created(log_state.clone(), child_id.clone(), notifications.clone());
                }
            }
        },
    );

    let status_hub = use_context::<StatusHub>().expect("StatusHub should be provided by App");
    use_effect_with(
        (log_state.clone(), notifications.clone(), status_hub),
        on_state_change,
    );

    let dummy_events = Vec::new();
    let events = log_state.events.get(execution_id).unwrap_or(&dummy_events);

    let dummy_response_map = HashMap::new();
    let responses = log_state
        .responses
        .get(execution_id)
        .unwrap_or(&dummy_response_map);

    let join_next_version_to_response = compute_join_next_to_response(events, responses);
    let submit_await_version_groups = compute_submit_await_version_groups(events, responses);

    // Build map of child execution ID -> Created event from the child-created cache.
    let child_created_events: HashMap<ExecutionId, Created> = log_state
        .child_created
        .iter()
        .filter_map(|(id, created)| created.clone().map(|created| (id.clone(), created)))
        .collect();

    let details_html = if !events.is_empty() {
        render_execution_details(
            execution_id,
            events,
            &join_next_version_to_response,
            &submit_await_version_groups,
            &child_created_events,
        )
    } else {
        html! { <div class="loading-details">{"Loading execution details..."}</div> }
    };

    html! {
        <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::ExecutionLog} />
            <div class="timeline-toolbar"><TimeDisplayControl /></div>
            <div class="timeline-container">
                {details_html}
            </div>
        </>
    }
}

/// Fetch a single direct child execution's `Created` (version 0) event and store it in the
/// child-created cache. Unlike the main fetch loop this grabs one event and never polls.
fn fetch_child_created(
    log_state: UseReducerHandle<ExecutionLogState>,
    child_execution_id: ExecutionId,
    notifications: NotificationContext,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let response = crate::rest::events::child_created(&child_execution_id).await;
        match response {
            Ok(created) => {
                if let Some(created) = created {
                    log_state.dispatch(ExecutionLogAction::SaveChildCreated {
                        execution_id: child_execution_id,
                        created: Box::new(created),
                    });
                }
            }
            Err(e) => {
                error!("Failed to list child execution created event: {:?}", e);
                notifications.push(Notification::error(format!(
                    "Failed to load child execution: {}",
                    e
                )));
            }
        }
    });
}

fn on_state_change(
    (log_state, notifications, hub): &(
        UseReducerHandle<ExecutionLogState>,
        NotificationContext,
        StatusHub,
    ),
) {
    trace!("Triggered on_state_change");
    for (execution_id, cursors) in
        log_state
            .execution_ids_to_fetch_state
            .iter()
            .filter_map(|(id, state)| match state {
                ExecutionFetchState::Requested(cursors) => Some((id, *cursors)),
                ExecutionFetchState::Pending | ExecutionFetchState::Finished => None,
            })
    {
        log_state.dispatch(ExecutionLogAction::SetPending(execution_id.clone()));
        let execution_id = execution_id.clone();
        let log_state = log_state.clone();
        let notifications = notifications.clone();
        let hub = hub.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _watch = hub.watch(&execution_id);
            let seq = hub.seq(&execution_id);
            let response = crate::rest::events::history_page(
                &execution_id,
                cursors.version_from,
                cursors.responses_cursor_from,
                PAGE,
            )
            .await;

            match response {
                Ok(resp) => {
                    let server_resp = resp;
                    let last_event = server_resp.events.last();
                    let is_finished = matches!(
                        last_event.map(|e| &e.event),
                        Some(ExecutionRequest::Finished { .. })
                    );
                    let full = server_resp.events.len() >= PAGE as usize
                        || server_resp.responses.len() >= PAGE as usize;
                    let cursors = Cursors {
                        version_from: last_event
                            .map(|e| e.version.0 + 1)
                            .unwrap_or(cursors.version_from),
                        responses_cursor_from: server_resp
                            .responses
                            .last()
                            .map(|resp| resp.cursor.0)
                            .unwrap_or(cursors.responses_cursor_from),
                    };
                    log_state.dispatch(ExecutionLogAction::SavePage {
                        execution_id: execution_id.clone(),
                        new_events: server_resp.events,
                        new_responses: server_resp.responses,
                        is_finished,
                    });
                    if !is_finished {
                        // A full page is followed at once, otherwise wait until the execution changes.
                        if !full {
                            hub.changed(&execution_id, seq).await;
                        }
                        log_state.dispatch(ExecutionLogAction::RequestNextPage {
                            execution_id,
                            cursors,
                        });
                    }
                }
                Err(e) => {
                    error!("Failed to list execution events: {:?}", e);
                    notifications.push(Notification::error(format!(
                        "Failed to load execution events: {}",
                        e
                    )));
                }
            }
        });
    }
}

fn render_execution_details(
    current_execution_id: &ExecutionId,
    events: &[ExecutionEvent],
    join_next_version_to_response: &HashMap<u32, &JoinSetResponseEventOuter>,
    submit_await_version_groups: &HashMap<u32, Vec<u32>>,
    child_created_events: &HashMap<ExecutionId, Created>,
) -> Html {
    let create_event = events.first().expect("not found");
    let execution_created_at = create_event.created_at;
    let create_event = assert_matches!(
        &create_event.event,
        ExecutionRequest::Created(created) => created
    );
    let initially_scheduled_at = create_event.scheduled_at;

    let initial_scheduling_duration =
        relative_time_if_significant(execution_created_at, initially_scheduled_at);

    let mut submit_version_to_color = HashMap::new();
    for event in events {
        if let ExecutionRequest::HistoryEvent {
            event: HistoryEvent::JoinSetRequest { request, .. },
        } = &event.event
        {
            let color = match request {
                JoinSetRequest::ChildExecutionRequest {
                    child_execution_id, ..
                } => ExecutionId::Derived(child_execution_id.clone()).color(),
                JoinSetRequest::DelayRequest { delay_id, .. } => delay_id.color(),
            };
            submit_version_to_color.insert(event.version.0, color);
        }
    }

    let mut event_links: HashMap<
        u32, /* start or end version */
        (u32 /* oposite version */, String /* color */),
    > = HashMap::new();
    for (submit_version, group) in submit_await_version_groups {
        let [group_submit_version, await_version] = group.as_slice() else {
            continue;
        };
        if submit_version != group_submit_version {
            continue;
        }
        let Some(color) = submit_version_to_color.get(submit_version) else {
            continue;
        };
        event_links.insert(*submit_version, (*await_version, color.clone()));
        event_links.insert(*await_version, (*submit_version, color.clone()));
    }

    let rows: Vec<_> = events
        .iter()
        .map(|event| {
            let detail = event_to_detail(
                event,
                join_next_version_to_response,
                child_created_events,
                ExecutionLink::ExecutionLog,
                false,
            );
            let event_created_at = event.created_at;
            let version = event.version.0;
            let since_created = human_formatted_timedelta(
                event_created_at - execution_created_at,
                TimeGranularity::Fine,
            );

            let event_inner = &event.event;
            let mut circle_class = "version-circle";
            let mut circle_color_style = "".to_string();
            let mut join_set_id = None;

            match event_inner {
                ExecutionRequest::Created(_) => circle_class = "version-circle is-created",
                ExecutionRequest::Finished { .. } => circle_class = "version-circle is-finished",
                ExecutionRequest::TemporarilyFailed { .. } | ExecutionRequest::TemporarilyTimedOut { .. } => {
                    circle_class = "version-circle is-error"
                },
                ExecutionRequest::HistoryEvent { event: history_event } => {
                    join_set_id = match history_event {
                        HistoryEvent::JoinSetRequest { join_set_id, .. }
                        | HistoryEvent::JoinNext { join_set_id, .. }
                        | HistoryEvent::JoinSetCreate { join_set_id }
                        | HistoryEvent::JoinNextTry { join_set_id, .. }
                        | HistoryEvent::JoinNextTooMany { join_set_id, .. } => Some(join_set_id),
                        _ => None,
                    };

                    if let Some(join_set_id) = join_set_id {
                        circle_class = "version-circle is-join";
                        let color = join_set_id.color();
                        circle_color_style = format!("border-color: {0}; color: {0};", color);
                    }
                },
                _ => {}
            }

            // Scroll Button
            let scroll_button = if let Some((target_version, color)) = event_links.get(&version) {
                let diff = (*target_version).abs_diff(version);

                if diff > 1 {
                    let label = if *target_version > version {
                        "Go to Await ↓"
                    } else {
                        "Go to Submit ↑"
                    };

                    let target_element_id = format!("event-content-{}", target_version);
                    let color_clone = color.clone();

                    let onclick = Callback::from(move |_| {
                        if let Some(window) = web_sys::window()
                             && let Some(document) = window.document()
                                 && let Some(element) = document.get_element_by_id(&target_element_id) {
                                     element.scroll_into_view();
                                     if let Some(html_el) = element.dyn_ref::<web_sys::HtmlElement>() {
                                         let style = html_el.style();
                                         let _ = style.set_property("transition", "none");
                                         let _ = style.set_property("border-color", &color_clone);
                                         let _ = style.set_property("box-shadow", &format!("0 0 8px {}", color_clone));
                                         let _ = html_el.offset_height();
                                         let _ = style.set_property("transition", "border-color 1.5s ease-out, box-shadow 1.5s ease-out");
                                         let _ = style.set_property("border-color", "var(--border)"); // Return to original .timeline-content
                                         let _ = style.set_property("box-shadow", "none");
                                     }
                                 }
                    });

                    let button_style = format!("color: {0}; border-color: {0};", color);
                    Some(html! {
                        <button class="scroll-link" style={button_style} {onclick}>{label}</button>
                    })
                } else {
                    None
                }
            } else {
                None
            };


            // Jump to this event highlighted in the trace view (correlated events only).
            let trace_jump_button = submit_await_version_groups
                .contains_key(&version)
                .then(|| html! {
                    <TraceHighlightJump
                        execution_id={current_execution_id.clone()}
                        version={version}
                    />
                });

            let backtrace_link = event.backtrace_id.as_ref().map(|backtrace_id| html! {
                <BacktraceJump execution_id={current_execution_id.clone()} version={backtrace_id.0} />
            });

            let class = format!("{} {}", circle_class, if circle_color_style.is_empty() { ""} else { "is-join" });
            let content_id = format!("event-content-{version}");

            // Timeline Line Logic
            // Do not show line if this is the last event OR if the event is Finished
            let is_finished = matches!(event_inner, ExecutionRequest::Finished { .. });

            let timeline_line = if !is_finished {
                 html! { <div class="timeline-line"></div> }
            } else {
                 html! {}
            };

            let event_duration = if let Some(next_event) = events.get(usize::try_from(version + 1).unwrap()) {
                relative_time_if_significant(event_created_at, next_event.created_at)
            } else {
                // Don't display event duration
                None
            };
            let title = join_set_id.map(|id| id.to_string());
            html! {
                <div class="timeline-row">
                    <div class="timeline-left">
                        <div class={class} style={circle_color_style} title={title}>
                            {version}
                        </div>
                        {timeline_line}
                    </div>
                    <div class="timeline-content" id={content_id}>
                        <div class="timeline-meta">
                            <div>
                                if version == 0 {
                                    <span>
                                        {"+0s"}
                                        if let Some(initial_scheduling_duration) = &initial_scheduling_duration {
                                            {", scheduled +"}
                                            {initial_scheduling_duration}
                                        }
                                    </span>
                                } else {
                                    <span title={format!("Created at: {}", format_date(event_created_at))}>
                                        {" +"}{since_created}
                                    </span>
                                    if let Some(event_duration) = event_duration {
                                        <span class="event-duration">
                                            {", took "}{event_duration}
                                        </span>
                                    }
                                }
                            </div>
                            <div class="timeline-meta-actions">
                                <Timestamp target={event_created_at} />
                                {scroll_button}
                                {trace_jump_button}
                                {backtrace_link}
                            </div>
                        </div>
                        <div class="detail-body">
                            {detail}
                        </div>
                    </div>
                </div>
            }
        })
        .collect();

    html! { <>{rows}</> }
}
