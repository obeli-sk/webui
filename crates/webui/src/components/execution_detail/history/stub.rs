use crate::components::{
    execution_detail::tree_component::TreeComponent, execution_header::ExecutionLink,
};
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::{
    ExecutionId,
    prefixed_ulid::ExecutionIdDerived,
    storage::{StubError, StubRetValHash, VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryStubEventProps {
    pub target_execution_id: ExecutionIdDerived,
    pub retval_hash: StubRetValHash,
    pub result: Result<(), StubError>,
    pub version: VersionType,
    pub link: ExecutionLink,
    pub is_selected: bool,
}

impl HistoryStubEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let stubbed_execution_id = ExecutionId::Derived(self.target_execution_id.clone());
        let is_ok = self.result.is_ok();

        let stub_node = tree
            .insert(
                Node::new(NodeData {
                    icon: if is_ok { Icon::Tick } else { Icon::Error },
                    label: html! {<>
                        { self.version }
                        {". Stubbed execution "}
                        { self.link.link(stubbed_execution_id.clone(), &stubbed_execution_id.to_string()) }
                    </>},
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::IdNumber,
                label: format!("Return Value Hash: {}", self.retval_hash).into(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&stub_node),
        )
        .unwrap();

        // Error detail
        if let Err(err) = &self.result {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Error,
                    label: format!("Error: {err}").into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&stub_node),
            )
            .unwrap();
        }

        TreeData::from(tree)
    }
}

#[component(HistoryStubEvent)]
pub fn history_stub_event(props: &HistoryStubEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
