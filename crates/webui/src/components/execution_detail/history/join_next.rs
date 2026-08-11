use crate::components::execution_header::ExecutionLink;
use crate::components::ffqn_with_links::FfqnWithLinks;
use crate::grpc::grpc_client::join_set_response_event::{ChildExecutionFinished, DelayFinished};
use crate::grpc::version::VersionType;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::{
        finished::attach_result_detail, tree_component::TreeComponent, utils::id_suffix,
    },
    grpc::grpc_client::{
        self, JoinSetResponseEvent, SupportedFunctionResult, join_set_response_event,
    },
};
use chrono::DateTime;
use log::error;
use yew::prelude::*;

/// A matched child execution whose result is an `ExecutionFailure` of kind `Cancelled`.
fn child_failure_is_cancelled(result: &SupportedFunctionResult) -> bool {
    matches!(
        &result.value,
        Some(grpc_client::supported_function_result::Value::ExecutionFailure(failure))
            if failure.kind() == grpc_client::ExecutionFailureKind::Cancelled
    )
}

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextEventProps {
    pub event: grpc_client::execution_event::history_event::JoinNext,
    pub response: Option<JoinSetResponseEvent>,
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
                                    ..
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

            // Cancelled delay
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::DelayFinished(DelayFinished {
                        success: false,
                        ..
                    })),
                ..
            }) => Icon::Cross,

            // Cancelled child execution
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::ChildExecutionFinished(
                        join_set_response_event::ChildExecutionFinished {
                            value: Some(result_detail),
                            ..
                        },
                    )),
                ..
            }) if child_failure_is_cancelled(result_detail) => Icon::Cross,

            Some(_) => Icon::Error,

            None => Icon::Search,
        };

        let id_suffix = match &self.response {
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::ChildExecutionFinished(
                        ChildExecutionFinished {
                            child_execution_id: Some(child_execution_id),
                            ..
                        },
                    )),
                ..
            }) => Some(id_suffix(&child_execution_id.id)),
            Some(JoinSetResponseEvent {
                response:
                    Some(join_set_response_event::Response::DelayFinished(DelayFinished {
                        delay_id: Some(delay_id),
                        ..
                    })),
                ..
            }) => Some(id_suffix(&delay_id.id)),
            _ => None,
        };

        let join_next_node = tree
            .insert(
                Node::new(NodeData {
                    icon,
                    label: html! {
                        <>
                            {self.version}
                            { match &id_suffix {
                                Some(id_suffix) => format!(". Join `{id_suffix}`"),
                                None => format!(". Join `{join_set_id}`"),
                            } }
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
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::IdNumber,
                        label: html! {
                            { self.link.link(child_execution_id.clone(), &child_execution_id.id) }
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_next_node),
                )
                .unwrap();

                attach_result_detail(&mut tree, &join_next_node, result_detail, None, false);

                let finished_at = DateTime::from(*finished_at);
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Time,
                        label: format!("Finished At: {finished_at}").into(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_next_node),
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
                // A delay that did not succeed was cancelled.
                let icon = if success { Icon::Time } else { Icon::Cross };
                let delay_node = tree
                    .insert(
                        Node::new(NodeData {
                            icon: Icon::IdNumber,
                            label: html! { {&delay_id.id} },
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
                        .into(),
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

        // Function name
        if let Some(function) = &self.event.function {
            let ffqn = crate::grpc::ffqn::FunctionFqn::from(function.clone());
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Function,
                    label: html! {
                        <FfqnWithLinks ffqn={ffqn} fully_qualified={true} />
                    },
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&join_next_node),
            )
            .unwrap();
        }

        // Add closing status
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Lock,
                label: format!("Closing: {}", self.event.closing).into(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&join_next_node),
        )
        .unwrap();
        TreeData::from(tree)
    }
}

#[component(HistoryJoinNextEvent)]
pub fn history_join_next_event(props: &HistoryJoinNextEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
