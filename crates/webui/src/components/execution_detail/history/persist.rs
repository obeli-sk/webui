use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent,
    grpc::{grpc_client, version::VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryPersistEventProps {
    pub event: grpc_client::execution_event::history_event::Persist,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryPersistEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        use grpc_client::execution_event::history_event::persist::persist_kind;

        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let detail = self
            .event
            .kind
            .as_ref()
            .and_then(|pk| pk.variant.as_ref())
            .map(|v| match v {
                persist_kind::Variant::RandomString(rs) => {
                    format!(
                        "RandomString [{}..{})",
                        rs.min_length, rs.max_length_exclusive
                    )
                }
                persist_kind::Variant::RandomU64(ru) => {
                    format!("RandomU64 [{}..={}]", ru.min, ru.max_inclusive)
                }
                persist_kind::Variant::ExecutionId(_) => "ExecutionId".to_string(),
            })
            .unwrap_or_default();

        let label = if detail.is_empty() {
            format!("{}. Persist Event", self.version)
        } else {
            format!("{}. Persist: {detail}", self.version)
        };

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
