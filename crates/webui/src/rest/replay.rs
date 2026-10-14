use concepts::ExecutionId;
use rest_types::replay::{
    AdvanceErrorSer, AdvanceRequestSer, AdvanceResponseSer, CapturedWriteSer, ReplayResponseSer,
};
use serde_json::Value;

pub async fn replay(id: &ExecutionId) -> Result<ReplayResponseSer, String> {
    let (_, response): (u16, Value) =
        super::put_empty_allow_conflict(&format!("/v1/executions/{id}/replay")).await?;
    replay_from_json(response)
}

fn replay_from_json(response: Value) -> Result<ReplayResponseSer, String> {
    if let Some(error) = response.get("err").and_then(Value::as_str) {
        return Err(format!("Replay failed: {error}"));
    }
    serde_json::from_value(response).map_err(|err| format!("Invalid replay response: {err}"))
}

pub async fn advance(
    id: &ExecutionId,
    captured_writes: Vec<CapturedWriteSer>,
) -> Result<Result<AdvanceResponseSer, AdvanceErrorSer>, String> {
    let (status, response): (u16, Value) = super::put_allow_unprocessable(
        &format!("/v1/executions/{id}/advance"),
        &AdvanceRequestSer {
            captured_writes,
            persist_backtrace: true,
        },
    )
    .await?;
    if status == 200 {
        serde_json::from_value(response)
            .map(Ok)
            .map_err(|err| format!("Invalid advance response: {err}"))
    } else {
        serde_json::from_value(response.clone())
            .map(Err)
            .map_err(|_| format!("Advance failed: {response}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concepts::storage::{ExecutionRequest, HistoryEvent, JoinSetRequest};
    use rest_types::replay::ReplayOutcomeSer;

    #[test]
    fn unavailable_component_is_reported_as_replay_error() {
        let response = serde_json::json!({"err": "component not found"});
        assert_eq!(
            replay_from_json(response).unwrap_err(),
            "Replay failed: component not found"
        );
    }

    #[test]
    fn captured_delay_pause_round_trips() {
        let response = serde_json::json!({
            "type": "advanceable",
            "replayed_event_count": 3,
            "replay_duration_ms": 1,
            "replay_version": 3,
            "captured_writes": [{
                "type": "append",
                "execution_id": "E_01KABCDEFGHJKMNPQRSTVWXYZ0",
                "version": 3,
                "event": {
                    "created_at": "2026-01-01T00:00:00Z",
                    "event": {"history_event": {"event": {
                        "type": "join_set_request",
                        "join_set_id": "n:timers",
                        "request": {
                            "type": "delay_request",
                            "delay_id": "Delay_01KABCDEFGHJKMNPQRSTVWXYZ0.n:timers_1",
                            "expires_at": "2026-01-01T00:00:01Z",
                            "schedule_at": "now",
                            "paused": false
                        }
                    }}}
                }
            }]
        });
        let response = replay_from_json(response).unwrap();
        let ReplayOutcomeSer::Advanceable {
            mut captured_writes,
        } = response.outcome
        else {
            panic!("expected advanceable")
        };
        let CapturedWriteSer::Append { event, .. } = &mut captured_writes[0] else {
            panic!("expected append")
        };
        let ExecutionRequest::HistoryEvent {
            event:
                HistoryEvent::JoinSetRequest {
                    request: JoinSetRequest::DelayRequest { paused, .. },
                    ..
                },
        } = &mut event.event
        else {
            panic!("expected delay request")
        };
        *paused = true;
        let json = serde_json::to_value(&captured_writes[0]).unwrap();
        assert_eq!(
            json["event"]["event"]["history_event"]["event"]["request"]["paused"],
            true
        );
    }
}
