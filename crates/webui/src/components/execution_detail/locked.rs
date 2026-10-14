use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent, model::ComponentTypeExt as _,
};
use concepts::storage::{Locked, VersionType};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct LockedEventProps {
    pub locked: Locked,
    pub version: VersionType,
    pub is_selected: bool,
}

impl LockedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let locked = &self.locked;
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();
        let event_type = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Lock,
                    label: Html::from(format!("{}. Locked", self.version)),
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        // Expires at
        let expires_at = locked.lock_expires_at;
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Time,
                label: html! { {format!("Expires At: {}", expires_at)} },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        // Run ID
        tree.insert(
            Node::new(NodeData {
                icon: Icon::IdNumber,
                label: html! { {format!("Run ID: {}", locked.run_id)} },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        // component id
        {
            let component_id = &locked.component_id;
            let component_id_node = tree
                .insert(
                    Node::new(NodeData {
                        icon: component_id.component_type.as_icon(),
                        label: html! {
                            { format!("Component: {}",  component_id.name) }
                        },
                        has_caret: true,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&event_type),
                )
                .unwrap();
            tree.insert(
                Node::new(NodeData {
                    icon: component_id.component_type.as_icon(),
                    label: html! {<>
                        {"Component Digest: "}
                        <input type="text" value={ component_id.component_digest.to_string() } />
                    </>},
                    has_caret: false,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&component_id_node),
            )
            .unwrap();
            tree.insert(
                Node::new(NodeData {
                    icon: component_id.component_type.as_icon(),
                    label: html! {
                        { format!("Component Type: {}",  component_id.component_type) }
                    },
                    has_caret: false,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&component_id_node),
            )
            .unwrap();
        }

        // deployment id
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Antenna,
                label: html! {
                    { format!("Deployment ID: {}", locked.deployment_id) }
                },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        // executor id
        {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::IdNumber,
                    label: html! { {format!("Executor ID: {}", locked.executor_id)} },
                    has_caret: false,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&event_type),
            )
            .unwrap();
        }

        // retry config
        {
            let retry_config = &locked.retry_config;
            let retry_node = tree
                .insert(
                    Node::new(NodeData {
                        icon: Icon::History,
                        label: "Retry Config".into(),
                        has_caret: true,
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&event_type),
                )
                .unwrap();
            if let Some(max_retries) = retry_config.max_retries {
                tree.insert(
                    Node::new(NodeData {
                        label: format!("Max Retries: {max_retries}").into(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&retry_node),
                )
                .unwrap();
            }
            {
                tree.insert(
                    Node::new(NodeData {
                        label: format!("Exp Backoff: {:?}", retry_config.retry_exp_backoff).into(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&retry_node),
                )
                .unwrap();
            }
        }

        TreeData::from(tree)
    }
}

#[component(LockedEvent)]
pub fn locked_event(props: &LockedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
