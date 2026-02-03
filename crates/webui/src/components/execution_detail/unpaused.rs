use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent, grpc::version::VersionType,
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct UnpausedEventProps {
    pub version: VersionType,
    pub is_selected: bool,
}

impl UnpausedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Play,
                label: format!("{}. Unpaused", self.version).to_html(),
                has_caret: false,
                is_selected: self.is_selected,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&root_id),
        )
        .unwrap();

        TreeData::from(tree)
    }
}

#[function_component(UnpausedEvent)]
pub fn unpaused_event(props: &UnpausedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
