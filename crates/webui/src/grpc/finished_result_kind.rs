use crate::grpc::grpc_client;

/// The outcome of a finished execution, rendered identically wherever it is
/// shown (execution status badge, trace timeline). Execution failures of any
/// kind collapse to `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum FinishedResultKind {
    #[display("OK")]
    Ok,
    #[display("Error")]
    Error,
    #[display("Failed")]
    Failed,
}

impl From<&grpc_client::result_kind::Value> for FinishedResultKind {
    fn from(value: &grpc_client::result_kind::Value) -> Self {
        match value {
            grpc_client::result_kind::Value::Ok(_) => FinishedResultKind::Ok,
            grpc_client::result_kind::Value::Error(_) => FinishedResultKind::Error,
            grpc_client::result_kind::Value::ExecutionFailureKind(_) => FinishedResultKind::Failed,
        }
    }
}

impl From<&grpc_client::supported_function_result::Value> for FinishedResultKind {
    fn from(value: &grpc_client::supported_function_result::Value) -> Self {
        match value {
            grpc_client::supported_function_result::Value::Ok(_) => FinishedResultKind::Ok,
            grpc_client::supported_function_result::Value::Error(_) => FinishedResultKind::Error,
            grpc_client::supported_function_result::Value::ExecutionFailure(_) => {
                FinishedResultKind::Failed
            }
        }
    }
}
