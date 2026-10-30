use crate::model::FinishedStatus;
use concepts::{ExecutionId, storage::PendingState};
use rest_types::{
    backtrace::BacktraceInfoSer,
    executions::{ExecutionListItemSer, ExecutionWithStateSer, RetVal},
};

pub async fn backtrace(id: &ExecutionId, version: u32) -> Result<BacktraceInfoSer, String> {
    let version = if version == 0 {
        "first".to_string()
    } else {
        version.to_string()
    };
    super::get(
        &format!("/v1/executions/{id}/backtrace"),
        &[("version", version)],
    )
    .await
}

pub async fn list(query: &[(&str, String)]) -> Result<Vec<ExecutionListItemSer>, String> {
    super::get("/v1/executions", query).await
}

pub async fn get(id: &ExecutionId) -> Result<ExecutionWithStateSer, String> {
    super::get(&format!("/v1/executions/{id}/status"), &[]).await
}

pub async fn retval(id: &ExecutionId) -> Result<RetVal, String> {
    super::get(&format!("/v1/executions/{id}"), &[]).await
}

pub async fn finished_status(summary: &ExecutionWithStateSer) -> Result<FinishedStatus, String> {
    let PendingState::Finished(finished) = &summary.pending_state else {
        return Err(format!(
            "Execution {} is not finished",
            summary.execution_id
        ));
    };
    let retval = retval(&summary.execution_id).await?;
    Ok(FinishedStatus {
        scheduled_at: summary.first_scheduled_at,
        finished_at: finished.finished_at,
        retval,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use concepts::{
        ExecutionFailureKind,
        storage::{PendingStateFinishedError, PendingStateFinishedResultKind},
    };

    fn finished_result_kind(result_kind: &str) -> PendingStateFinishedResultKind {
        let execution: ExecutionWithStateSer = serde_json::from_str(&format!(
            r#"{{
                "execution_id":"E_01KABCDEFGHJKMNPQRSTVWXYZ0", "ffqn":"test:pkg/ifc.run",
                "pending_state":{{"status":"finished", "version":4,
                    "finished_at":"2026-09-25T12:00:01Z",
                    "result_kind":{result_kind}}},
                "created_at":"2026-09-25T12:00:00Z",
                "first_scheduled_at":"2026-09-25T12:00:00Z",
                "component_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "component_type":"workflow",
                "deployment_id":"Dep_01KABCDEFGHJKMNPQRSTVWXYZ0"
            }}"#
        ))
        .unwrap();
        let PendingState::Finished(finished) = execution.pending_state else {
            panic!("expected finished status")
        };
        finished.result_kind
    }

    #[test]
    fn execution_summary_parses_finished_function_error() {
        assert_eq!(
            finished_result_kind(r#"{"err":"error"}"#),
            PendingStateFinishedResultKind::Err(PendingStateFinishedError::Error)
        );
    }

    #[test]
    fn execution_summary_parses_finished_failure() {
        assert_eq!(
            finished_result_kind(r#"{"err":{"execution_failure":"cancelled"}}"#),
            PendingStateFinishedResultKind::Err(PendingStateFinishedError::ExecutionFailure(
                ExecutionFailureKind::Cancelled
            ))
        );
    }
}
