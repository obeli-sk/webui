use crate::tree::Icon;
use grpc_client::ComponentType;
use yew::{Html, ToHtml as _};

mod component_id;
pub mod component_type;
pub mod content_digest;
pub mod delay_id;
pub mod deployment_id;
pub mod execution_id;
pub mod ffqn;
pub mod function_detail;
pub mod grpc_client;
pub mod ifc_fqn;
pub mod join_set_id;
pub mod pkg_fqn;
pub mod version;

pub const NAMESPACE_OBELISK: &str = "obelisk"; // TODO: unify with concepts
pub const SUFFIX_PKG_EXT: &str = "-obelisk-ext"; // TODO: unify with concepts
pub const SUFFIX_PKG_STUB: &str = "-obelisk-stub"; // TODO: unify with concepts

impl grpc_client::Component {
    pub fn as_type(&self) -> ComponentType {
        self.component_id
            .as_ref()
            .expect("`component_id` is sent")
            .component_type()
    }
}

impl yew::ToHtml for ComponentType {
    fn to_html(&self) -> yew::Html {
        match self {
            ComponentType::Unspecified => "Unspecified",
            ComponentType::Workflow => "Workflow",
            ComponentType::Activity => "Activity",
            ComponentType::ActivityStub => "Activity Stub",
            ComponentType::WebhookEndpoint => "Webhook Endpoint",
            ComponentType::Cron => "Cron",
        }
        .to_html()
    }
}

impl ComponentType {
    pub fn as_icon(&self) -> Icon {
        match self {
            ComponentType::Unspecified => Icon::Cog,
            ComponentType::Workflow => Icon::GanttChart,
            ComponentType::Activity => Icon::CodeBlock,
            ComponentType::ActivityStub => Icon::Import,
            ComponentType::WebhookEndpoint => Icon::GlobeNetwork,
            ComponentType::Cron => Icon::Calendar,
        }
    }

    pub fn as_root_label(&self) -> Html {
        match self {
            ComponentType::Unspecified => "Unspecified",
            ComponentType::Workflow => "Workflows",
            ComponentType::Activity => "Activities",
            ComponentType::WebhookEndpoint => "Webhooks",
            ComponentType::ActivityStub => "Stub Activities",
            ComponentType::Cron => "Crons",
        }
        .to_html()
    }
}
