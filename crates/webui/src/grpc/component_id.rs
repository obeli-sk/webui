use crate::grpc::grpc_client::ComponentType;

use super::grpc_client::ComponentId;
use std::{fmt::Display, str::FromStr};

impl Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.component_type().as_str_name(),
            self.name,
            self.input_sha256_digest
        )
    }
}

impl FromStr for ComponentId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, ':');

        let component_type = parts
            .next()
            .and_then(ComponentType::from_str_name)
            .ok_or(())?;

        let name = parts.next().ok_or(())?.to_string();
        let input_sha256_digest = parts.next().ok_or(())?.to_string();

        Ok(Self {
            component_type: component_type.into(),
            name,
            input_sha256_digest,
        })
    }
}
