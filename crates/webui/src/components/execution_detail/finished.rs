use crate::tree::{Icon, InsertBehavior, Node, NodeData, NodeId, TreeBuilder, TreeData};
use crate::{
    components::{
        execution_detail::{http_trace::attach_http_traces, tree_component::TreeComponent},
        json_tree::{JsonValue, insert_json_into_tree},
    },
    grpc::{grpc_client, version::VersionType},
};
use id_tree::Tree;
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct FinishedEventProps {
    pub result_detail: grpc_client::SupportedFunctionResult,
    pub version: Option<VersionType>,
    pub is_selected: bool,
    #[prop_or_default]
    pub http_client_traces: Vec<grpc_client::HttpClientTrace>,
}

fn with_version(version: Option<VersionType>, label: &'static str) -> Html {
    if let Some(version) = version {
        Html::from(format!("{version}. {label}"))
    } else {
        Html::from(label)
    }
}

fn execution_failure_presentation(kind: grpc_client::ExecutionFailureKind) -> (&'static str, Icon) {
    match kind {
        grpc_client::ExecutionFailureKind::Unspecified => ("Unspecified", Icon::Error),
        grpc_client::ExecutionFailureKind::TimedOut => ("Timed out", Icon::Time),
        grpc_client::ExecutionFailureKind::NondeterminismDetected => {
            ("Nondeterminism detected", Icon::Exchange)
        }
        grpc_client::ExecutionFailureKind::OutOfFuel => ("Out of fuel", Icon::Error),
        grpc_client::ExecutionFailureKind::Cancelled => ("Cancelled", Icon::Cross),
        grpc_client::ExecutionFailureKind::Uncategorized => ("Uncategorized", Icon::Error),
        grpc_client::ExecutionFailureKind::ValueTooLarge => ("Value too large", Icon::Error),
    }
}

pub fn attach_result_detail(
    tree: &mut Tree<NodeData<u32>>,
    root_id: &NodeId,
    result_detail: &grpc_client::SupportedFunctionResult,
    version: Option<VersionType>,
    is_selected: bool,
) -> NodeId {
    match &result_detail
        .value
        .as_ref()
        .expect("`value` is sent in `ResultDetail` message")
    {
        grpc_client::supported_function_result::Value::Ok(ok) => {
            let ok_node = tree
                .insert(
                    Node::new(NodeData {
                        icon: Icon::Tick,
                        label: with_version(version, "Succeeded"),
                        has_caret: ok.return_value.is_some(),
                        is_selected,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(root_id),
                )
                .unwrap();

            if let Some(any) = &ok.return_value {
                let _ = insert_json_into_tree(tree, &ok_node, JsonValue::Serialized(&any.value));
            }
            ok_node
        }
        grpc_client::supported_function_result::Value::Error(fallible) => {
            let error_node = tree
                .insert(
                    Node::new(NodeData {
                        icon: Icon::Error,
                        label: with_version(version, "Finished with error"),
                        has_caret: fallible.return_value.is_some(),
                        is_selected,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(root_id),
                )
                .unwrap();
            if let Some(any) = &fallible.return_value {
                let _ = insert_json_into_tree(tree, &error_node, JsonValue::Serialized(&any.value));
            }
            error_node
        }

        grpc_client::supported_function_result::Value::ExecutionFailure(failure) => {
            let (failure_kind, icon) = execution_failure_presentation(failure.kind());

            let failure_node = tree
                .insert(
                    Node::new(NodeData {
                        icon,
                        label: with_version(version, "Execution Failed"),
                        has_caret: true,
                        is_selected,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(root_id),
                )
                .unwrap();

            tree.insert(
                Node::new(NodeData {
                    icon,
                    label: failure_kind.into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&failure_node),
            )
            .unwrap();

            if let Some(reason) = &failure.reason {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Error,
                        label: reason.as_str().into(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&failure_node),
                )
                .unwrap();
            }
            if let Some(detail) = &failure.detail {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Database,
                        label: html! {<> {"Detail: "} <input type="text" readonly=true value={detail.clone()} /> </>},
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&failure_node),
                )
                .unwrap();
            }
            failure_node
        }
    }
}

impl FinishedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let node_id = attach_result_detail(
            &mut tree,
            &root_id,
            &self.result_detail,
            self.version,
            self.is_selected,
        );
        attach_http_traces(&mut tree, &node_id, &self.http_client_traces);
        TreeData::from(tree)
    }
}

#[component(FinishedEvent)]
pub fn finished_event(props: &FinishedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_too_large_has_named_failure_presentation() {
        let (label, icon) =
            execution_failure_presentation(grpc_client::ExecutionFailureKind::ValueTooLarge);

        assert_eq!(label, "Value too large");
        assert_eq!(icon, Icon::Error);
    }
}
