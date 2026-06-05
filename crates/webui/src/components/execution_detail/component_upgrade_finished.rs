use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent,
    grpc::{
        grpc_client::{self, execution_event::component_upgrade_finished},
        version::VersionType,
    },
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct ComponentUpgradeFinishedEventProps {
    pub event: grpc_client::execution_event::ComponentUpgradeFinished,
    pub version: VersionType,
    pub is_selected: bool,
}

impl ComponentUpgradeFinishedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let event = &self.event;
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();

        let (header_label, outcome_label) = match &event.outcome {
            Some(component_upgrade_finished::Outcome::Success(success)) => {
                let reason_label = match &success.reason {
                    Some(component_upgrade_finished::success::Reason::Auto(_)) => {
                        "Reason: Auto".to_string()
                    }
                    Some(component_upgrade_finished::success::Reason::Manual(m)) => {
                        format!("Reason: Manual (force: {})", m.force)
                    }
                    None => "Reason: (?)".to_string(),
                };
                (
                    format!("{}. Component Upgrade Succeeded", self.version),
                    reason_label,
                )
            }
            Some(component_upgrade_finished::Outcome::Failed(failed)) => (
                format!("{}. Component Upgrade Failed", self.version),
                format!("Reason: {}", failed.reason),
            ),
            None => (
                format!("{}. Component Upgrade Finished", self.version),
                "Outcome: (?)".to_string(),
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

        if let Some(digest) = &event.component_digest {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Tag,
                    label: html! {<>
                        {"Component Digest: "}
                        <input type="text" value={ digest.digest.clone() } />
                    </>},
                    has_caret: false,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&event_type),
            )
            .unwrap();
        }

        if let Some(deployment_id) = &event.deployment_id {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Antenna,
                    label: html! { { format!("Deployment ID: {}", deployment_id.id) } },
                    has_caret: false,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&event_type),
            )
            .unwrap();
        }

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
