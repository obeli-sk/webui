use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent,
    grpc::{grpc_client, version::VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinSetCreatedEventProps {
    pub event: grpc_client::execution_event::history_event::JoinSetCreated,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryJoinSetCreatedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        // Add node for JoinSet ID
        let join_set_id = &self
            .event
            .join_set_id
            .as_ref()
            .expect("join_set_id must be sent");
        tree.insert(
            Node::new(NodeData {
                icon: Icon::History,
                label: html! {
                    <>
                        {self.version}
                        {". Join Set Created: `"}
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

#[component(HistoryJoinSetCreatedEvent)]
pub fn history_join_set_created_event(props: &HistoryJoinSetCreatedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
