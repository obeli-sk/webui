use crate::components::execution_detail::tree_component::TreeComponent;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::{
    component_id::ComponentDigest,
    prefixed_ulid::DeploymentId,
    storage::{ComponentUpgradeOutcome, ComponentUpgradeReason, VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct ComponentUpgradeFinishedEventProps {
    pub component_digest: ComponentDigest,
    pub deployment_id: DeploymentId,
    pub outcome: ComponentUpgradeOutcome,
    pub version: VersionType,
    pub is_selected: bool,
}

impl ComponentUpgradeFinishedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let (header_label, outcome_label) = match &self.outcome {
            ComponentUpgradeOutcome::Success { reason } => {
                let reason_label = match reason {
                    ComponentUpgradeReason::Auto => "Reason: Auto".to_string(),
                    ComponentUpgradeReason::Manual { force } => {
                        format!("Reason: Manual (force: {force})")
                    }
                };
                (
                    format!("{}. Component Upgrade Succeeded", self.version),
                    reason_label,
                )
            }
            ComponentUpgradeOutcome::Failed { reason } => (
                format!("{}. Component Upgrade Failed", self.version),
                format!("Reason: {reason}"),
            ),
        };

        let event_type = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Exchange,
                    label: Html::from(header_label),
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Cog,
                label: outcome_label.into(),
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Tag,
                label: html! {<>
                    {"Component Digest: "}
                    <input type="text" value={ self.component_digest.to_string() } />
                </>},
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Antenna,
                label: html! { { format!("Deployment ID: {}", self.deployment_id) } },
                has_caret: false,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        TreeData::from(tree)
    }
}

#[component(ComponentUpgradeFinishedEvent)]
pub fn component_upgrade_finished_event(props: &ComponentUpgradeFinishedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
