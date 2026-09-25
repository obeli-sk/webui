use crate::grpc::{ffqn::FunctionFqn, grpc_client as grpc};
use chrono::{DateTime, Utc};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::HashMap, str::FromStr};

#[derive(Deserialize)]
struct EventPage {
    events: Vec<EventRecord>,
}

#[derive(Deserialize)]
struct EventRecord {
    event: Value,
}

#[derive(Deserialize)]
struct HistoryPage {
    events: Vec<HistoryRecord>,
    responses: Vec<Value>,
    current_status: super::executions::ExecutionWithState,
    max_version: u32,
    max_cursor: u32,
}

#[derive(Deserialize)]
struct HistoryRecord {
    created_at: DateTime<Utc>,
    version: u32,
    backtrace_id: Option<u32>,
    event: Value,
}

pub async fn history_page(
    id: &str,
    version_from: u32,
    responses_cursor_from: u32,
    length: u32,
) -> Result<grpc::ListExecutionEventsAndResponsesResponse, String> {
    let page: HistoryPage = super::get(
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
    .await?;
    let summary: grpc::ExecutionSummary = page.current_status.try_into()?;
    Ok(grpc::ListExecutionEventsAndResponsesResponse {
        events: page
            .events
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?,
        responses: page
            .responses
            .into_iter()
            .map(map_response)
            .collect::<Result<_, _>>()?,
        current_status: summary.current_status,
        max_version: page.max_version,
        max_cursor: page.max_cursor,
    })
}

fn field<T: DeserializeOwned>(value: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(
        value
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Missing {name}: {value}"))?,
    )
    .map_err(|err| format!("Invalid {name}: {err}"))
}

fn variant(value: &Value) -> Result<(&str, &Value), String> {
    if let Some(name) = value.as_str() {
        return Ok((name, &Value::Null));
    }
    let object = value
        .as_object()
        .ok_or_else(|| format!("Invalid event: {value}"))?;
    let (name, data) = object
        .iter()
        .next()
        .ok_or_else(|| "Empty event".to_string())?;
    Ok((name, data))
}

fn timestamp(value: &Value) -> Result<prost_wkt_types::Timestamp, String> {
    let datetime: DateTime<Utc> =
        serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
    Ok(datetime.into())
}

fn timestamp_field(
    value: &Value,
    name: &str,
) -> Result<Option<prost_wkt_types::Timestamp>, String> {
    value.get(name).map(timestamp).transpose()
}

fn component(value: &Value) -> Result<grpc::ComponentId, String> {
    let component: ComponentId =
        serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
    let component_type = match component.component_type.as_str() {
        "workflow" => grpc::ComponentType::Workflow,
        "activity" => grpc::ComponentType::Activity,
        "activity_stub" => grpc::ComponentType::ActivityStub,
        "webhook_endpoint" => grpc::ComponentType::WebhookEndpoint,
        "cron" => grpc::ComponentType::Cron,
        other => return Err(format!("Unknown component type: {other}")),
    };
    Ok(grpc::ComponentId {
        component_type: component_type as i32,
        name: component.name,
        digest: Some(grpc::ContentDigest {
            digest: component.component_digest,
        }),
    })
}

fn json_any(value: &Value, type_url: &str) -> Result<prost_wkt_types::Any, String> {
    Ok(prost_wkt_types::Any {
        type_url: type_url.to_string(),
        value: serde_json::to_vec(value).map_err(|err| err.to_string())?,
    })
}

fn result(value: &Value) -> Result<grpc::SupportedFunctionResult, String> {
    use grpc::supported_function_result::{
        ErrorPayload, ExecutionFailure, OkPayload, Value as ResultValue,
    };
    let (kind, data) = variant(value)?;
    let (value, wit_type_inline) = match kind {
        "ok" | "err" => {
            let payload = if data.is_null() {
                None
            } else {
                Some(json_any(
                    data.get("value")
                        .ok_or_else(|| format!("Missing result value: {data}"))?,
                    "urn:obelisk:json:retval:TBD",
                )?)
            };
            let wit_type = data.get("type").map(ToString::to_string);
            let value = if kind == "ok" {
                ResultValue::Ok(OkPayload {
                    return_value: payload,
                })
            } else {
                ResultValue::Error(ErrorPayload {
                    return_value: payload,
                })
            };
            (value, wit_type)
        }
        "execution_failure" => {
            let failure_kind: String = field(data, "kind")?;
            let kind = match failure_kind.as_str() {
                "timed_out" => grpc::ExecutionFailureKind::TimedOut,
                "nondeterminism_detected" => grpc::ExecutionFailureKind::NondeterminismDetected,
                "out_of_fuel" => grpc::ExecutionFailureKind::OutOfFuel,
                "cancelled" => grpc::ExecutionFailureKind::Cancelled,
                "value_too_large" => grpc::ExecutionFailureKind::ValueTooLarge,
                "uncategorized" => grpc::ExecutionFailureKind::Uncategorized,
                other => return Err(format!("Unknown failure kind: {other}")),
            };
            (
                ResultValue::ExecutionFailure(ExecutionFailure {
                    kind: kind as i32,
                    reason: field(data, "reason")?,
                    detail: field(data, "detail")?,
                }),
                None,
            )
        }
        other => return Err(format!("Unknown execution result: {other}")),
    };
    Ok(grpc::SupportedFunctionResult {
        value: Some(value),
        wit_type_inline,
    })
}

fn http_traces(value: &Value) -> Result<Vec<grpc::HttpClientTrace>, String> {
    let Some(traces) = value.get("http_client_traces").filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    traces
        .as_array()
        .ok_or_else(|| "Invalid HTTP traces".to_string())?
        .iter()
        .map(|trace| {
            let req = trace
                .get("req")
                .ok_or_else(|| "Missing HTTP request trace".to_string())?;
            let resp = trace.get("resp").filter(|v| !v.is_null());
            let result = resp
                .and_then(|v| v.get("status"))
                .map(|status| {
                    if let Some(ok) = status.get("Ok") {
                        Ok(grpc::http_client_trace::Result::Status(
                            serde_json::from_value(ok.clone()).map_err(|err| err.to_string())?,
                        ))
                    } else if let Some(err) = status.get("Err") {
                        Ok(grpc::http_client_trace::Result::Error(
                            serde_json::from_value(err.clone()).map_err(|err| err.to_string())?,
                        ))
                    } else {
                        Err(format!("Invalid HTTP trace status: {status}"))
                    }
                })
                .transpose()?;
            Ok(grpc::HttpClientTrace {
                sent_at: timestamp_field(req, "sent_at")?,
                uri: field(req, "uri")?,
                method: field(req, "method")?,
                finished_at: resp
                    .map(|v| timestamp_field(v, "finished_at"))
                    .transpose()?
                    .flatten(),
                result,
            })
        })
        .collect()
}

impl TryFrom<HistoryRecord> for grpc::ExecutionEvent {
    type Error = String;

    fn try_from(record: HistoryRecord) -> Result<Self, Self::Error> {
        use grpc::execution_event::{self as event, Event};
        let (kind, data) = variant(&record.event)?;
        let event = match kind {
            "created" => Event::Created(
                serde_json::from_value::<Created>(data.clone())
                    .map_err(|err| err.to_string())?
                    .try_into()?,
            ),
            "locked" => Event::Locked(event::Locked {
                component_id: Some(component(
                    data.get("component_id").ok_or("Missing component ID")?,
                )?),
                deployment_id: Some(grpc::DeploymentId {
                    id: field(data, "deployment_id")?,
                }),
                lock_expires_at: timestamp_field(data, "lock_expires_at")?,
                run_id: field(data, "run_id")?,
                executor_id: field(data, "executor_id")?,
                retry_config: data
                    .get("retry_config")
                    .map(|retry| {
                        Ok::<_, String>(grpc::ComponentRetryConfig {
                            max_retries: field(retry, "max_retries")?,
                            retry_exp_backoff: Some(duration(
                                retry
                                    .get("retry_exp_backoff")
                                    .ok_or("Missing retry backoff")?,
                            )?),
                        })
                    })
                    .transpose()?,
            }),
            "unlocked" => Event::Unlocked(event::Unlocked {
                backoff_expires_at: timestamp_field(data, "backoff_expires_at")?,
                reason: field(data, "reason")?,
            }),
            "temporarily_failed" => Event::TemporarilyFailed(event::TemporarilyFailed {
                reason: field(data, "reason")?,
                detail: field(data, "detail")?,
                backoff_expires_at: timestamp_field(data, "backoff_expires_at")?,
                http_client_traces: http_traces(data)?,
            }),
            "temporarily_timed_out" => Event::TemporarilyTimedOut(event::TemporarilyTimedOut {
                backoff_expires_at: timestamp_field(data, "backoff_expires_at")?,
                http_client_traces: http_traces(data)?,
            }),
            "finished" => Event::Finished(event::Finished {
                value: Some(result(data.get("retval").ok_or("Missing retval")?)?),
                http_client_traces: http_traces(data)?,
            }),
            "history_event" => Event::HistoryVariant(map_history(
                data.get("event").ok_or("Missing history event")?,
            )?),
            "paused" => Event::Paused(event::Paused {}),
            "unpaused" => Event::Unpaused(event::Unpaused {}),
            "cancellation_requested" => {
                Event::CancellationRequested(event::CancellationRequested {})
            }
            "component_upgrade_finished" => Event::ComponentUpgradeFinished(map_upgrade(data)?),
            other => return Err(format!("Unknown execution event: {other}")),
        };
        Ok(Self {
            created_at: Some(record.created_at.into()),
            version: record.version,
            backtrace_id: record.backtrace_id,
            event: Some(event),
        })
    }
}

fn map_upgrade(data: &Value) -> Result<grpc::execution_event::ComponentUpgradeFinished, String> {
    use grpc::execution_event::component_upgrade_finished as upgrade;
    let outcome = data.get("outcome").ok_or("Missing upgrade outcome")?;
    let kind: String = field(outcome, "type")?;
    let outcome = match kind.as_str() {
        "success" => {
            let reason = outcome.get("reason").ok_or("Missing upgrade reason")?;
            let reason_kind: String = field(reason, "type")?;
            let reason = match reason_kind.as_str() {
                "auto" => upgrade::success::Reason::Auto(upgrade::Auto {}),
                "manual" => upgrade::success::Reason::Manual(upgrade::Manual {
                    force: field(reason, "force")?,
                }),
                other => return Err(format!("Unknown upgrade reason: {other}")),
            };
            upgrade::Outcome::Success(upgrade::Success {
                reason: Some(reason),
            })
        }
        "failed" => upgrade::Outcome::Failed(upgrade::Failed {
            reason: field(outcome, "reason")?,
        }),
        other => return Err(format!("Unknown upgrade outcome: {other}")),
    };
    Ok(grpc::execution_event::ComponentUpgradeFinished {
        component_digest: Some(grpc::ContentDigest {
            digest: field(data, "component_digest")?,
        }),
        deployment_id: Some(grpc::DeploymentId {
            id: field(data, "deployment_id")?,
        }),
        outcome: Some(outcome),
    })
}

fn map_response(value: Value) -> Result<grpc::ResponseWithCursor, String> {
    use grpc::join_set_response_event as response;
    let cursor: u32 = field(&value, "cursor")?;
    let outer = value.get("event").ok_or("Missing response event")?;
    let inner = outer.get("event").ok_or("Missing response payload")?;
    let payload = inner.get("event").ok_or("Missing response kind")?;
    let kind: String = field(payload, "type")?;
    let response = match kind.as_str() {
        "delay_finished" => {
            let result = payload.get("result").ok_or("Missing delay result")?;
            response::Response::DelayFinished(response::DelayFinished {
                delay_id: Some(grpc::DelayId {
                    id: field(payload, "delay_id")?,
                }),
                success: result.get("Ok").is_some(),
            })
        }
        "child_execution_finished" => {
            response::Response::ChildExecutionFinished(response::ChildExecutionFinished {
                child_execution_id: Some(grpc::ExecutionId {
                    id: field(payload, "child_execution_id")?,
                }),
                value: Some(result(payload.get("result").ok_or("Missing child result")?)?),
            })
        }
        other => return Err(format!("Unknown response type: {other}")),
    };
    Ok(grpc::ResponseWithCursor {
        cursor,
        event: Some(grpc::JoinSetResponseEvent {
            created_at: timestamp_field(outer, "created_at")?,
            join_set_id: Some(parse_join_set_id(&field::<String>(inner, "join_set_id")?)?),
            response: Some(response),
        }),
    })
}

fn function(value: &str) -> Result<grpc::FunctionName, String> {
    Ok(FunctionFqn::from_str(value)
        .map_err(|err| err.to_string())?
        .into())
}

fn optional_function(value: &Value, name: &str) -> Result<Option<grpc::FunctionName>, String> {
    value
        .get(name)
        .filter(|v| !v.is_null())
        .map(|v| function(v.as_str().ok_or_else(|| format!("Invalid {name}"))?))
        .transpose()
}

fn duration(value: &Value) -> Result<prost_wkt_types::Duration, String> {
    let value: std::time::Duration =
        serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
    Ok(prost_wkt_types::Duration {
        seconds: value
            .as_secs()
            .try_into()
            .map_err(|_| "Duration too long".to_string())?,
        nanos: value.subsec_nanos() as i32,
    })
}

fn map_history(value: &Value) -> Result<grpc::execution_event::HistoryEvent, String> {
    use grpc::execution_event::history_event as history;
    let kind: String = field(value, "type")?;
    let event = match kind.as_str() {
        "persist" => {
            use history::persist::{PersistKind, persist_kind as pk};
            let kind = value.get("kind").ok_or("Missing persist kind")?;
            let kind_name: String = field(kind, "type")?;
            let variant = match kind_name.as_str() {
                "random_u64" => pk::Variant::RandomU64(pk::RandomU64 {
                    min: field(kind, "min")?,
                    max_inclusive: field(kind, "max_inclusive")?,
                }),
                "random_string" => pk::Variant::RandomString(pk::RandomString {
                    min_length: field(kind, "min_length")?,
                    max_length_exclusive: field(kind, "max_length_exclusive")?,
                }),
                "execution_id" => pk::Variant::ExecutionId(pk::ExecutionId {}),
                other => return Err(format!("Unknown persist kind: {other}")),
            };
            let data = value
                .get("value")
                .filter(|v| !v.is_null())
                .map(|v| {
                    Ok::<_, String>(prost_wkt_types::Any {
                        type_url: "unknown".to_string(),
                        value: serde_json::from_value(v.clone()).map_err(|err| err.to_string())?,
                    })
                })
                .transpose()?;
            history::Event::Persist(history::Persist {
                data,
                value_hash: value
                    .get("value_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                kind: Some(PersistKind {
                    variant: Some(variant),
                }),
            })
        }
        "join_set_create" => history::Event::JoinSetCreated(history::JoinSetCreated {
            join_set_id: Some(parse_join_set_id(&field::<String>(value, "join_set_id")?)?),
        }),
        "join_set_request" => history::Event::JoinSetRequest(map_join_set_request(value)?),
        "join_next" => history::Event::JoinNext(history::JoinNext {
            join_set_id: Some(parse_join_set_id(&field::<String>(value, "join_set_id")?)?),
            run_expires_at: timestamp_field(value, "run_expires_at")?,
            closing: field(value, "closing")?,
            function: optional_function(value, "requested_ffqn")?,
        }),
        "join_next_too_many" => history::Event::JoinNextTooMany(history::JoinNextTooMany {
            join_set_id: Some(parse_join_set_id(&field::<String>(value, "join_set_id")?)?),
            function: optional_function(value, "requested_ffqn")?,
        }),
        "join_next_try" => {
            let outcome: String = field(value, "outcome")?;
            let outcome = match outcome.as_str() {
                "found" => history::join_next_try::Outcome::Found,
                "pending" => history::join_next_try::Outcome::Pending,
                "all_processed" => history::join_next_try::Outcome::AllProcessed,
                other => return Err(format!("Unknown join outcome: {other}")),
            };
            history::Event::JoinNextTry(history::JoinNextTry {
                join_set_id: Some(parse_join_set_id(&field::<String>(value, "join_set_id")?)?),
                outcome: outcome as i32,
            })
        }
        "schedule" => history::Event::Schedule(map_schedule(value)?),
        "stub" => history::Event::Stub(map_stub(value)?),
        other => return Err(format!("Unknown history event: {other}")),
    };
    Ok(grpc::execution_event::HistoryEvent { event: Some(event) })
}

fn schedule_at(
    value: &Value,
) -> Result<
    (
        String,
        Option<prost_wkt_types::Timestamp>,
        Option<prost_wkt_types::Duration>,
    ),
    String,
> {
    let (kind, payload) = variant(value)?;
    match kind {
        "now" => Ok((kind.to_string(), None, None)),
        "at" => Ok((kind.to_string(), Some(timestamp(payload)?), None)),
        "in" => Ok((kind.to_string(), None, Some(duration(payload)?))),
        other => Err(format!("Unknown schedule kind: {other}")),
    }
}

fn schedule_result(value: &Value) -> Result<(&str, Option<String>), String> {
    let (kind, data) = variant(value)?;
    if kind == "Ok" {
        return Ok(("ok", None));
    }
    if kind != "Err" {
        return Err(format!("Invalid result: {value}"));
    }
    let (error, detail) = variant(data)?;
    let detail = if error == "type_check_error" {
        Some(serde_json::from_value::<String>(detail.clone()).map_err(|err| err.to_string())?)
    } else if error == "value_too_large" {
        Some(field::<u64>(detail, "limit")?.to_string())
    } else {
        None
    };
    Ok((error, detail))
}

fn map_join_set_request(
    value: &Value,
) -> Result<grpc::execution_event::history_event::JoinSetRequest, String> {
    use grpc::execution_event::history_event::join_set_request as request;
    let req = value.get("request").ok_or("Missing join set request")?;
    let kind: String = field(req, "type")?;
    let join_set_request = match kind.as_str() {
        "delay_request" => {
            use request::delay_request as delay;
            let (kind, at, in_duration) =
                schedule_at(req.get("schedule_at").ok_or("Missing delay schedule")?)?;
            let variant = match kind.as_str() {
                "now" => delay::scheduled_at::Variant::Now(delay::scheduled_at::Now {}),
                "at" => delay::scheduled_at::Variant::At(delay::scheduled_at::At { at }),
                "in" => {
                    delay::scheduled_at::Variant::In(delay::scheduled_at::In { r#in: in_duration })
                }
                _ => unreachable!(),
            };
            request::JoinSetRequest::DelayRequest(request::DelayRequest {
                delay_id: Some(grpc::DelayId {
                    id: field(req, "delay_id")?,
                }),
                expires_at: timestamp_field(req, "expires_at")?,
                scheduled_at: Some(delay::ScheduledAt {
                    variant: Some(variant),
                }),
                paused: req.get("paused").and_then(Value::as_bool).unwrap_or(false),
            })
        }
        "child_execution_request" => {
            use request::child_execution_request as child;
            let (result_kind, detail) =
                schedule_result(req.get("result").ok_or("Missing child result")?)?;
            let result = if result_kind == "ok" {
                child::Result::Ok(child::Ok {})
            } else {
                let kind = match result_kind {
                    "function_not_found" => child::error::Kind::FunctionNotFound,
                    "type_check_error" => child::error::Kind::TypeCheckError,
                    "value_too_large" => child::error::Kind::ValueTooLarge,
                    other => return Err(format!("Unknown child request error: {other}")),
                };
                child::Result::Error(child::Error {
                    kind: kind as i32,
                    detail,
                })
            };
            let params = req.get("params").ok_or("Missing child params")?;
            let (params, rejected_params) = if let Some(rejected) = params.get("rejected") {
                (
                    None,
                    Some(child::RejectedParams {
                        encoded_size_at_least: field(rejected, "encoded_size_at_least")?,
                    }),
                )
            } else {
                (
                    Some(json_any(
                        params,
                        "urn:obelisk:json:params:child-execution-request",
                    )?),
                    None,
                )
            };
            request::JoinSetRequest::ChildExecutionRequest(request::ChildExecutionRequest {
                child_execution_id: Some(grpc::ExecutionId {
                    id: field(req, "child_execution_id")?,
                }),
                function_name: Some(function(&field::<String>(req, "target_ffqn")?)?),
                params,
                result: Some(result),
                rejected_params,
                params_hash: req
                    .get("params_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        }
        other => return Err(format!("Unknown join set request: {other}")),
    };
    Ok(grpc::execution_event::history_event::JoinSetRequest {
        join_set_id: Some(parse_join_set_id(&field::<String>(value, "join_set_id")?)?),
        join_set_request: Some(join_set_request),
    })
}

fn map_schedule(value: &Value) -> Result<grpc::execution_event::history_event::Schedule, String> {
    use grpc::execution_event::history_event::schedule;
    let (kind, at, in_duration) = schedule_at(value.get("schedule_at").ok_or("Missing schedule")?)?;
    let variant = match kind.as_str() {
        "now" => schedule::scheduled_at::Variant::Now(schedule::scheduled_at::Now {}),
        "at" => schedule::scheduled_at::Variant::At(schedule::scheduled_at::At { at }),
        "in" => {
            schedule::scheduled_at::Variant::In(schedule::scheduled_at::In { r#in: in_duration })
        }
        _ => unreachable!(),
    };
    let (kind, detail) = schedule_result(value.get("result").ok_or("Missing schedule result")?)?;
    let result = if kind == "ok" {
        schedule::Result::Ok(schedule::Ok {})
    } else {
        let kind = match kind {
            "function_not_found" => schedule::error::Kind::FunctionNotFound,
            "type_check_error" => schedule::error::Kind::TypeCheckError,
            "value_too_large" => schedule::error::Kind::ValueTooLarge,
            other => return Err(format!("Unknown schedule error: {other}")),
        };
        schedule::Result::Error(schedule::Error {
            kind: kind as i32,
            detail,
        })
    };
    Ok(grpc::execution_event::history_event::Schedule {
        execution_id: Some(grpc::ExecutionId {
            id: field(value, "execution_id")?,
        }),
        scheduled_at: Some(schedule::ScheduledAt {
            variant: Some(variant),
        }),
        result: Some(result),
        params_hash: value
            .get("params_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn map_stub(value: &Value) -> Result<grpc::execution_event::history_event::Stub, String> {
    use grpc::execution_event::history_event::stub;
    let (kind, detail) = schedule_result(value.get("result").ok_or("Missing stub result")?)?;
    let result = if kind == "ok" {
        stub::Result::Ok(stub::Ok {})
    } else {
        let kind = match kind {
            "execution_not_found" => stub::error::Kind::ExecutionNotFound,
            "type_check_error" => stub::error::Kind::TypeCheckError,
            "conflict" => stub::error::Kind::Conflict,
            "value_too_large" => stub::error::Kind::ValueTooLarge,
            other => return Err(format!("Unknown stub error: {other}")),
        };
        stub::Result::Error(stub::Error {
            kind: kind as i32,
            detail,
        })
    };
    Ok(grpc::execution_event::history_event::Stub {
        execution_id: Some(grpc::ExecutionId {
            id: field(value, "target_execution_id")?,
        }),
        retval_hash: field(value, "retval_hash")?,
        result: Some(result),
    })
}

#[derive(Deserialize)]
struct Created {
    ffqn: String,
    params: serde_json::Value,
    parent: Option<(String, String)>,
    scheduled_at: DateTime<Utc>,
    component_id: ComponentId,
    deployment_id: String,
    metadata: HashMap<String, String>,
    scheduled_by: Option<String>,
    max_persisted_value_size_bytes: u64,
}

#[derive(Deserialize)]
struct ComponentId {
    component_type: String,
    name: String,
    component_digest: String,
}

pub async fn child_created(id: &str) -> Result<Option<grpc::execution_event::Created>, String> {
    let page: EventPage = super::get(
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
        .map(|record| {
            let value = record
                .event
                .get("created")
                .ok_or_else(|| "First execution event is not Created".to_string())?;
            let created: Created =
                serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
            created.try_into()
        })
        .transpose()
}

impl TryFrom<Created> for grpc::execution_event::Created {
    type Error = String;

    fn try_from(value: Created) -> Result<Self, Self::Error> {
        let ffqn = FunctionFqn::from_str(&value.ffqn).map_err(|err| err.to_string())?;
        let component_type = match value.component_id.component_type.as_str() {
            "workflow" => grpc::ComponentType::Workflow,
            "activity" => grpc::ComponentType::Activity,
            "activity_stub" => grpc::ComponentType::ActivityStub,
            "webhook_endpoint" => grpc::ComponentType::WebhookEndpoint,
            "cron" => grpc::ComponentType::Cron,
            other => return Err(format!("Unknown component type: {other}")),
        };
        Ok(Self {
            function_name: Some(ffqn.into()),
            params: Some(prost_wkt_types::Any {
                type_url: format!("urn:obelisk:json:params:{}", value.ffqn),
                value: serde_json::to_vec(&value.params).map_err(|err| err.to_string())?,
            }),
            scheduled_at: Some(value.scheduled_at.into()),
            component_id: Some(grpc::ComponentId {
                component_type: component_type as i32,
                name: value.component_id.name,
                digest: Some(grpc::ContentDigest {
                    digest: value.component_id.component_digest,
                }),
            }),
            deployment_id: Some(grpc::DeploymentId {
                id: value.deployment_id,
            }),
            parent_execution_id: value
                .parent
                .as_ref()
                .map(|(id, _)| grpc::ExecutionId { id: id.clone() }),
            parent_join_set_id: value
                .parent
                .as_ref()
                .map(|(_, id)| parse_join_set_id(id))
                .transpose()?,
            metadata: value.metadata,
            scheduled_by: value.scheduled_by.map(|id| grpc::ExecutionId { id }),
            max_persisted_value_size_bytes: value.max_persisted_value_size_bytes,
        })
    }
}

fn parse_join_set_id(value: &str) -> Result<grpc::JoinSetId, String> {
    let (kind, name) = value
        .split_once(':')
        .ok_or_else(|| format!("Invalid join set ID: {value}"))?;
    let kind = match kind {
        "o" => grpc::join_set_id::JoinSetKind::OneOff,
        "n" => grpc::join_set_id::JoinSetKind::Named,
        "g" => grpc::join_set_id::JoinSetKind::Generated,
        _ => return Err(format!("Invalid join set ID: {value}")),
    };
    Ok(grpc::JoinSetId {
        kind: kind as i32,
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_event_preserves_parent_and_json_params() {
        let created: Created = serde_json::from_value(serde_json::json!({
            "ffqn": "example:pkg/ifc.run",
            "params": [42, {"name": "child"}],
            "parent": ["parent-id", "n:children"],
            "scheduled_at": "2026-01-01T00:00:00Z",
            "component_id": {
                "component_type": "workflow",
                "name": "worker",
                "component_digest": "sha256:abc"
            },
            "deployment_id": "deployment-id",
            "metadata": {"traceparent": "trace"},
            "scheduled_by": null,
            "max_persisted_value_size_bytes": 1024
        }))
        .unwrap();
        let mapped: grpc::execution_event::Created = created.try_into().unwrap();
        assert_eq!(mapped.parent_execution_id.unwrap().id, "parent-id");
        assert_eq!(mapped.parent_join_set_id.unwrap().name, "children");
        assert_eq!(mapped.params.unwrap().value, br#"[42,{"name":"child"}]"#);
    }

    #[test]
    fn history_event_and_response_map_join_set_ids() {
        let event: HistoryRecord = serde_json::from_value(serde_json::json!({
            "created_at": "2026-01-01T00:00:00Z",
            "version": 3,
            "backtrace_id": 2,
            "event": {"history_event": {"event": {
                "type": "join_set_request",
                "join_set_id": "n:children",
                "request": {
                    "type": "child_execution_request",
                    "child_execution_id": "child-id",
                    "target_ffqn": "example:pkg/ifc.run",
                    "params": [42],
                    "result": {"Ok": null}
                }
            }}}
        }))
        .unwrap();
        let mapped: grpc::ExecutionEvent = event.try_into().unwrap();
        let grpc::execution_event::Event::HistoryVariant(history) = mapped.event.unwrap() else {
            panic!("expected history")
        };
        let grpc::execution_event::history_event::Event::JoinSetRequest(req) =
            history.event.unwrap()
        else {
            panic!("expected request")
        };
        assert_eq!(req.join_set_id.unwrap().name, "children");

        let response = serde_json::json!({
            "cursor": 4,
            "event": {
                "created_at": "2026-01-01T00:00:01Z",
                "event": {
                    "join_set_id": "n:children",
                    "event": {
                        "type": "delay_finished",
                        "delay_id": "delay-id",
                        "result": {"Ok": null}
                    }
                }
            }
        });
        let response = map_response(response).unwrap();
        assert_eq!(response.cursor, 4);
        assert_eq!(
            response.event.unwrap().join_set_id.unwrap().name,
            "children"
        );
    }
}
