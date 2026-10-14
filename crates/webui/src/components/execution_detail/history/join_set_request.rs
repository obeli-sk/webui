use crate::app::AppState;
use crate::components::execution_actions::{
    CancelDelayButton, PauseDelayButton, UnpauseDelayButton,
};
use crate::components::execution_detail::tree_component::TreeComponent;
use crate::components::execution_detail::utils::id_suffix;
use crate::components::execution_header::ExecutionLink;
use crate::components::ffqn_with_links::FfqnWithLinks;
use crate::components::json_tree::{JsonValue, insert_json_into_tree};
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::util::time::{TimeGranularity, format_date, relative_time, use_relative_now};
use chrono::{DateTime, Utc};
use concepts::{
    ExecutionId,
    storage::{Created, HistoryEventScheduleAt, JoinSetRequest, PersistedParams, VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryJoinSetRequestEventProps {
    pub request: JoinSetRequest,
    pub version: VersionType,
    pub link: ExecutionLink,
    pub is_selected: bool,
    /// Optional Created event of the child execution.
    /// When provided, the component displays the child's function name and parameters.
    #[prop_or_default]
    pub child_created: Option<Created>,
}

impl HistoryJoinSetRequestEventProps {
    fn construct_tree(&self, app_state: &AppState) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        // For child executions, extract ffqn + params from the child's Created event
        // (when it was fetched) so we can label with the function name and flatten the
        // details directly under this node.
        let child_info = match &self.request {
            JoinSetRequest::ChildExecutionRequest { .. } => {
                self.child_created.as_ref().map(|created| {
                    let ffqn = created.ffqn.clone();
                    let raw_params = created.params.as_json_values().unwrap_or_default();
                    let params: Vec<(String, serde_json::Value)> =
                        match app_state.ffqns_to_details.get(&ffqn) {
                            Some((function_detail, _))
                                if function_detail.parameter_types.len() == raw_params.len() =>
                            {
                                function_detail
                                    .parameter_types
                                    .iter()
                                    .zip(raw_params.iter())
                                    .map(|(fn_param, param_value)| {
                                        (fn_param.name.to_string(), param_value.clone())
                                    })
                                    .collect()
                            }
                            _ => raw_params
                                .iter()
                                .map(|v| ("(unknown)".to_string(), v.clone()))
                                .collect(),
                        };
                    (ffqn, params)
                })
            }
            JoinSetRequest::DelayRequest { .. } => None,
        };

        let (join_set_icon, id_suffix) = match &self.request {
            JoinSetRequest::DelayRequest { delay_id, .. } => (Icon::Time, id_suffix(delay_id)),
            JoinSetRequest::ChildExecutionRequest {
                child_execution_id, ..
            } => (Icon::Function, id_suffix(child_execution_id)),
        };
        let join_set_node = tree
            .insert(
                Node::new(NodeData {
                    icon: join_set_icon,
                    label: html! {
                        <>
                            {self.version}
                            {format!(". Submit `{id_suffix}`")}
                        </>
                    },
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        match &self.request {
            JoinSetRequest::DelayRequest {
                delay_id,
                expires_at,
                schedule_at,
                paused,
            } => {
                let expires_at = *expires_at;
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::IdNumber,
                        label: html! { {delay_id.to_string()} },
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
                // Paused flag
                if *paused {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::Lock,
                            label: "Paused".into(),
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();
                }
                // Scheduled at variant
                let label: Html = match schedule_at {
                    HistoryEventScheduleAt::Now => "Scheduled At: Now".into(),
                    HistoryEventScheduleAt::At(at) => format!("Scheduled At: {at}").into(),
                    HistoryEventScheduleAt::In(_) => {
                        html! { <RelativeScheduledIn expires_at={expires_at} /> }
                    }
                };
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Time,
                        label,
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
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Lock,
                        label: html! {
                            <PauseDelayButton delay_id={delay_id.clone()} />
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Unlock,
                        label: html! {
                            <UnpauseDelayButton delay_id={delay_id.clone()} />
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();
            }
            JoinSetRequest::ChildExecutionRequest {
                child_execution_id,
                target_ffqn: _,
                params_hash,
                params,
                result,
            } => {
                let child_execution_id = ExecutionId::Derived(child_execution_id.clone());

                // Execution id link
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::IdNumber,
                        label: html! {
                            { self.link.link(child_execution_id.clone(), &child_execution_id.to_string()) }
                        },
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&join_set_node),
                )
                .unwrap();

                // Error detail
                if let Err(err) = result {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::Error,
                            label: format!("Error: {err}").into(),
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();
                }

                if let Some(params_hash) = params_hash {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::IdNumber,
                            label: format!("Parameters Hash: {params_hash}").into(),
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();
                }

                if let PersistedParams::Rejected { rejected } = params {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::Error,
                            label: format!(
                                "Encoded Parameters Size: at least {} bytes",
                                rejected.encoded_size_at_least
                            )
                            .into(),
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();
                }

                if let Some((ffqn, params)) = &child_info {
                    tree.insert(
                        Node::new(NodeData {
                            icon: Icon::Function,
                            label: html! {
                                <FfqnWithLinks ffqn={ffqn.clone()} fully_qualified={true} />
                            },
                            ..Default::default()
                        }),
                        InsertBehavior::UnderNode(&join_set_node),
                    )
                    .unwrap();
                    let params_node_id = tree
                        .insert(
                            Node::new(NodeData {
                                icon: Icon::FolderClose,
                                label: "Parameters".into(),
                                has_caret: true,
                                ..Default::default()
                            }),
                            InsertBehavior::UnderNode(&join_set_node),
                        )
                        .unwrap();
                    for (param_name, param_value) in params {
                        let param_name_node = tree
                            .insert(
                                Node::new(NodeData {
                                    icon: Icon::Function,
                                    label: format!("{param_name}: {param_value}").into(),
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
        TreeData::from(tree)
    }
}

#[derive(Properties, PartialEq)]
struct RelativeScheduledInProps {
    expires_at: DateTime<Utc>,
}

#[function_component(RelativeScheduledIn)]
fn relative_scheduled_in(props: &RelativeScheduledInProps) -> Html {
    let now = use_relative_now(props.expires_at);
    let label = if props.expires_at > now {
        format!(
            "Scheduled In: {}",
            relative_time(now, props.expires_at, TimeGranularity::Coarse)
        )
    } else {
        format!(
            "Scheduled: {} ago",
            relative_time(props.expires_at, now, TimeGranularity::Coarse)
        )
    };
    html! { <span title={format_date(props.expires_at)}>{label}</span> }
}

#[component(HistoryJoinSetRequestEvent)]
pub fn history_join_set_request_event(props: &HistoryJoinSetRequestEventProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let tree = props.construct_tree(&app_state);
    html! {
        <TreeComponent {tree} />
    }
}
