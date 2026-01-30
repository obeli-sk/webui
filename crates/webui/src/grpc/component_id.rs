use super::grpc_client::ComponentId;
use crate::grpc::grpc_client::{self, ContentDigest};
use std::{fmt::Display, str::FromStr};

impl Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.component_type(),
            self.name,
            self.digest.as_ref().expect("`digest` is sent").digest
        )
    }
}

impl FromStr for ComponentId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, ':');

        let component_type = grpc_client::ComponentType::from_str(
            parts
                .next()
                .ok_or_else(|| format!("delimiter not found in `{s}`"))?,
        )?;

        let name = parts
            .next()
            .ok_or_else(|| format!("delimiter not found in `{s}`"))?
            .to_string();
        let digest = parts
            .next()
            .ok_or_else(|| format!("delimiter not found in `{s}`"))?
            .to_string();

        Ok(Self {
            component_type: component_type.into(),
            name,
            digest: Some(ContentDigest { digest }),
        })
    }
}
