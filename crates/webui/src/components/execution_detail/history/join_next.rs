use crate::components::execution_detail::{
    finished::attach_result_detail, tree_component::TreeComponent, utils::id_suffix,
};
use crate::components::execution_header::ExecutionLink;
use crate::components::ffqn_with_links::FfqnWithLinks;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::{
    ExecutionFailureKind, ExecutionId, FunctionFqn, JoinSetId, SupportedFunctionReturnValue,
    storage::{JoinSetResponse, JoinSetResponseEventOuter, VersionType},
};
use rest_types::executions::RetVal;
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinNextEventProps {
    pub join_set_id: JoinSetId,
    pub requested_ffqn: Option<FunctionFqn>,
    pub closing: bool,
    pub response: Option<JoinSetResponseEventOuter>,
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
        let join_set_id = &self.join_set_id;
        let response = self.response.as_ref().map(|outer| &outer.event.event);

        let icon = match response {
            Some(JoinSetResponse::ChildExecutionFinished {
                result: SupportedFunctionReturnValue::Ok(_),
                ..
            })
            | Some(JoinSetResponse::DelayFinished { result: Ok(()), .. }) => Icon::Tick,
            // Cancelled delay
            Some(JoinSetResponse::DelayFinished {
                result: Err(()), ..
            }) => Icon::Cross,
            // Cancelled child execution
            Some(JoinSetResponse::ChildExecutionFinished {
                result: SupportedFunctionReturnValue::ExecutionFailure(failure),
                ..
            }) if failure.kind == ExecutionFailureKind::Cancelled => Icon::Cross,
            Some(_) => Icon::Error,
            None => Icon::Search,
        };

        let id_suffix = match response {
            Some(JoinSetResponse::ChildExecutionFinished {
                child_execution_id, ..
            }) => Some(id_suffix(child_execution_id)),
            Some(JoinSetResponse::DelayFinished { delay_id, .. }) => Some(id_suffix(delay_id)),
            None => None,
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

        if let Some(JoinSetResponseEventOuter {
            created_at: finished_at,
            event: response,
        }) = &self.response
        {
            match &response.event {
                JoinSetResponse::ChildExecutionFinished {
                    child_execution_id,
                    finished_version: _,
                    result,
                } => {
                    let child_execution_id = ExecutionId::Derived(child_execution_id.clone());
                    tree.insert(
                    Node::new(NodeData {
                        icon: Icon::IdNumber,
                        label: html! {
                            { self.link.link(child_execution_id.clone(), &child_execution_id.to_string()) }
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_next_node),
                )
                .unwrap();

                    attach_result_detail(
                        &mut tree,
                        &join_next_node,
                        &RetVal::from(result.clone()),
                        None,
                        false,
                    );

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
                JoinSetResponse::DelayFinished { delay_id, result } => {
                    let success = result.is_ok();
                    // A delay that did not succeed was cancelled.
                    let icon = if success { Icon::Time } else { Icon::Cross };
                    let delay_node = tree
                        .insert(
                            Node::new(NodeData {
                                icon: Icon::IdNumber,
                                label: html! { {delay_id.to_string()} },
                                has_caret: true,
                                ..Default::default()
                            }),
                            InsertBehavior::UnderNode(&join_next_node),
                        )
                        .unwrap();
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
            }
        }

        // Function name
        if let Some(ffqn) = &self.requested_ffqn {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Function,
                    label: html! {
                        <FfqnWithLinks ffqn={ffqn.clone()} fully_qualified={true} />
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
                label: format!("Closing: {}", self.closing).into(),
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
