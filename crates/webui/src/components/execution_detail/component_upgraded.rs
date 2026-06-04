use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use crate::{
    components::execution_detail::tree_component::TreeComponent,
    grpc::{
        grpc_client::{self, execution_event::component_upgraded},
        version::VersionType,
    },
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct ComponentUpgradedEventProps {
    pub event: grpc_client::execution_event::ComponentUpgraded,
    pub version: VersionType,
    pub is_selected: bool,
}

impl ComponentUpgradedEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let event = &self.event;
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();
        let event_type = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Exchange,
                    label: Html::from(format!("{}. Component Upgraded", self.version)),
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        let reason_label = match &event.reason {
            Some(component_upgraded::Reason::Auto(_)) => "Reason: Auto".to_string(),
            Some(component_upgraded::Reason::Manual(m)) => {
                format!("Reason: Manual (force: {})", m.force)
            }
            None => "Reason: (?)".to_string(),
        };
        tree.insert(
            Node::new(NodeData {
                icon: Icon::Cog,
                label: reason_label.into(),
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

#[component(ComponentUpgradedEvent)]
pub fn component_upgraded_event(props: &ComponentUpgradedEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
