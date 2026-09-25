use crate::grpc::grpc_client as grpc;
use crate::grpc::wkt_types;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

thread_local! {
    static CAPTURED_WRITES: RefCell<HashMap<String, Vec<Value>>> = RefCell::new(HashMap::new());
}

pub async fn replay(id: &str) -> Result<grpc::ReplayExecutionResponse, String> {
    let (_, response): (u16, Value) =
        super::put_empty_allow_conflict(&format!("/v1/executions/{id}/replay")).await?;
    replay_from_json(id, &response)
}

fn replay_from_json(id: &str, response: &Value) -> Result<grpc::ReplayExecutionResponse, String> {
    if let Some(error) = response.get("err").and_then(Value::as_str) {
        return Err(format!("Replay failed: {error}"));
    }
    let kind: String = get(response, "type")?;
    let replayed_event_count = response
        .get("replayed_event_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let replay_duration_ms = response
        .get("replay_duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let replay_version = response
        .get("replay_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    use grpc::replay_execution_response as outcome;
    let outcome = match kind.as_str() {
        "advanceable" => {
            let raw: Vec<Value> = get(response, "captured_writes")?;
            let captured_writes = raw.iter().map(map_write).collect::<Result<_, _>>()?;
            CAPTURED_WRITES.with(|cache| {
                cache.borrow_mut().insert(id.to_string(), raw);
            });
            outcome::Outcome::Advanceable(outcome::Advanceable { captured_writes })
        }
        "finished" => outcome::Outcome::Finished(outcome::Finished { result: None }),
        "blocked" => outcome::Outcome::Blocked(outcome::Blocked {}),
        "replay_failed" => {
            let raw: Vec<Value> = get(response, "captured_writes")?;
            let captured_writes = raw.iter().map(map_write).collect::<Result<_, _>>()?;
            CAPTURED_WRITES.with(|cache| {
                cache.borrow_mut().insert(id.to_string(), raw);
            });
            let failure = response
                .get("failure")
                .filter(|v| !v.is_null())
                .map(map_failure)
                .transpose()?;
            outcome::Outcome::ReplayFailed(outcome::ReplayFailed {
                error: get(response, "error")?,
                captured_writes,
                failure,
            })
        }
        other => return Err(format!("Unknown replay outcome: {other}")),
    };
    Ok(grpc::ReplayExecutionResponse {
        outcome: Some(outcome),
        replayed_event_count,
        replay_duration: Some(wkt_types::Duration {
            seconds: (replay_duration_ms / 1000).try_into().unwrap_or(i64::MAX),
            nanos: ((replay_duration_ms % 1000) * 1_000_000) as i32,
        }),
        replay_version,
    })
}

fn get<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(
        value
            .get(key)
            .cloned()
            .ok_or_else(|| format!("Missing {key}: {value}"))?,
    )
    .map_err(|err| format!("Invalid {key}: {err}"))
}

fn map_failure(value: &Value) -> Result<grpc::supported_function_result::ExecutionFailure, String> {
    let kind: String = get(value, "kind")?;
    let kind = match kind.as_str() {
        "timed_out" => grpc::ExecutionFailureKind::TimedOut,
        "nondeterminism_detected" => grpc::ExecutionFailureKind::NondeterminismDetected,
        "out_of_fuel" => grpc::ExecutionFailureKind::OutOfFuel,
        "cancelled" => grpc::ExecutionFailureKind::Cancelled,
        "value_too_large" => grpc::ExecutionFailureKind::ValueTooLarge,
        "uncategorized" => grpc::ExecutionFailureKind::Uncategorized,
        other => return Err(format!("Unknown failure kind: {other}")),
    };
    Ok(grpc::supported_function_result::ExecutionFailure {
        kind: kind as i32,
        reason: get(value, "reason")?,
        detail: get(value, "detail")?,
    })
}

fn id(value: &Value, key: &str) -> Result<grpc::ExecutionId, String> {
    Ok(grpc::ExecutionId {
        id: get(value, key)?,
    })
}

fn optional_id(value: &Value, key: &str) -> Result<Option<grpc::ExecutionId>, String> {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .map(|_| id(value, key))
        .transpose()
}

fn events(value: &Value, version: u32) -> Result<Vec<grpc::ExecutionEvent>, String> {
    let raw: Vec<Value> = get(value, "events")?;
    raw.iter()
        .enumerate()
        .map(|(offset, event)| super::events::append_event(event, version + offset as u32))
        .collect()
}

fn backtraces(value: &Value) -> Result<Vec<grpc::CapturedBacktrace>, String> {
    let raw: Vec<Value> = value
        .get("backtraces")
        .map(|_| get(value, "backtraces"))
        .transpose()?
        .unwrap_or_default();
    raw.iter()
        .map(|item| {
            let response = super::executions::backtrace_from_json(item)?;
            Ok(grpc::CapturedBacktrace {
                execution_id: Some(id(item, "execution_id")?),
                component_id: response.component_id,
                wasm_backtrace: response.wasm_backtrace,
            })
        })
        .collect()
}

fn map_create(value: &Value) -> Result<grpc::CreateExecutionRequest, String> {
    let ffqn: String = get(value, "ffqn")?;
    let params: Value = get(value, "params")?;
    let scheduled_at: DateTime<Utc> = get(value, "scheduled_at")?;
    let created_at: DateTime<Utc> = get(value, "created_at")?;
    Ok(grpc::CreateExecutionRequest {
        execution_id: Some(id(value, "execution_id")?),
        function_name: Some(super::events::function_from_json(&ffqn)?),
        params: Some(wkt_types::Any {
            type_url: format!("urn:obelisk:json:params:{ffqn}"),
            value: serde_json::to_vec(&params).map_err(|err| err.to_string())?,
        }),
        scheduled_at: Some(scheduled_at.into()),
        component_id: Some(super::events::component_from_json(
            value.get("component_id").ok_or("Missing component ID")?,
        )?),
        deployment_id: Some(grpc::DeploymentId {
            id: get(value, "deployment_id")?,
        }),
        parent_execution_id: optional_id(value, "parent_execution_id")?,
        parent_join_set_id: value
            .get("parent_join_set_id")
            .and_then(Value::as_str)
            .map(super::events::join_set_id_from_json)
            .transpose()?,
        created_at: Some(created_at.into()),
        metadata: get(value, "metadata")?,
        paused: get(value, "paused")?,
        scheduled_by: optional_id(value, "scheduled_by")?,
        max_persisted_value_size_bytes: get(value, "max_persisted_value_size_bytes")?,
    })
}

fn map_write(value: &Value) -> Result<grpc::CapturedWrite, String> {
    use grpc::captured_write::{self as write, Write};
    let kind: String = get(value, "type")?;
    let execution_id = Some(id(value, "execution_id")?);
    let version: u32 = get(value, "version")?;
    let write = match kind.as_str() {
        "append" => Write::Append(write::Append {
            execution_id,
            version,
            event: Some(super::events::append_event(
                value.get("event").ok_or("Missing event")?,
                version,
            )?),
            backtraces: backtraces(value)?,
        }),
        "append_batch" => Write::AppendBatch(write::AppendBatch {
            execution_id,
            version,
            events: events(value, version)?,
            backtraces: backtraces(value)?,
        }),
        "append_batch_create_new_execution" => {
            Write::AppendBatchCreateNewExecution(write::AppendBatchCreateNewExecution {
                execution_id,
                version,
                events: events(value, version)?,
                child_requests: get::<Vec<Value>>(value, "child_requests")?
                    .iter()
                    .map(map_create)
                    .collect::<Result<_, _>>()?,
                backtraces: backtraces(value)?,
            })
        }
        "append_batch_with_delay_response" => {
            Write::AppendBatchWithDelayResponse(write::AppendBatchWithDelayResponse {
                execution_id,
                version,
                events: events(value, version)?,
                join_set_id: Some(super::events::join_set_id_from_json(&get::<String>(
                    value,
                    "join_set_id",
                )?)?),
                delay_id: get(value, "delay_id")?,
                backtraces: backtraces(value)?,
            })
        }
        "append_stub_response" => {
            let response = value.get("response").ok_or("Missing stub response")?;
            Write::AppendStubResponse(write::AppendStubResponse {
                execution_id,
                version,
                events: events(value, version)?,
                parent_execution_id: Some(id(response, "parent_execution_id")?),
                join_set_id: Some(super::events::join_set_id_from_json(&get::<String>(
                    response,
                    "join_set_id",
                )?)?),
                child_execution_id: Some(id(response, "child_execution_id")?),
                result: Some(super::events::result_from_json(
                    response.get("result").ok_or("Missing stub result")?,
                )?),
                finished_version: get(response, "finished_version")?,
                backtraces: backtraces(value)?,
            })
        }
        "append_finished" => Write::AppendFinished(write::AppendFinished {
            execution_id,
            version,
            event: Some(grpc::execution_event::Finished {
                value: Some(super::events::result_from_json(
                    value.get("retval").ok_or("Missing retval")?,
                )?),
                http_client_traces: Vec::new(),
            }),
            parent_execution_id: optional_id(value, "parent_execution_id")?,
            parent_join_set_id: value
                .get("parent_join_set_id")
                .and_then(Value::as_str)
                .map(super::events::join_set_id_from_json)
                .transpose()?,
        }),
        other => return Err(format!("Unknown captured write: {other}")),
    };
    Ok(grpc::CapturedWrite { write: Some(write) })
}

fn sync_delay_flag(raw: &mut Value, event: &grpc::ExecutionEvent) -> Result<(), String> {
    use grpc::execution_event::{Event, history_event};
    let Some(Event::HistoryVariant(history)) = &event.event else {
        return Ok(());
    };
    let Some(history_event::Event::JoinSetRequest(req)) = &history.event else {
        return Ok(());
    };
    let Some(history_event::join_set_request::JoinSetRequest::DelayRequest(delay)) =
        &req.join_set_request
    else {
        return Ok(());
    };
    let slot = raw
        .get_mut("event")
        .and_then(|v| v.get_mut("history_event"))
        .and_then(|v| v.get_mut("event"))
        .and_then(|v| v.get_mut("request"))
        .and_then(Value::as_object_mut)
        .ok_or("Missing raw delay request")?;
    slot.insert("paused".to_string(), Value::Bool(delay.paused));
    Ok(())
}

fn sync_events(raw: &mut Value, events: &[grpc::ExecutionEvent]) -> Result<(), String> {
    let raw_events = raw
        .get_mut("events")
        .and_then(Value::as_array_mut)
        .ok_or("Missing raw events")?;
    if raw_events.len() != events.len() {
        return Err("Captured event count changed".to_string());
    }
    for (raw, event) in raw_events.iter_mut().zip(events) {
        sync_delay_flag(raw, event)?;
    }
    Ok(())
}

fn sync_write_flags(raw: &mut Value, write: &grpc::CapturedWrite) -> Result<(), String> {
    use grpc::captured_write::Write;
    let kind: String = get(raw, "type")?;
    match (&write.write, kind.as_str()) {
        (Some(Write::Append(append)), "append") => {
            let raw = raw.get_mut("event").ok_or("Missing raw append event")?;
            if let Some(event) = &append.event {
                sync_delay_flag(raw, event)?;
            }
        }
        (Some(Write::AppendBatch(batch)), "append_batch") => sync_events(raw, &batch.events)?,
        (Some(Write::AppendBatchWithDelayResponse(batch)), "append_batch_with_delay_response") => {
            sync_events(raw, &batch.events)?
        }
        (
            Some(Write::AppendBatchCreateNewExecution(batch)),
            "append_batch_create_new_execution",
        ) => {
            sync_events(raw, &batch.events)?;
            let children = raw
                .get_mut("child_requests")
                .and_then(Value::as_array_mut)
                .ok_or("Missing raw child requests")?;
            if children.len() != batch.child_requests.len() {
                return Err("Captured child count changed".to_string());
            }
            for (child, mapped) in children.iter_mut().zip(&batch.child_requests) {
                child
                    .as_object_mut()
                    .ok_or("Invalid child request")?
                    .insert("paused".to_string(), Value::Bool(mapped.paused));
            }
        }
        (Some(Write::AppendStubResponse(_)), "append_stub_response")
        | (Some(Write::AppendFinished(_)), "append_finished") => {}
        _ => return Err("Captured write differs from replay response".to_string()),
    }
    Ok(())
}

pub async fn advance(
    id: &str,
    writes: &[grpc::CapturedWrite],
) -> Result<grpc::AdvanceExecutionResponse, String> {
    let mut raw = CAPTURED_WRITES
        .with(|cache| cache.borrow().get(id).cloned())
        .ok_or("No replay data available for advance")?;
    if raw.len() != writes.len() {
        return Err("Captured write count changed".to_string());
    }
    for (raw, write) in raw.iter_mut().zip(writes) {
        sync_write_flags(raw, write)?;
    }
    let (status, response): (u16, Value) = super::put_allow_unprocessable(
        &format!("/v1/executions/{id}/advance"),
        &serde_json::json!({"captured_writes": raw, "persist_backtrace": true}),
    )
    .await?;
    use grpc::advance_execution_response as outcome;
    let kind: String = get(&response, "type")?;
    let result = if status == 200 {
        match kind.as_str() {
            "finished" | "in_progress" => {
                outcome::Result::Success(outcome::Success { finished: None })
            }
            other => return Err(format!("Unknown advance outcome: {other}")),
        }
    } else {
        let error = match kind.as_str() {
            "version_mismatch" => {
                outcome::error::Error::VersionMismatch(outcome::error::VersionMismatch {
                    expected: get(&response, "expected")?,
                })
            }
            "replay_mismatch" => {
                outcome::error::Error::ReplayMismatch(outcome::error::ReplayMismatch {})
            }
            "transient" => outcome::error::Error::TransientError(outcome::error::TransientError {
                message: response
                    .as_str()
                    .unwrap_or("Transient advance error")
                    .to_string(),
            }),
            other => return Err(format!("Unknown advance error: {other}")),
        };
        outcome::Result::Error(outcome::Error { error: Some(error) })
    };
    Ok(grpc::AdvanceExecutionResponse {
        result: Some(result),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_component_is_reported_as_replay_error() {
        let response = serde_json::json!({"err": "component not found"});
        assert_eq!(
            replay_from_json("E_123", &response).unwrap_err(),
            "Replay failed: component not found"
        );
    }

    #[test]
    fn captured_delay_pause_is_applied_to_rest_write() {
        let mut raw = serde_json::json!({
            "type": "append",
            "execution_id": "execution-id",
            "version": 3,
            "event": {
                "created_at": "2026-01-01T00:00:00Z",
                "event": {"history_event": {"event": {
                    "type": "join_set_request",
                    "join_set_id": "n:timers",
                    "request": {
                        "type": "delay_request",
                        "delay_id": "delay-id",
                        "expires_at": "2026-01-01T00:00:01Z",
                        "schedule_at": "now",
                        "paused": false
                    }
                }}}
            }
        });
        let mut mapped = map_write(&raw).unwrap();
        let Some(grpc::captured_write::Write::Append(append)) = &mut mapped.write else {
            panic!("expected append")
        };
        let Some(grpc::execution_event::Event::HistoryVariant(history)) =
            &mut append.event.as_mut().unwrap().event
        else {
            panic!("expected history")
        };
        let Some(grpc::execution_event::history_event::Event::JoinSetRequest(req)) =
            &mut history.event
        else {
            panic!("expected request")
        };
        let Some(
            grpc::execution_event::history_event::join_set_request::JoinSetRequest::DelayRequest(
                delay,
            ),
        ) = &mut req.join_set_request
        else {
            panic!("expected delay")
        };
        delay.paused = true;
        sync_write_flags(&mut raw, &mapped).unwrap();
        assert_eq!(
            raw["event"]["event"]["history_event"]["event"]["request"]["paused"],
            true
        );
    }
}
