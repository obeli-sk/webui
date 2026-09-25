use crate::grpc::{ffqn::FunctionFqn, grpc_client as grpc};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Backtrace {
    component_id: BacktraceComponentId,
    version_min_including: u32,
    version_max_excluding: u32,
    wasm_backtrace: BacktraceFrames,
}

#[derive(Deserialize)]
struct BacktraceComponentId {
    component_type: String,
    name: String,
    component_digest: String,
}

#[derive(Deserialize)]
struct BacktraceFrames {
    frames: Vec<BacktraceFrame>,
}

#[derive(Deserialize)]
struct BacktraceFrame {
    module: String,
    func_name: String,
    symbols: Vec<BacktraceSymbol>,
}

#[derive(Deserialize)]
struct BacktraceSymbol {
    func_name: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    col: Option<u32>,
}

pub async fn backtrace(id: &str, version: u32) -> Result<grpc::GetBacktraceResponse, String> {
    let version = if version == 0 {
        "first".to_string()
    } else {
        version.to_string()
    };
    let response: Backtrace = super::get(
        &format!("/v1/executions/{id}/backtrace"),
        &[("version", version)],
    )
    .await?;
    let component_type = match response.component_id.component_type.as_str() {
        "workflow" => grpc::ComponentType::Workflow,
        "activity" => grpc::ComponentType::Activity,
        "activity_stub" => grpc::ComponentType::ActivityStub,
        "webhook_endpoint" => grpc::ComponentType::WebhookEndpoint,
        "cron" => grpc::ComponentType::Cron,
        other => return Err(format!("Unknown component type: {other}")),
    };
    Ok(grpc::GetBacktraceResponse {
        component_id: Some(grpc::ComponentId {
            component_type: component_type as i32,
            name: response.component_id.name,
            digest: Some(grpc::ContentDigest {
                digest: response.component_id.component_digest,
            }),
        }),
        wasm_backtrace: Some(grpc::WasmBacktrace {
            frames: response
                .wasm_backtrace
                .frames
                .into_iter()
                .map(|frame| grpc::FrameInfo {
                    module: frame.module,
                    func_name: frame.func_name,
                    symbols: frame
                        .symbols
                        .into_iter()
                        .map(|symbol| grpc::FrameSymbol {
                            func_name: symbol.func_name,
                            file: symbol.file,
                            line: symbol.line,
                            col: symbol.col,
                        })
                        .collect(),
                })
                .collect(),
            version_min_including: response.version_min_including,
            version_max_excluding: response.version_max_excluding,
        }),
    })
}

#[derive(Deserialize)]
pub struct ExecutionWithState {
    execution_id: String,
    ffqn: String,
    pending_state: Value,
    created_at: DateTime<Utc>,
    first_scheduled_at: DateTime<Utc>,
    component_digest: String,
    component_type: String,
    deployment_id: String,
}

pub async fn list(query: &[(&str, String)]) -> Result<Vec<grpc::ExecutionSummary>, String> {
    let executions: Vec<ExecutionWithState> = super::get("/v1/executions", query).await?;
    executions.into_iter().map(TryInto::try_into).collect()
}

pub async fn get(id: &str) -> Result<grpc::ExecutionSummary, String> {
    let execution: ExecutionWithState =
        super::get(&format!("/v1/executions/{id}/status"), &[]).await?;
    execution.try_into()
}

pub async fn finished_status(
    id: &str,
    summary: &grpc::ExecutionSummary,
) -> Result<grpc::FinishedStatus, String> {
    let retval: Value = super::get(&format!("/v1/executions/{id}"), &[]).await?;
    let value = if let Some(ok) = retval.get("ok") {
        grpc::supported_function_result::Value::Ok(grpc::supported_function_result::OkPayload {
            return_value: payload(ok)?,
        })
    } else if let Some(err) = retval.get("err") {
        grpc::supported_function_result::Value::Error(
            grpc::supported_function_result::ErrorPayload {
                return_value: payload(err)?,
            },
        )
    } else if let Some(failure) = retval.get("execution_failed") {
        let kind = failure
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing execution failure kind".to_string())?;
        grpc::supported_function_result::Value::ExecutionFailure(
            grpc::supported_function_result::ExecutionFailure {
                kind: failure_kind(kind)? as i32,
                reason: failure
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                detail: failure
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
        )
    } else {
        return Err(format!("Unexpected execution result: {retval}"));
    };
    let finished_at = match summary
        .current_status
        .as_ref()
        .and_then(|s| s.status.as_ref())
    {
        Some(grpc::execution_status::Status::Finished(finished)) => finished.finished_at,
        _ => None,
    };
    Ok(grpc::FinishedStatus {
        created_at: summary.created_at,
        scheduled_at: summary.first_scheduled_at,
        finished_at,
        value: Some(grpc::SupportedFunctionResult {
            value: Some(value),
            wit_type_inline: None,
        }),
    })
}

fn payload(value: &Value) -> Result<Option<prost_wkt_types::Any>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(prost_wkt_types::Any {
            type_url: String::new(),
            value: serde_json::to_vec(value).map_err(|error| error.to_string())?,
        }))
    }
}

fn failure_kind(value: &str) -> Result<grpc::ExecutionFailureKind, String> {
    Ok(match value {
        "timed_out" => grpc::ExecutionFailureKind::TimedOut,
        "nondeterminism_detected" => grpc::ExecutionFailureKind::NondeterminismDetected,
        "out_of_fuel" => grpc::ExecutionFailureKind::OutOfFuel,
        "cancelled" => grpc::ExecutionFailureKind::Cancelled,
        "uncategorized" => grpc::ExecutionFailureKind::Uncategorized,
        "value_too_large" => grpc::ExecutionFailureKind::ValueTooLarge,
        other => return Err(format!("Unknown execution failure kind: {other}")),
    })
}

impl TryFrom<ExecutionWithState> for grpc::ExecutionSummary {
    type Error = String;

    fn try_from(value: ExecutionWithState) -> Result<Self, Self::Error> {
        let ffqn: FunctionFqn = value.ffqn.parse().map_err(|error| format!("{error}"))?;
        let component_type = value.component_type.parse::<grpc::ComponentType>()?;
        let status = status(&value.pending_state)?;
        Ok(Self {
            execution_id: Some(grpc::ExecutionId {
                id: value.execution_id,
            }),
            function_name: Some(ffqn.into()),
            current_status: Some(grpc::ExecutionStatus {
                status: Some(status),
                component_digest: Some(grpc::ContentDigest {
                    digest: value.component_digest.clone(),
                }),
            }),
            created_at: Some(value.created_at.into()),
            first_scheduled_at: Some(value.first_scheduled_at.into()),
            component_digest: Some(grpc::ContentDigest {
                digest: value.component_digest,
            }),
            deployment_id: Some(grpc::DeploymentId {
                id: value.deployment_id,
            }),
            component_type: component_type as i32,
        })
    }
}

fn timestamp(value: &Value, key: &str) -> Result<prost_wkt_types::Timestamp, String> {
    let raw = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing {key} in execution state"))?;
    let parsed = DateTime::parse_from_rfc3339(raw).map_err(|error| error.to_string())?;
    Ok(parsed.to_utc().into())
}

fn status(value: &Value) -> Result<grpc::execution_status::Status, String> {
    use grpc::execution_status::{
        BlockedByJoinSet, Cancelling, Finished, Locked, Paused, PendingAt, Status,
    };
    let kind = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing execution status".to_string())?;
    Ok(match kind {
        "locked" => {
            let run_id = value
                .pointer("/locked_by/run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing run ID in locked execution".to_string())?;
            Status::Locked(Locked {
                run_id: Some(grpc::RunId {
                    id: run_id.to_string(),
                }),
                lock_expires_at: Some(timestamp(value, "lock_expires_at")?),
            })
        }
        "pending_at" => Status::PendingAt(PendingAt {
            scheduled_at: Some(timestamp(value, "scheduled_at")?),
        }),
        "blocked_by_join_set" => {
            let raw = value
                .get("join_set_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing join set ID".to_string())?;
            let (kind, name) = raw
                .split_once(':')
                .ok_or_else(|| format!("Invalid join set ID: {raw}"))?;
            let kind = match kind {
                "o" => grpc::join_set_id::JoinSetKind::OneOff,
                "n" => grpc::join_set_id::JoinSetKind::Named,
                "g" => grpc::join_set_id::JoinSetKind::Generated,
                other => return Err(format!("Unknown join set kind: {other}")),
            };
            Status::BlockedByJoinSet(BlockedByJoinSet {
                join_set_id: Some(grpc::JoinSetId {
                    kind: kind as i32,
                    name: name.to_string(),
                }),
                lock_expires_at: Some(timestamp(value, "lock_expires_at")?),
                closing: value
                    .get("closing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        "paused" => Status::Paused(Paused {}),
        "cancelling" => Status::Cancelling(Cancelling {}),
        "finished" => {
            let kind = value
                .get("result_kind")
                .ok_or_else(|| "Missing finished result kind".to_string())?;
            let result_kind = if kind == "ok" {
                grpc::result_kind::Value::Ok(grpc::result_kind::Ok {})
            } else if let Some(error) = kind.get("err") {
                let failure = error
                    .get("execution_failure")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("Unknown finished result: {kind}"))?;
                let failure_kind = failure_kind(failure)?;
                grpc::result_kind::Value::ExecutionFailureKind(failure_kind as i32)
            } else {
                return Err(format!("Unknown finished result: {kind}"));
            };
            Status::Finished(Finished {
                finished_at: Some(timestamp(value, "finished_at")?),
                result_kind: Some(grpc::ResultKind {
                    value: Some(result_kind),
                }),
            })
        }
        other => return Err(format!("Unknown execution status: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_summary_maps_finished_failure() {
        let execution: ExecutionWithState = serde_json::from_str(
            r#"{
                "execution_id":"E_123", "ffqn":"test:pkg/ifc.run",
                "pending_state":{"status":"finished", "version":4,
                    "finished_at":"2026-09-25T12:00:01Z",
                    "result_kind":{"err":{"execution_failure":"cancelled"}}},
                "created_at":"2026-09-25T12:00:00Z",
                "first_scheduled_at":"2026-09-25T12:00:00Z",
                "component_digest":"sha256:abc", "component_type":"workflow",
                "deployment_id":"Dep_123"
            }"#,
        )
        .unwrap();
        let summary: grpc::ExecutionSummary = execution.try_into().unwrap();
        let Some(grpc::execution_status::Status::Finished(finished)) =
            summary.current_status.unwrap().status
        else {
            panic!("expected finished status")
        };
        assert_eq!(
            finished.result_kind.unwrap().value,
            Some(grpc::result_kind::Value::ExecutionFailureKind(
                grpc::ExecutionFailureKind::Cancelled as i32
            ))
        );
        assert_eq!(summary.function_name.unwrap().function_name, "run");
    }
}
