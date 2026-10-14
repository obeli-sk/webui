use concepts::{
    ExecutionId,
    storage::{Created, ExecutionRequest},
};
use rest_types::executions::{ExecutionEventsAndResponsesResponse, ExecutionEventsResponse};

pub async fn history_page(
    id: &ExecutionId,
    version_from: u32,
    responses_cursor_from: u32,
    length: u32,
) -> Result<ExecutionEventsAndResponsesResponse, String> {
    super::get(
        &format!("/v1/executions/{id}/events-and-responses"),
        &[
            ("version_from", version_from.to_string()),
            ("events_length", length.to_string()),
            ("responses_cursor_from", responses_cursor_from.to_string()),
            ("responses_length", length.to_string()),
            (
                "responses_including_cursor",
                (responses_cursor_from == 0).to_string(),
            ),
            ("include_backtrace_id", "true".to_string()),
        ],
    )
    .await
}

pub async fn child_created(id: &ExecutionId) -> Result<Option<Created>, String> {
    let page: ExecutionEventsResponse = super::get(
        &format!("/v1/executions/{id}/events"),
        &[
            ("version", "0".to_string()),
            ("length", "1".to_string()),
            ("including_cursor", "true".to_string()),
        ],
    )
    .await?;
    page.events
        .into_iter()
        .next()
        .map(|event| match event.event {
            ExecutionRequest::Created(created) => Ok(created),
            _ => Err("First execution event is not Created".to_string()),
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use concepts::storage::{
        ExecutionEvent, HistoryEvent, JoinSetRequest, JoinSetResponse, ResponseWithCursor,
    };

    use super::*;

    const EXECUTION_ID: &str = "E_01KABCDEFGHJKMNPQRSTVWXYZ0";
    const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn created_event_preserves_parent_and_json_params() {
        let created: Created = serde_json::from_value(serde_json::json!({
            "ffqn": "example:pkg/ifc.run",
            "params": [42, {"name": "child"}],
            "parent": [EXECUTION_ID, "n:children"],
            "scheduled_at": "2026-01-01T00:00:00Z",
            "component_id": {
                "component_type": "workflow",
                "name": "worker",
                "component_digest": DIGEST
            },
            "deployment_id": "Dep_01KABCDEFGHJKMNPQRSTVWXYZ0",
            "metadata": {"traceparent": "trace"},
            "scheduled_by": null,
            "max_persisted_value_size_bytes": 1024
        }))
        .unwrap();
        let (parent_id, join_set_id) = created.parent.unwrap();
        assert_eq!(parent_id.to_string(), EXECUTION_ID);
        assert_eq!(join_set_id.name.as_ref(), "children");
        assert_eq!(
            created.params.as_json_values().unwrap(),
            &[serde_json::json!(42), serde_json::json!({"name": "child"})]
        );
    }

    #[test]
    fn history_event_and_response_parse_join_set_ids() {
        let event: ExecutionEvent = serde_json::from_value(serde_json::json!({
            "created_at": "2026-01-01T00:00:00Z",
            "version": 3,
            "backtrace_id": 2,
            "event": {"history_event": {"event": {
                "type": "join_set_request",
                "join_set_id": "n:children",
                "request": {
                    "type": "child_execution_request",
                    "child_execution_id": format!("{EXECUTION_ID}.n:children_1"),
                    "target_ffqn": "example:pkg/ifc.run",
                    "params": [42],
                    "result": {"Ok": null}
                }
            }}}
        }))
        .unwrap();
        let ExecutionRequest::HistoryEvent {
            event:
                HistoryEvent::JoinSetRequest {
                    join_set_id,
                    request: JoinSetRequest::ChildExecutionRequest { .. },
                },
        } = event.event
        else {
            panic!("expected child execution request")
        };
        assert_eq!(join_set_id.name.as_ref(), "children");

        let response: ResponseWithCursor = serde_json::from_value(serde_json::json!({
            "cursor": 4,
            "event": {
                "created_at": "2026-01-01T00:00:01Z",
                "event": {
                    "join_set_id": "n:children",
                    "event": {
                        "type": "delay_finished",
                        "delay_id": "Delay_01KABCDEFGHJKMNPQRSTVWXYZ0.n:children_1".to_string(),
                        "result": {"Ok": null}
                    }
                }
            }
        }))
        .unwrap();
        assert_eq!(response.cursor.0, 4);
        assert_eq!(response.event.event.join_set_id.name.as_ref(), "children");
        assert!(matches!(
            response.event.event.event,
            JoinSetResponse::DelayFinished { .. }
        ));
    }
}
