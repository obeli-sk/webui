use crate::components::execution_detail::tree_component::TreeComponent;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::{
    JoinSetId,
    storage::{JoinNextTryOutcome, VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextTryEventProps {
    pub join_set_id: JoinSetId,
    pub outcome: JoinNextTryOutcome,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryJoinNextTryEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let join_set_id = self.join_set_id.to_string();
        let (icon, status) = match self.outcome {
            JoinNextTryOutcome::Found => (Icon::Tick, "found"),
            JoinNextTryOutcome::Pending => (Icon::Search, "pending"),
            JoinNextTryOutcome::AllProcessed => (Icon::Error, "all processed"),
        };

        tree.insert(
            Node::new(NodeData {
                icon,
                label: html! {
                    <>
                        {self.version}
                        {". Join Next Try ("}
                        {status}
                        {"): `"}
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

#[component(HistoryJoinNextTryEvent)]
pub fn history_join_next_try_event(props: &HistoryJoinNextTryEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
