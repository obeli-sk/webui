use crate::app::query::BacktraceVersionsPath;
use crate::components::execution_header::ExecutionLink;
use crate::grpc::grpc_client::ExecutionId;
use crate::grpc::grpc_client::join_set_response_event::{ChildExecutionFinished, DelayFinished};
use crate::grpc::version::VersionType;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    app::Route,
    components::execution_detail::{finished::attach_result_detail, tree_component::TreeComponent},
    grpc::grpc_client::{
        self, JoinSetResponseEvent, SupportedFunctionResult, join_set_response_event,
    },
};
use chrono::DateTime;
use log::error;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextEventProps {
    pub event: grpc_client::execution_event::history_event::JoinNext,
    pub response: Option<JoinSetResponseEvent>,
    pub execution_id: ExecutionId,
    pub backtrace_id: Option<VersionType>,
    pub version: VersionType,
    pub link: ExecutionLink,
    pub is_selected: bool,
}

impl HistoryJoinNextEventProps {
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
            .expect("JoinSetRequest.join_set_id is sent");

        let icon = match &self.response {
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::ChildExecutionFinished(
                        join_set_response_event::ChildExecutionFinished {
                            value:
                                Some(SupportedFunctionResult {
                                    value:
                                        Some(grpc_client::supported_function_result::Value::Ok(_)),
                                }),
                            ..
                        },
                    )),
                ..
            }) => Icon::Tick,
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::DelayFinished(DelayFinished {
                        success: true,
                        ..
                    })),
                ..
            }) => Icon::Tick,

            Some(_) => Icon::Error,

            None => Icon::Search,
        };

        let join_next_node = tree
            .insert(
                Node::new(NodeData {
                    icon,
                    label: html! {
                        <>
                            {self.version}
                            {". Join Next: `"}
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

        match &self.response {
            Some(JoinSetResponseEvent {
                created_at: Some(finished_at),
                join_set_id: _,
                response:
                    Some(join_set_response_event::Response::ChildExecutionFinished(
                        ChildExecutionFinished {
                            child_execution_id: Some(child_execution_id),
                            value: Some(result_detail),
                        },
                    )),
            }) => {
                let success = matches!(
                    result_detail.value,
                    Some(grpc_client::supported_function_result::Value::Ok(_))
                );
                let icon = if success { Icon::Flows } else { Icon::Error };

                let child_node = tree.insert(
                        Node::new(NodeData {
                            icon,
                            label: html! {
                                <>
                                    {"Matched Child "}
                                    { if success { "Finished" } else { "Failed" } }
                                    {": "}
                                    { self.link.link(child_execution_id.clone(), &child_execution_id.id) }
                                </>
                            },
                            has_caret: true,
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_next_node),
                    )
                    .unwrap();

                attach_result_detail(&mut tree, &child_node, result_detail, None, false);

                let finished_at = DateTime::from(*finished_at);
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Time,
                        label: format!("Finished At: {finished_at}").into_html(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&child_node),
                )
                .unwrap();
            }
            Some(JoinSetResponseEvent {
                created_at: Some(finished_at),
                join_set_id: _,
                response:
                    Some(join_set_response_event::Response::DelayFinished(DelayFinished {
                        delay_id: Some(delay_id),
                        success,
                    })),
            }) => {
                let success = *success;
                let icon = if success { Icon::Time } else { Icon::Error };
                let delay_node = tree
                    .insert(
                        Node::new(NodeData {
                            icon,
                            label: html! {
                                <>
                                    {"Matched Delay "}
                                    { if success { "Finished" } else {"Cancelled"} }
                                    {": "}
                                    {&delay_id.id}
                                </>
                            },
                            has_caret: true,
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_next_node),
                    )
                    .unwrap();

                let finished_at = DateTime::from(*finished_at);

                tree.insert(
                    Node::new(NodeData {
                        icon,
                        label: if success {
                            format!("Finished At: {finished_at}")
                        } else {
                            format!("Cancelled At: {finished_at}")
                        }
                        .into_html(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&delay_node),
                )
                .unwrap();
            }
            None => {}
            other => {
                error!("Unknown format {other:?}");
            }
        }

        // Add closing status
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Lock,
                label: format!("Closing: {}", self.event.closing).into_html(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&join_next_node),
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
                InsertBehavior::UnderNode(&join_next_node),
            )
            .unwrap();
        }
        TreeData::from(tree)
    }
}

#[function_component(HistoryJoinNextEvent)]
pub fn history_join_next_event(props: &HistoryJoinNextEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
