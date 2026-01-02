use crate::BASE_URL;
use crate::components::execution_detail::utils::{compute_join_next_to_response, event_to_detail};
use crate::components::execution_header::{ExecutionHeader, ExecutionLink};
use crate::components::trace::trace_view::{PAGE, SLEEP_MILLIS};
use crate::grpc::grpc_client::{
    self, ExecutionEvent, ExecutionId, JoinSetId, JoinSetResponseEvent, ResponseWithCursor,
    execution_event,
};
use crate::util::time::{TimeGranularity, human_formatted_timedelta};
use assert_matches::assert_matches;
use chrono::DateTime;
use gloo::timers::future::TimeoutFuture;
use hashbrown::HashMap;
use log::{debug, trace};
use std::rc::Rc;
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
                    // Will be followed by ExecutionFetchState::Requested
                };
                this.execution_ids_to_fetch_state
                    .insert(execution_id, new_fetch_state);
                Rc::from(this)
            }
        }
    }
}

#[function_component(ExecutionLogPage)]
pub fn execution_log_page(ExecutionLogPageProps { execution_id }: &ExecutionLogPageProps) -> Html {
    let log_state = use_reducer_eq(ExecutionLogState::default);

    // Fill the current execution id
    use_effect_with(execution_id.clone(), {
        let log_state = log_state.clone();
        move |execution_id| {
            log_state.dispatch(ExecutionLogAction::AddExecutionId(execution_id.clone()));
        }
    });

    use_effect_with(log_state.clone(), on_state_change);

    let dummy_events = Vec::new();
    let events = log_state.events.get(execution_id).unwrap_or(&dummy_events);
    log::debug!("rendering ExecutionDetailPage {:?}", events.iter().next());

    let dummy_response_map = HashMap::new();
    let responses = log_state
        .responses
        .get(execution_id)
        .unwrap_or(&dummy_response_map);

    let join_next_version_to_response = compute_join_next_to_response(events, responses);

    let details_html =
        render_execution_details(execution_id, events, &join_next_version_to_response);

    html! {
        <>
        <ExecutionHeader execution_id={execution_id.clone()} link={ExecutionLink::Log} />

        if !events.is_empty() {
            {details_html}
        } else {
            <p>{"Loading details..."}</p>
        }
    </>}
}

fn on_state_change(log_state: &UseReducerHandle<ExecutionLogState>) {
    trace!("Triggered use_effects");
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
            };
        });
    }
}

fn render_execution_details(
    execution_id: &ExecutionId,
    events: &[ExecutionEvent],
    join_next_version_to_response: &HashMap<u32, &JoinSetResponseEvent>,
) -> Option<Html> {
    if events.is_empty() {
        return None;
    }
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

    let execution_scheduled_at = {
        DateTime::from(
            create_event
                .scheduled_at
                .expect("`scheduled_at` is sent by the server"),
        )
    };

    let rows: Vec<_> = events
        .iter()
        .map(|event| {
            let detail = event_to_detail(
                execution_id,
                event,
                join_next_version_to_response,
                ExecutionLink::Log,
                false,
            );
            let created_at =
                DateTime::from(event.created_at.expect("`created_at` sent by the server"));
            let since_scheduled = human_formatted_timedelta(
                created_at - execution_scheduled_at,
                TimeGranularity::Fine,
            );

            html! { <tr>
                <td>{created_at.to_string()}</td>
                <td>
                    <label title={execution_scheduled_at.to_string()}>
                        { since_scheduled }
                    </label>
                </td>
                <td>{detail}</td>
            </tr>}
        })
        .collect();
    Some(html! {
        <div class="table-wrapper">
        <table>
        <thead>
        <tr>
            <th>{"Timestamp"}</th>
            <th>{"Since scheduled"}</th>
            <th>{"Detail"}</th>
        </tr>
        </thead>
        <tbody>
        {rows}
        </tbody>
        </table>
        </div>
    })
}
