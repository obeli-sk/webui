use crate::app::query::BacktraceVersionsPath;
use crate::grpc::grpc_client::ExecutionId;
use crate::grpc::version::VersionType;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    app::Route, components::execution_detail::tree_component::TreeComponent, grpc::grpc_client,
};
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextTryEventProps {
    pub event: grpc_client::execution_event::history_event::JoinNextTry,
    pub execution_id: ExecutionId,
    pub backtrace_id: Option<VersionType>,
    pub version: VersionType,
    pub is_selected: bool,
}

impl HistoryJoinNextTryEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let join_set_id = self
            .event
            .join_set_id
            .as_ref()
            .expect("JoinNextTry.join_set_id is sent");

        let icon = if self.event.found_response {
            Icon::Tick
        } else {
            Icon::Search
        };

        let status = if self.event.found_response {
            "found"
        } else {
            "pending"
        };

        let join_next_try_node = tree
            .insert(
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
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        if let Some(backtrace_id) = self.backtrace_id {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Flows,
                    label: html! {
                        <Link<Route> to={Route::ExecutionDebuggerWithVersions { execution_id: self.execution_id.clone(), versions: BacktraceVersionsPath::from(backtrace_id) } }>
                            {"Backtrace"}
                        </Link<Route>>
                    },
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&join_next_try_node),
            )
            .unwrap();
        }
        TreeData::from(tree)
    }
}

#[function_component(HistoryJoinNextTryEvent)]
pub fn history_join_next_try_event(props: &HistoryJoinNextTryEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
