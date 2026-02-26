use crate::app::AppState;
use crate::app::{Route, query::BacktraceVersionsPath};
use crate::components::execution_actions::CancelDelayButton;
use crate::components::execution_detail::tree_component::TreeComponent;
use crate::components::execution_header::ExecutionLink;
use crate::components::ffqn_with_links::FfqnWithLinks;
use crate::components::json_tree::{JsonValue, insert_json_into_tree};
use crate::grpc::ffqn::FunctionFqn;
use crate::grpc::grpc_client::{
    self, ExecutionId, execution_event::history_event::join_set_request,
};
use crate::grpc::version::VersionType;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use chrono::DateTime;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinSetRequestEventProps {
    pub event: grpc_client::execution_event::history_event::JoinSetRequest,
    pub execution_id: ExecutionId,
    pub backtrace_id: Option<VersionType>,
    pub version: VersionType,
    pub link: ExecutionLink,
    pub is_selected: bool,
    /// Optional Created event of the child execution.
    /// When provided, the component displays the child's function name and parameters.
    #[prop_or_default]
    pub child_created: Option<grpc_client::execution_event::Created>,
}

impl HistoryJoinSetRequestEventProps {
    fn construct_tree(&self, app_state: &AppState) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let join_set_id = self
            .event
            .join_set_id
            .as_ref()
            .expect("JoinSetRequest.join_set_id is sent");
        let join_set_node = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::History,
                    label: html! {
                        <>
                            {self.version}
                            {". Join Set Request: `"}
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

        match self
            .event
            .join_set_request
            .as_ref()
            .expect("`join_set_request` is sent in `JoinSetRequest`")
        {
            join_set_request::JoinSetRequest::DelayRequest(delay_req) => {
                let (Some(delay_id), Some(expires_at)) =
                    (&delay_req.delay_id, &delay_req.expires_at)
                else {
                    panic!("`delay_id` and `expires_at` are sent in `DelayRequest` message");
                };
                let expires_at = DateTime::from(*expires_at);
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Time,
                        label: html! {
                            <>
                                {"Delay Request: "}
                                {&delay_id.id}
                            </>
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Time,
                        label: html! {
                            <>
                                {"Expires At: "}
                                {expires_at}
                            </>
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Cross,
                        label: html! {
                            <CancelDelayButton delay_id={delay_id.clone()} />
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();
            }
            join_set_request::JoinSetRequest::ChildExecutionRequest(child_req) => {
                let child_execution_id = child_req
                    .child_execution_id
                    .as_ref()
                    .expect("`child_execution_id` is sent in `ChildExecutionRequest`");

                // Extract function name and params from child's Created event if available
                let child_info = self.child_created.as_ref().map(|created| {
                    let function_name = created
                        .function_name
                        .as_ref()
                        .expect("`function_name` is sent in Created");
                    let ffqn = FunctionFqn::from(function_name.clone());
                    let raw_params: Vec<serde_json::Value> = serde_json::from_slice(
                        &created
                            .params
                            .as_ref()
                            .expect("`params` is sent in Created")
                            .value,
                    )
                    .expect("`params` must be a JSON array");
                    let params: Vec<(String, serde_json::Value)> =
                        match app_state.ffqns_to_details.get(&ffqn) {
                            Some((function_detail, _))
                                if function_detail.params.len() == raw_params.len() =>
                            {
                                function_detail
                                    .params
                                    .iter()
                                    .zip(raw_params.iter())
                                    .map(|(fn_param, param_value)| {
                                        (fn_param.name.clone(), param_value.clone())
                                    })
                                    .collect()
                            }
                            _ => raw_params
                                .iter()
                                .map(|v| ("(unknown)".to_string(), v.clone()))
                                .collect(),
                        };
                    (ffqn, params)
                });

                let child_node = tree
                    .insert(
                        Node::new(NodeData {
                            icon: if child_req.success {
                                Icon::Flows
                            } else {
                                Icon::Error
                            },
                            label: html! {
                                <>
                                    {"Child Execution Request: "}
                                    { self.link.link(child_execution_id.clone(), &child_execution_id.id) }
                                </>
                            },
                            has_caret: child_info.is_some(),
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();

                if let Some((ffqn, params)) = &child_info {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::Function,
                            label: html! {
                                <FfqnWithLinks ffqn={ffqn.clone()} />
                            },
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&child_node),
                    )
                    .unwrap();
                    let params_node_id = tree
                        .insert(
                            Node::new(NodeData {
                                icon: Icon::FolderClose,
                                label: "Parameters".into_html(),
                                has_caret: true,
                                ..Default::default()
                            }),
                            InsertBehavior::UnderNode(&child_node),
                        )
                        .unwrap();
                    for (param_name, param_value) in params {
                        let param_name_node = tree
                            .insert(
                                Node::new(NodeData {
                                    icon: Icon::Function,
                                    label: format!("{param_name}: {param_value}").into_html(),
                                    has_caret: true,
                                    ..Default::default()
                                }),
                                InsertBehavior::UnderNode(&params_node_id),
                            )
                            .unwrap();
                        let _ = insert_json_into_tree(
                            &mut tree,
                            &param_name_node,
                            JsonValue::Parsed(param_value),
                        );
                    }
                }
            }
        }
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
                InsertBehavior::UnderNode(&join_set_node),
            )
            .unwrap();
        }
        TreeData::from(tree)
    }
}

#[function_component(HistoryJoinSetRequestEvent)]
pub fn history_join_set_request_event(props: &HistoryJoinSetRequestEventProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let tree = props.construct_tree(&app_state);
    html! {
        <TreeComponent {tree} />
    }
}
