use crate::components::execution_detail::{
    http_trace::attach_http_traces, tree_component::TreeComponent,
};
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use chrono::{DateTime, Utc};
use concepts::storage::{VersionType, http_client_trace::HttpClientTrace};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct TemporarilyTimedOutEventProps {
    pub backoff_expires_at: DateTime<Utc>,
    pub http_client_traces: Vec<HttpClientTrace>,
    pub version: VersionType,
    pub is_selected: bool,
}

impl TemporarilyTimedOutEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(
                Node::new(NodeData {
                    data: 0_u32,
                    ..Default::default()
                }),
                InsertBehavior::AsRoot,
            )
            .unwrap();

        let timed_out_node = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Time,
                    label: format!("{}. Temporarily Timed Out", self.version).into(),
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        // Add backoff expiration
        let backoff_expires_at = self.backoff_expires_at;
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Calendar,
                label: format!("Backoff Expires At: {backoff_expires_at}").into(),
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&timed_out_node),
        )
        .unwrap();

        // Add HTTP Traces
        attach_http_traces(&mut tree, &timed_out_node, &self.http_client_traces);

        TreeData::from(tree)
    }
}

#[component(TemporarilyTimedOutEvent)]
pub fn temporarily_timed_out_event(props: &TemporarilyTimedOutEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
