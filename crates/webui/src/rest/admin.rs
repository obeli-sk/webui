use rest_types::admin::{
    CleanupRequest, CleanupResponse, DeleteResponse, RetainDeploymentsRequest,
    RetainSystemEventsRequest, RetainSystemEventsResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
struct CursorRequest<'a, T> {
    #[serde(flatten)]
    request: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<&'a Value>,
}

// backcompat: 0.43.0-rc.11 responses and the pinned types have no retention cursors.
#[derive(Deserialize)]
pub struct RetentionResponse {
    #[serde(flatten)]
    pub cleanup: CleanupResponse,
    #[serde(default)]
    pub next_execution_cursor: Option<Value>,
    #[serde(default)]
    pub next_deployment_cursor: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetentionPolicy {
    Count(u32),
    MaxAgeSeconds(u64),
}

impl RetentionPolicy {
    fn split(self) -> (Option<u32>, Option<u64>) {
        match self {
            RetentionPolicy::Count(count) => (Some(count), None),
            RetentionPolicy::MaxAgeSeconds(seconds) => (None, Some(seconds)),
        }
    }
}

pub async fn retain_executions(
    policy: RetentionPolicy,
    batch_size: u32,
    force_non_terminal: bool,
    dry_run: bool,
    cursor: Option<&Value>,
) -> Result<RetentionResponse, String> {
    let (retain_count, max_age_seconds) = policy.split();
    super::post(
        "/v1/admin/executions/retain",
        &CursorRequest {
            request: CleanupRequest {
                retain_count,
                max_age_seconds,
                batch_size,
                force_non_terminal,
                dry_run,
            },
            cursor,
        },
    )
    .await
}

pub async fn retain_deployments(
    policy: RetentionPolicy,
    batch_size: u32,
    delete_executions: bool,
    force_non_terminal: bool,
    dry_run: bool,
    cursor: Option<&Value>,
) -> Result<RetentionResponse, String> {
    let (retain_count, max_age_seconds) = policy.split();
    super::post(
        "/v1/admin/deployments/retain",
        &CursorRequest {
            request: RetainDeploymentsRequest {
                retain_count,
                max_age_seconds,
                batch_size,
                delete_executions,
                force_non_terminal,
                dry_run,
            },
            cursor,
        },
    )
    .await
}

pub async fn retain_system_events(
    max_age_seconds: u64,
    batch_size: u32,
    dry_run: bool,
) -> Result<RetainSystemEventsResponse, String> {
    super::post(
        "/v1/admin/system-events/retain",
        &RetainSystemEventsRequest {
            max_age_seconds,
            batch_size,
            dry_run,
        },
    )
    .await
}

pub async fn delete_execution_tree(
    execution_id: &str,
    force_non_terminal: bool,
) -> Result<DeleteResponse, String> {
    super::delete(
        &format!("/v1/admin/executions/{execution_id}"),
        &[("force_non_terminal", force_non_terminal.to_string())],
    )
    .await
}
