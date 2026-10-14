use concepts::{ComponentId, prefixed_ulid::DeploymentId};
use rest_types::{
    admin::DeleteDeploymentResponse,
    deployment::{DeploymentRecordSer, DeploymentStateSer},
};

pub async fn delete(
    deployment_id: &DeploymentId,
    delete_executions: bool,
) -> Result<DeleteDeploymentResponse, String> {
    super::delete(
        &format!("/v1/admin/deployments/{deployment_id}"),
        &[
            ("delete_executions", delete_executions.to_string()),
            ("force_non_terminal", "false".to_string()),
        ],
    )
    .await
}

pub async fn get(id: &DeploymentId) -> Result<DeploymentRecordSer, String> {
    super::get(
        &format!("/v1/deployments/{id}"),
        &[("include_generated_metadata", "false".to_string())],
    )
    .await
}

pub async fn file(digest: &str) -> Result<String, String> {
    let bytes = super::get_bytes(&format!("/v1/files/{digest}")).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn component_source(component_id: &ComponentId, file: &str) -> Result<String, String> {
    super::get_text(
        &format!("/v1/components/{}/source", component_id.component_digest),
        &[("file", file.to_string())],
    )
    .await
}

pub async fn list(
    cursor: Option<&DeploymentId>,
    direction: &str,
    length: u32,
    including_cursor: bool,
    include_derived: bool,
    include_component_summary: bool,
) -> Result<Vec<DeploymentStateSer>, String> {
    let mut query = vec![
        ("direction", direction.to_string()),
        ("length", length.to_string()),
        ("including_cursor", including_cursor.to_string()),
        ("include_derived", include_derived.to_string()),
        (
            "include_component_summary",
            include_component_summary.to_string(),
        ),
        ("include_execution_counts", "true".to_string()),
    ];
    if let Some(cursor) = cursor {
        query.push(("cursor_from", cursor.to_string()));
    }
    super::get("/v1/deployments", &query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rest_types::deployment::{DeploymentComponentType, DeploymentStatusSer};

    #[test]
    fn deployment_state_preserves_counts_and_component_type() {
        let state: DeploymentStateSer = serde_json::from_str(
            r#"{
                "deployment_id":"Dep_01KABCDEFGHJKMNPQRSTVWXYZ0", "description":"example",
                "digest":"sha256:abc", "status":"active",
                "created_at":"2026-09-25T12:00:00Z", "last_active_at":null,
                "locked":1, "pending":2, "scheduled":3, "blocked":4, "paused":5,
                "cancelling":6, "finished_ok":7, "finished_error":8,
                "finished_execution_failure":9,
                "component_summary":{"components":[{"component_type":"activity_exec","count":2}]},
                "deployment_toml":null
            }"#,
        )
        .unwrap();
        assert_eq!(state.status, DeploymentStatusSer::Active);
        assert_eq!(state.cancelling, 6);
        let component = &state.component_summary.unwrap().components[0];
        assert_eq!(
            component.component_type,
            DeploymentComponentType::ActivityExec
        );
        assert_eq!(component.count, 2);
    }
}
