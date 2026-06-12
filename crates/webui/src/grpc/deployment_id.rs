use crate::grpc::grpc_client::DeploymentId;
use std::{fmt::Display, str::FromStr};

impl From<String> for DeploymentId {
    fn from(id: String) -> Self {
        DeploymentId { id }
    }
}

impl Display for DeploymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl FromStr for DeploymentId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DeploymentId { id: s.to_string() })
    }
}
