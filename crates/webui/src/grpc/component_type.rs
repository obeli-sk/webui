use std::fmt::Display;

use crate::grpc::grpc_client::ComponentType;

impl Display for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            ComponentType::Unspecified => "unspecified",
            ComponentType::Workflow => "workflow",
            ComponentType::ActivityWasm => "activity_wasm",
            ComponentType::WebhookEndpoint => "webhook_endpoint",
            ComponentType::ActivityStub => "activity_stub",
            ComponentType::ActivityExternal => "activity_external",
        };
        f.write_str(str)
    }
}
