use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetentionPolicy {
    Count(u32),
    MaxAgeSeconds(u64),
}

#[derive(Serialize)]
struct RetainExecutionsRequest {
    retain_count: Option<u32>,
    max_age_seconds: Option<u64>,
    batch_size: u32,
    force_non_terminal: bool,
    dry_run: bool,
}

#[derive(Serialize)]
struct RetainDeploymentsRequest {
    retain_count: Option<u32>,
    max_age_seconds: Option<u64>,
    batch_size: u32,
    delete_executions: bool,
    force_non_terminal: bool,
    dry_run: bool,
}

#[derive(Serialize)]
struct RetainSystemEventsRequest {
    max_age_seconds: u64,
    batch_size: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct CleanupResponse {
    pub deleted_execution_trees: u64,
    pub deleted_deployments: u64,
    pub retained: u64,
    pub blocked_non_terminal: u64,
    pub blocked_by_execution_reference: u64,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct RetainSystemEventsResponse {
    pub deleted: u64,
    pub has_more: bool,
}

#[derive(Deserialize)]
pub struct DeleteResponse {
    pub deleted: bool,
    pub already_deleted: bool,
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
) -> Result<CleanupResponse, String> {
    let (retain_count, max_age_seconds) = policy.split();
    super::post(
        "/v1/admin/executions/retain",
        &RetainExecutionsRequest {
            retain_count,
            max_age_seconds,
            batch_size,
            force_non_terminal,
            dry_run,
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
) -> Result<CleanupResponse, String> {
    let (retain_count, max_age_seconds) = policy.split();
    super::post(
        "/v1/admin/deployments/retain",
        &RetainDeploymentsRequest {
            retain_count,
            max_age_seconds,
            batch_size,
            delete_executions,
            force_non_terminal,
            dry_run,
        },
    )
    .await
}

pub async fn retain_system_events(
    max_age_seconds: u64,
    batch_size: u32,
) -> Result<RetainSystemEventsResponse, String> {
    super::post(
        "/v1/admin/system-events/retain",
        &RetainSystemEventsRequest {
            max_age_seconds,
            batch_size,
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
