use std::{fmt::Display, str::FromStr};

use crate::grpc::grpc_client::ComponentType;

impl Display for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            ComponentType::Unspecified => "unspecified",
            ComponentType::Workflow => "workflow",
            ComponentType::Activity => "activity",
            ComponentType::WebhookEndpoint => "webhook_endpoint",
            ComponentType::ActivityStub => "activity_stub",
            ComponentType::Cron => "cron",
        };
        f.write_str(str)
    }
}

impl FromStr for ComponentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unspecified" => Ok(ComponentType::Unspecified),
            "workflow" => Ok(ComponentType::Workflow),
            "activity" => Ok(ComponentType::Activity),
            "webhook_endpoint" => Ok(ComponentType::WebhookEndpoint),
            "activity_stub" => Ok(ComponentType::ActivityStub),
            "cron" => Ok(ComponentType::Cron),
            _ => Err(format!("invalid ComponentType: {}", s)),
        }
    }
}
