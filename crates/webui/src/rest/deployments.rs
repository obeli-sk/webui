use crate::grpc::grpc_client as grpc;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
struct DeploymentRecord {
    deployment_id: String,
    description: Option<String>,
    digest: String,
    status: String,
    created_at: DateTime<Utc>,
    last_active_at: Option<DateTime<Utc>>,
    deployment_toml: String,
    files: Vec<FileRef>,
}

#[derive(Deserialize)]
struct FileRef {
    path: String,
    digest: String,
    size: u64,
}

#[derive(Deserialize)]
struct DeploymentState {
    deployment_id: String,
    description: Option<String>,
    digest: String,
    status: String,
    created_at: DateTime<Utc>,
    last_active_at: Option<DateTime<Utc>>,
    locked: u32,
    pending: u32,
    scheduled: u32,
    blocked: u32,
    paused: u32,
    cancelling: u32,
    finished_ok: u32,
    finished_error: u32,
    finished_execution_failure: u32,
    component_summary: Option<ComponentSummary>,
    deployment_toml: Option<String>,
}

#[derive(Deserialize)]
struct ComponentSummary {
    components: Vec<ComponentCount>,
}

#[derive(Deserialize)]
struct ComponentCount {
    component_type: String,
    count: u32,
}

pub async fn get(id: &str) -> Result<grpc::Deployment, String> {
    let record: DeploymentRecord = super::get(
        &format!("/v1/deployments/{id}"),
        &[("include_generated_metadata", "false".to_string())],
    )
    .await?;
    record.try_into()
}

pub async fn file(digest: &str) -> Result<String, String> {
    let bytes = super::get_bytes(&format!("/v1/files/{digest}")).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn component_source(
    component_id: &grpc::ComponentId,
    file: &str,
) -> Result<String, String> {
    let digest = component_id
        .digest
        .as_ref()
        .ok_or_else(|| "component has no digest".to_string())?;
    super::get_text(
        &format!("/v1/components/{}/source", digest.digest),
        &[("file", file.to_string())],
    )
    .await
}

pub async fn list(
    cursor: Option<&str>,
    direction: &str,
    length: u32,
    including_cursor: bool,
    include_derived: bool,
    include_component_summary: bool,
) -> Result<Vec<grpc::DeploymentSummary>, String> {
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
    let states: Vec<DeploymentState> = super::get("/v1/deployments", &query).await?;
    states.into_iter().map(TryInto::try_into).collect()
}

fn status(value: &str) -> Result<i32, String> {
    Ok(match value {
        "inactive" => grpc::DeploymentStatus::Inactive,
        "enqueued" => grpc::DeploymentStatus::Enqueued,
        "active" => grpc::DeploymentStatus::Active,
        other => return Err(format!("Unknown deployment status: {other}")),
    } as i32)
}

impl TryFrom<DeploymentRecord> for grpc::Deployment {
    type Error = String;

    fn try_from(record: DeploymentRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            deployment_id: Some(grpc::DeploymentId {
                id: record.deployment_id,
            }),
            status: status(&record.status)?,
            created_at: Some(record.created_at.into()),
            last_active_at: record.last_active_at.map(Into::into),
            deployment_toml: Some(record.deployment_toml),
            description: record.description,
            digest: record.digest,
            files: record
                .files
                .into_iter()
                .map(|file| grpc::FileRef {
                    path: file.path,
                    digest: file.digest,
                    size: file.size,
                })
                .collect(),
        })
    }
}

impl TryFrom<DeploymentState> for grpc::DeploymentSummary {
    type Error = String;

    fn try_from(state: DeploymentState) -> Result<Self, Self::Error> {
        let component_summary = state
            .component_summary
            .map(|summary| {
                summary
                    .components
                    .into_iter()
                    .map(|component| {
                        let component_type = match component.component_type.as_str() {
                            "workflow_wasm" => grpc::DeploymentComponentType::WorkflowWasm,
                            "workflow_js" => grpc::DeploymentComponentType::WorkflowJs,
                            "activity_wasm" => grpc::DeploymentComponentType::ActivityWasm,
                            "activity_js" => grpc::DeploymentComponentType::ActivityJs,
                            "activity_exec" => grpc::DeploymentComponentType::ActivityExec,
                            "activity_vm" => grpc::DeploymentComponentType::ActivityVm,
                            "activity_stub" => grpc::DeploymentComponentType::ActivityStub,
                            "activity_external" => grpc::DeploymentComponentType::ActivityExternal,
                            "webhook_endpoint_wasm" => {
                                grpc::DeploymentComponentType::WebhookEndpointWasm
                            }
                            "webhook_endpoint_js" => {
                                grpc::DeploymentComponentType::WebhookEndpointJs
                            }
                            "cron" => grpc::DeploymentComponentType::Cron,
                            other => {
                                return Err(format!("Unknown deployment component type: {other}"));
                            }
                        };
                        Ok(grpc::DeploymentComponentCount {
                            component_type: component_type as i32,
                            count: component.count,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map(|components| grpc::DeploymentComponentSummary { components })
            })
            .transpose()?;
        Ok(Self {
            deployment: Some(grpc::Deployment {
                deployment_id: Some(grpc::DeploymentId {
                    id: state.deployment_id,
                }),
                status: status(&state.status)?,
                created_at: Some(state.created_at.into()),
                last_active_at: state.last_active_at.map(Into::into),
                deployment_toml: state.deployment_toml,
                description: state.description,
                digest: state.digest,
                files: vec![],
            }),
            execution_summary: Some(grpc::DeploymentExecutionSummary {
                locked: state.locked,
                pending: state.pending,
                scheduled: state.scheduled,
                blocked: state.blocked,
                paused: state.paused,
                finished_ok: state.finished_ok,
                finished_error: state.finished_error,
                finished_execution_failure: state.finished_execution_failure,
                cancelling: state.cancelling,
            }),
            component_summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_state_preserves_counts_and_component_type() {
        let state: DeploymentState = serde_json::from_str(
            r#"{
                "deployment_id":"Dep_123", "description":"example", "digest":"sha256:abc",
                "status":"active", "created_at":"2026-09-25T12:00:00Z", "last_active_at":null,
                "locked":1, "pending":2, "scheduled":3, "blocked":4, "paused":5,
                "cancelling":6, "finished_ok":7, "finished_error":8,
                "finished_execution_failure":9,
                "component_summary":{"components":[{"component_type":"activity_exec","count":2}]},
                "deployment_toml":null
            }"#,
        )
        .unwrap();
        let summary: grpc::DeploymentSummary = state.try_into().unwrap();
        let deployment = summary.deployment.unwrap();
        assert_eq!(deployment.status(), grpc::DeploymentStatus::Active);
        assert_eq!(deployment.deployment_id.unwrap().id, "Dep_123");
        assert_eq!(summary.execution_summary.unwrap().cancelling, 6);
        let component = &summary.component_summary.unwrap().components[0];
        assert_eq!(
            component.component_type(),
            grpc::DeploymentComponentType::ActivityExec
        );
        assert_eq!(component.count, 2);
    }
}
