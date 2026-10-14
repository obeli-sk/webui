use super::history::stub::HistoryStubEvent;
use crate::components::execution_detail::cancellation_requested::CancellationRequestedEvent;
use crate::components::execution_detail::component_upgrade_finished::ComponentUpgradeFinishedEvent;
use crate::components::execution_detail::created::CreatedEvent;
use crate::components::execution_detail::finished::FinishedEvent;
use crate::components::execution_detail::history::join_next::HistoryJoinNextEvent;
use crate::components::execution_detail::history::join_next_too_many::HistoryJoinNextTooManyEvent;
use crate::components::execution_detail::history::join_next_try::HistoryJoinNextTryEvent;
use crate::components::execution_detail::history::join_set_created::HistoryJoinSetCreatedEvent;
use crate::components::execution_detail::history::join_set_request::HistoryJoinSetRequestEvent;
use crate::components::execution_detail::history::persist::HistoryPersistEvent;
use crate::components::execution_detail::history::schedule::HistoryScheduleEvent;
use crate::components::execution_detail::locked::LockedEvent;
use crate::components::execution_detail::paused::PausedEvent;
use crate::components::execution_detail::temporarily_failed::TemporarilyFailedEvent;
use crate::components::execution_detail::timed_out::TemporarilyTimedOutEvent;
use crate::components::execution_detail::unlocked::UnlockedEvent;
use crate::components::execution_detail::unpaused::UnpausedEvent;
use crate::components::execution_header::ExecutionLink;
use concepts::{
    ExecutionId, JoinSetId,
    storage::{
        Created, ExecutionEvent, ExecutionRequest, HistoryEvent, JoinSetRequest,
        JoinSetResponseEventOuter,
    },
};
use hashbrown::HashMap;
use rest_types::executions::RetVal;
use yew::prelude::*;

/// Identifying suffix of a direct child's derived id relative to its parent execution:
/// the trailing `{join_set_id}_{idx}` segment.
/// E.g. `E_01ABC.g:1_1.o:1-current-deployment-id_1` -> `o:1-current-deployment-id_1`.
pub fn id_suffix(id: &impl std::fmt::Display) -> String {
    let id = id.to_string();
    id.rsplit_once('.')
        .map_or(id.as_str(), |(_, rest)| rest)
        .to_string()
}

pub fn compute_join_next_to_response<'a>(
    events: impl IntoIterator<Item = &'a ExecutionEvent>,
    responses: &'a HashMap<JoinSetId, Vec<JoinSetResponseEventOuter>>,
) -> HashMap<u32 /* version of JoinNext */, &'a JoinSetResponseEventOuter> {
    let mut map = HashMap::new();
    let mut seen_join_nexts: HashMap<&JoinSetId, usize> = HashMap::new();
    for event in events {
        if let ExecutionRequest::HistoryEvent {
            event: HistoryEvent::JoinNext { join_set_id, .. },
        } = &event.event
        {
            let counter_val = seen_join_nexts.entry(join_set_id).or_default();
            if let Some(response) = responses
                .get(join_set_id)
                .and_then(|responses| responses.get(*counter_val))
            {
                map.insert(event.version.0, response);
            }
            *counter_val += 1;
        }
    }
    map
}

pub fn event_to_detail(
    event: &ExecutionEvent,
    join_next_version_to_response: &HashMap<u32, &JoinSetResponseEventOuter>,
    child_created_events: &HashMap<ExecutionId, Created>,
    link: ExecutionLink,
    is_selected: bool,
) -> Html {
    let version = event.version.0;
    match &event.event {
        ExecutionRequest::Created(created) => html! {
            <CreatedEvent created={created.clone()} {version} {link} {is_selected} />
        },
        ExecutionRequest::Locked(locked) => html! {
            <LockedEvent locked={locked.clone()} {version} {is_selected} />
        },
        ExecutionRequest::Unlocked(inner_event) => html! {
            <UnlockedEvent event={inner_event.clone()} {version} {is_selected} />
        },
        ExecutionRequest::TemporarilyFailed {
            backoff_expires_at,
            reason,
            detail,
            http_client_traces,
        } => html! {
            <TemporarilyFailedEvent
                backoff_expires_at={*backoff_expires_at}
                reason={reason.to_string()}
                detail={detail.clone()}
                http_client_traces={http_client_traces.clone().unwrap_or_default()}
                {version}
                {is_selected} />
        },
        ExecutionRequest::TemporarilyTimedOut {
            backoff_expires_at,
            http_client_traces,
        } => html! {
            <TemporarilyTimedOutEvent
                backoff_expires_at={*backoff_expires_at}
                http_client_traces={http_client_traces.clone().unwrap_or_default()}
                {version}
                {is_selected} />
        },
        ExecutionRequest::Finished {
            retval,
            http_client_traces,
        } => html! {
            <FinishedEvent
                result_detail={RetVal::from(retval.clone())}
                http_client_traces={http_client_traces.clone().unwrap_or_default()}
                version={Some(version)}
                {is_selected} />
        },
        ExecutionRequest::HistoryEvent { event: history } => match history {
            HistoryEvent::Schedule {
                execution_id,
                schedule_at,
                params_hash,
                result,
            } => html! {
                <HistoryScheduleEvent
                    execution_id={execution_id.clone()}
                    schedule_at={*schedule_at}
                    params_hash={params_hash.clone()}
                    result={result.clone()}
                    {version}
                    {link}
                    {is_selected} />
            },
            HistoryEvent::JoinSetCreate { join_set_id } => html! {
                <HistoryJoinSetCreatedEvent join_set_id={join_set_id.clone()} {version} {is_selected} />
            },
            HistoryEvent::JoinSetRequest {
                join_set_id: _,
                request,
            } => {
                // Look up child's Created event if this is a ChildExecutionRequest
                let child_created = match request {
                    JoinSetRequest::ChildExecutionRequest {
                        child_execution_id, ..
                    } => child_created_events
                        .get(&ExecutionId::Derived(child_execution_id.clone()))
                        .cloned(),
                    JoinSetRequest::DelayRequest { .. } => None,
                };
                html! {
                    <HistoryJoinSetRequestEvent
                        request={request.clone()}
                        {version}
                        {link}
                        {is_selected}
                        {child_created} />
                }
            }
            HistoryEvent::JoinNext {
                join_set_id,
                run_expires_at: _,
                requested_ffqn,
                closing,
            } => {
                let response = join_next_version_to_response
                    .get(&version)
                    .copied()
                    .cloned();
                html! {
                    <HistoryJoinNextEvent
                        join_set_id={join_set_id.clone()}
                        requested_ffqn={requested_ffqn.clone()}
                        closing={*closing}
                        {response}
                        {version}
                        {link}
                        {is_selected} />
                }
            }
            HistoryEvent::JoinNextTry {
                join_set_id,
                outcome,
            } => html! {
                <HistoryJoinNextTryEvent
                    join_set_id={join_set_id.clone()}
                    outcome={*outcome}
                    {version}
                    {is_selected} />
            },
            HistoryEvent::JoinNextTooMany { join_set_id, .. } => html! {
                <HistoryJoinNextTooManyEvent join_set_id={join_set_id.clone()} {version} {is_selected} />
            },
            HistoryEvent::Persist { kind, .. } => html! {
                <HistoryPersistEvent kind={*kind} {version} {is_selected} />
            },
            HistoryEvent::Stub {
                target_execution_id,
                retval_hash,
                result,
            } => html! {
                <HistoryStubEvent
                    target_execution_id={target_execution_id.clone()}
                    retval_hash={retval_hash.clone()}
                    result={result.clone()}
                    {version}
                    {link}
                    {is_selected} />
            },
        },
        ExecutionRequest::Paused => html! {
            <PausedEvent {version} {is_selected} />
        },
        ExecutionRequest::Unpaused => html! {
            <UnpausedEvent {version} {is_selected} />
        },
        ExecutionRequest::CancellationRequested => html! {
            <CancellationRequestedEvent {version} {is_selected} />
        },
        ExecutionRequest::ComponentUpgradeFinished {
            component_digest,
            deployment_id,
            outcome,
        } => html! {
            <ComponentUpgradeFinishedEvent
                component_digest={component_digest.clone()}
                deployment_id={*deployment_id}
                outcome={outcome.clone()}
                {version}
                {is_selected} />
        },
    }
}
