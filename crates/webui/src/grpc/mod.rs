use grpc_client::ComponentType;
use yew::{Html, ToHtml as _};

mod component_id;
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
            ComponentType::Workflow => "Workflow",
            ComponentType::ActivityWasm => "Activity",
            ComponentType::ActivityStub => "Activity Stub",
            ComponentType::ActivityExternal => "External Activity",
            ComponentType::WebhookEndpoint => "Webhook Endpoint",
        }
        .to_html()
    }
}

impl ComponentType {
    pub fn as_icon(&self) -> yewprint::Icon {
        match self {
            ComponentType::Workflow => yewprint::Icon::GanttChart,
            ComponentType::ActivityWasm => yewprint::Icon::CodeBlock,
            ComponentType::ActivityStub | ComponentType::ActivityExternal => yewprint::Icon::Import,
            ComponentType::WebhookEndpoint => yewprint::Icon::GlobeNetwork,
        }
    }

    pub fn as_root_label(&self) -> Html {
        match self {
            ComponentType::Workflow => "Workflows",
            ComponentType::ActivityWasm => "WASM Activities",
            ComponentType::WebhookEndpoint => "Webhooks",
            ComponentType::ActivityStub => "Stub Activities",
            ComponentType::ActivityExternal => "External Activities",
        }
        .to_html()
    }
}
