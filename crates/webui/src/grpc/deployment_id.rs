use crate::grpc::grpc_client::DeploymentId;

impl From<String> for DeploymentId {
    fn from(id: String) -> Self {
        DeploymentId { id }
    }
}
