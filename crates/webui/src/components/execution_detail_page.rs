use crate::BASE_URL;
use crate::components::execution_detail::utils::{compute_join_next_to_response, event_to_detail};
use crate::components::execution_header::{ExecutionHeader, ExecutionLink};
use crate::components::notification::{Notification, NotificationContext};
use crate::components::trace::trace_view::{PAGE, SLEEP_MILLIS};
use crate::grpc::grpc_client::{
    self, ExecutionEvent, ExecutionId, JoinSetId, JoinSetResponseEvent, ResponseWithCursor,
    execution_event,
    execution_event::history_event::{
        Event as HistoryEventEnum, JoinNext, JoinNextTooMany, JoinSetCreated, JoinSetRequest,
        join_set_request,
    },
    join_set_response_event,
};
use crate::util::time::{
    TimeGranularity, format_date, human_formatted_timedelta, relative_time_if_significant,
};
use assert_matches::assert_matches;
use chrono::DateTime;
use gloo::timers::future::TimeoutFuture;
use hashbrown::HashMap;
use log::{error, trace};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ExecutionLogPageProps {
    pub execution_id: grpc_client::ExecutionId,
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
}

#[derive(Default, Clone, PartialEq)]
struct ExecutionLogState {
    execution_ids_to_fetch_state: HashMap<ExecutionId, ExecutionFetchState>,
    events: HashMap<ExecutionId, Vec<ExecutionEvent>>,
    responses: HashMap<ExecutionId, HashMap<JoinSetId, Vec<JoinSetResponseEvent>>>,
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
                };
                this.execution_ids_to_fetch_state
                    .insert(execution_id, new_fetch_state);
                Rc::from(this)
            }
        }
    }
}

// Execution ID or Delay ID metadata
#[derive(Debug)]
struct IdMetadata {
    start_version: u32,
    end_version: u32,
    color: String,
    is_completed: bool,
}

#[function_component(ExecutionLogPage)]
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

    use_effect_with((log_state.clone(), notifications.clone()), on_state_change);

    let dummy_events = Vec::new();
    let events = log_state.events.get(execution_id).unwrap_or(&dummy_events);

    let dummy_response_map = HashMap::new();
    let responses = log_state
        .responses
        .get(execution_id)
        .unwrap_or(&dummy_response_map);

    let join_next_version_to_response = compute_join_next_to_response(events, responses);

    let details_html = if !events.is_empty() {
        render_execution_details(execution_id, events, &join_next_version_to_response)
    } else {
        html! { <div class="loading-details">{"Loading execution details..."}</div> }
    };

    html! {
        <>
            <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::ExecutionLog} />
            <div class="timeline-container">
                {details_html}
            </div>
        </>
    }
}

fn on_state_change(
    (log_state, notifications): &(UseReducerHandle<ExecutionLogState>, NotificationContext),
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
        wasm_bindgen_futures::spawn_local(async move {
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
                    log_state.dispatch(ExecutionLogAction::SavePage {
                        execution_id: execution_id.clone(),
                        new_events: server_resp.events,
                        new_responses: server_resp.responses,
                        is_finished,
                    });
                    if !is_finished {
                        TimeoutFuture::new(SLEEP_MILLIS).await;
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
                        e.message()
                    )));
                }
            }
        });
    }
}

fn render_execution_details(
    current_execution_id: &ExecutionId,
    events: &[ExecutionEvent],
    join_next_version_to_response: &HashMap<u32, &JoinSetResponseEvent>,
) -> Html {
    let create_event = events.first().expect("not found");
    let execution_created_at = DateTime::from(create_event.created_at.expect("crated_at is sent"));
    let create_event = create_event.event.as_ref().expect("event sent");
    let create_event = assert_matches!(
        create_event,
        grpc_client::execution_event::Event::Created(created) => created
    );
    let initially_scheduled_at =
        DateTime::from(create_event.scheduled_at.expect("scheduled_at sent"));

    let initial_scheduling_duration =
        relative_time_if_significant(execution_created_at, initially_scheduled_at);

    let last_known_version = events.last().map(|e| e.version).unwrap_or(0);

    // Support for "Go to" buttons
    let mut ids: HashMap<String /* execution or delay id */, IdMetadata> = HashMap::new();
    for event in events {
        let event_inner = event.event.as_ref().unwrap();

        if let execution_event::Event::HistoryVariant(h) = event_inner {
            match &h.event {
                Some(HistoryEventEnum::JoinSetRequest(JoinSetRequest {
                    join_set_request: Some(inner_req),
                    ..
                })) => {
                    let (task_id, color) = match inner_req {
                        join_set_request::JoinSetRequest::ChildExecutionRequest(req) => {
                            let exe_id =
                                req.child_execution_id.as_ref().expect("id is always sent");
                            (exe_id.to_string(), exe_id.color())
                        }
                        join_set_request::JoinSetRequest::DelayRequest(req) => {
                            let delay_id = req.delay_id.as_ref().expect("id is always sent");
                            (delay_id.to_string(), delay_id.color())
                        }
                    };
                    ids.insert(
                        task_id,
                        IdMetadata {
                            start_version: event.version,
                            end_version: last_known_version,
                            color,
                            is_completed: false,
                        },
                    );
                }
                Some(HistoryEventEnum::JoinNext(_)) => {
                    if let Some(resp) = join_next_version_to_response.get(&event.version)
                        && let Some(response_enum) = &resp.response
                    {
                        let completed_task_id = match response_enum {
                            join_set_response_event::Response::ChildExecutionFinished(c) => c
                                .child_execution_id
                                .as_ref()
                                .expect("id is always sent")
                                .to_string(),
                            join_set_response_event::Response::DelayFinished(d) => {
                                d.delay_id.as_ref().expect("id is always sent").to_string()
                            }
                        };
                        if let Some(meta) = ids.get_mut(&completed_task_id) {
                            meta.end_version = event.version;
                            meta.is_completed = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Build Links Map for Navigation ---
    let mut event_links: HashMap<
        u32, /* start or end version */
        (u32 /* oposite version */, String /* color */),
    > = HashMap::new();
    for meta in ids.values() {
        if meta.is_completed {
            event_links.insert(meta.start_version, (meta.end_version, meta.color.clone()));
            event_links.insert(meta.end_version, (meta.start_version, meta.color.clone()));
        }
    }

    let rows: Vec<_> = events
        .iter()
        .map(|event| {
            let detail = event_to_detail(
                current_execution_id,
                event,
                join_next_version_to_response,
                ExecutionLink::ExecutionLog,
                false,
            );
            let event_created_at = DateTime::from(event.created_at.expect("created_at sent"));
            let since_initially_scheduled = human_formatted_timedelta(
                event_created_at - initially_scheduled_at,
                TimeGranularity::Fine,
            );

            let event_inner = event.event.as_ref().unwrap();
            let mut circle_class = "version-circle";
            let mut circle_color_style = "".to_string();
            let mut join_set_id = None;

            match event_inner {
                execution_event::Event::Created(_) => circle_class = "version-circle is-created",
                execution_event::Event::Finished(_) => circle_class = "version-circle is-finished",
                execution_event::Event::TemporarilyFailed(_) | execution_event::Event::TemporarilyTimedOut(_) => {
                    circle_class = "version-circle is-error"
                },
                execution_event::Event::HistoryVariant(h) => {
                    if let Some(history_event) = &h.event {
                        join_set_id = match history_event {
                            HistoryEventEnum::JoinSetRequest(JoinSetRequest { join_set_id: Some(jid), .. }) => Some(jid),
                            HistoryEventEnum::JoinNext(JoinNext { join_set_id: Some(jid), .. }) => Some(jid),
                            HistoryEventEnum::JoinSetCreated(JoinSetCreated { join_set_id: Some(jid), .. }) => Some(jid),
                            HistoryEventEnum::JoinNextTooMany(JoinNextTooMany { join_set_id: Some(jid), .. }) => Some(jid),
                            _ => None
                        };

                        if let Some(join_set_id) = join_set_id {
                            circle_class = "version-circle is-join";
                            let color = join_set_id.color();
                            circle_color_style = format!("border-color: {0}; color: {0};", color);
                        }
                    }
                },
                _ => {}
            }

            // Scroll Button
            let scroll_button = if let Some((target_version, color)) = event_links.get(&event.version) {
                let diff = (*target_version).abs_diff(event.version);

                if diff > 1 {
                    let label = if *target_version > event.version {
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
                                         let _ = style.set_property("border-color", "#44475a"); // Return to original .timeline-content
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


            let class = format!("{} {}", circle_class, if circle_color_style.is_empty() { ""} else { "is-join" });
            let content_id = format!("event-content-{}", event.version);

            // Timeline Line Logic
            // Do not show line if this is the last event OR if the event is Finished
            let is_finished = matches!(event_inner, execution_event::Event::Finished(_));

            let timeline_line = if !is_finished {
                 html! { <div class="timeline-line"></div> }
            } else {
                 html! {}
            };

            let event_duration = if let Some(next_event) = events.get(usize::try_from(event.version + 1).unwrap()) {
                relative_time_if_significant(event_created_at, DateTime::from(next_event.created_at.expect("created_at sent")))
            } else {
                // Don't display event duration
                None
            };
            let title = join_set_id.map(|id| id.to_string());
            html! {
                <div class="timeline-row">
                    <div class="timeline-left">
                        <div class={class} style={circle_color_style} title={title}>
                            {event.version}
                        </div>
                        {timeline_line}
                    </div>
                    <div class="timeline-content" id={content_id}>
                        <div class="timeline-meta">
                            <div>
                                if event.version == 0 {
                                    <span>
                                        {format_date(event_created_at)}
                                        if let Some(initial_scheduling_duration) = &initial_scheduling_duration {
                                            {", scheduled +"}
                                            {initial_scheduling_duration}
                                        }
                                    </span>
                                } else {
                                    <span title={format!("Created at: {}", format_date(event_created_at))}>
                                        {" +"}{since_initially_scheduled}
                                        if event.version == 1 {
                                            {" after initial scheduling"}
                                        }
                                    </span>
                                    if let Some(event_duration) = event_duration {
                                        <span class="event-duration">
                                            {", took "}{event_duration}
                                        </span>
                                    }
                                }
                            </div>
                            {scroll_button}
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
