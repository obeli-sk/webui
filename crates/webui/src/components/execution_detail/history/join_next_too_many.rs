use crate::grpc::version::VersionType;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{components::execution_detail::tree_component::TreeComponent, grpc::grpc_client};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextTooManyEventProps {
    pub event: grpc_client::execution_event::history_event::JoinNextTooMany,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryJoinNextTooManyEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        // Add node for JoinSet ID and details
        let join_set_id = self
            .event
            .join_set_id
            .as_ref()
            .expect("JoinNextTooMany.join_set_id is sent");

        let icon = Icon::Error;

        tree.insert(
            Node::new(NodeData {
                icon,
                label: html! {
                    <>
                        {self.version}
                        {". Join Next (more than submissions) : `"}
                        {join_set_id}
                        {"`"}
                    </>
                },
                is_selected: self.is_selected,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&root_id),
        )
        .unwrap();
        TreeData::from(tree)
    }
}

#[component(HistoryJoinNextTooManyEvent)]
pub fn history_join_next_too_many_event(props: &HistoryJoinNextTooManyEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
