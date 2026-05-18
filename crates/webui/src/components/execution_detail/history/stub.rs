use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::{
        execution_detail::tree_component::TreeComponent, execution_header::ExecutionLink,
    },
    grpc::{grpc_client, version::VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryStubEventProps {
    pub event: grpc_client::execution_event::history_event::Stub,
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

        let stubbed_execution_id = self
            .event
            .execution_id
            .clone()
            .expect("`execution_id` is sent by the server");

        let is_ok = matches!(
            self.event.result,
            Some(grpc_client::execution_event::history_event::stub::Result::Ok(_))
        );

        let stub_node = tree
            .insert(
                Node::new(NodeData {
                    icon: if is_ok { Icon::Tick } else { Icon::Error },
                    label: html! {<>
                        { self.version }
                        {". Stubbed execution "}
                        { self.link.link(stubbed_execution_id.clone(), &stubbed_execution_id.id) }
                    </>},
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        // retval_hash
        if !self.event.retval_hash.is_empty() {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::IdNumber,
                    label: format!("Return Value Hash: {}", self.event.retval_hash).into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&stub_node),
            )
            .unwrap();
        }

        // Error detail
        if let Some(grpc_client::execution_event::history_event::stub::Result::Error(err)) =
            &self.event.result
        {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Error,
                    label: format!(
                        "Error: {:?}{}",
                        err.kind(),
                        err.detail
                            .as_deref()
                            .map(|d| format!(" - {d}"))
                            .unwrap_or_default()
                    )
                    .into(),
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
