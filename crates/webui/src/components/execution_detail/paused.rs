use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent, grpc::version::VersionType,
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct PausedEventProps {
    pub version: VersionType,
    pub is_selected: bool,
}

impl PausedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Pause,
                label: format!("{}. Paused", self.version).to_html(),
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

#[function_component(PausedEvent)]
pub fn paused_event(props: &PausedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
