use rest_types::admin::{
    CleanupRequest, CleanupResponse, DeleteResponse, RetainDeploymentsRequest,
    RetainSystemEventsRequest, RetainSystemEventsResponse,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

fn parse_cursor<T: DeserializeOwned>(cursor: Option<&Value>) -> Result<Option<T>, String> {
    cursor
        .map(|cursor| serde_json::from_value(cursor.clone()))
        .transpose()
        .map_err(|err| format!("invalid retention cursor: {err}"))
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
) -> Result<CleanupResponse, String> {
    let (retain_count, max_age_seconds) = policy.split();
    super::post(
        "/v1/admin/executions/retain",
        &CleanupRequest {
            retain_count,
            max_age_seconds,
            batch_size,
            force_non_terminal,
            dry_run,
            cursor: parse_cursor(cursor)?,
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
            cursor: parse_cursor(cursor)?,
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
