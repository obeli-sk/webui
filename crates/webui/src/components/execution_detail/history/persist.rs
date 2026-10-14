use crate::components::execution_detail::tree_component::TreeComponent;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::storage::{PersistKind, VersionType};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryPersistEventProps {
    pub kind: PersistKind,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryPersistEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let detail = match &self.kind {
            PersistKind::RandomString {
                min_length,
                max_length_exclusive,
            } => format!("RandomString [{min_length}..{max_length_exclusive})"),
            PersistKind::RandomU64 { min, max_inclusive } => {
                format!("RandomU64 [{min}..={max_inclusive}]")
            }
            PersistKind::ExecutionId => "ExecutionId".to_string(),
        };
        let label = format!("{}. Persist: {detail}", self.version);

        tree.insert(
            Node::new(NodeData {
                icon: Icon::History,
                label: label.into(),
                is_selected: self.is_selected,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&root_id),
        )
        .unwrap();
        TreeData::from(tree)
    }
}

#[component(HistoryPersistEvent)]
pub fn history_persist_event(props: &HistoryPersistEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
