use crate::components::execution_detail::{
    http_trace::attach_http_traces, tree_component::TreeComponent,
};
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use chrono::{DateTime, Utc};
use concepts::storage::{VersionType, http_client_trace::HttpClientTrace};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct TemporarilyFailedEventProps {
    pub backoff_expires_at: DateTime<Utc>,
    pub reason: String,
    pub detail: Option<String>,
    pub http_client_traces: Vec<HttpClientTrace>,
    pub version: VersionType,
    pub is_selected: bool,
}

impl TemporarilyFailedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let failed_node = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Error,
                    label: format!("{}. Temporarily Failed", self.version).into(),
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        // Add reason node
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Error,
                label: self.reason.as_str().into(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&failed_node),
        )
        .unwrap();

        // Add detail
        if let Some(detail) = &self.detail {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::List,
                    label: format!("detail: {detail}").into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&failed_node),
            )
            .unwrap();
        }

        // Add backoff expiration
        let backoff_expires_at = self.backoff_expires_at;
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Time,
                label: format!("Backoff Expires At: {backoff_expires_at}").into(),
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&failed_node),
        )
        .unwrap();

        // Add HTTP Traces
        attach_http_traces(&mut tree, &failed_node, &self.http_client_traces);

        TreeData::from(tree)
    }
}

#[component(TemporarilyFailedEvent)]
pub fn temporarily_failed_event(props: &TemporarilyFailedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
