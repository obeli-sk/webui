use crate::components::{
    execution_detail::tree_component::TreeComponent, execution_header::ExecutionLink,
};
use crate::tree::{Icon, InsertBehavior, Node, NodeData, TreeBuilder, TreeData};
use concepts::{
    ExecutionId,
    component_id::Digest,
    storage::{HistoryEventScheduleAt, ScheduleRequestError, VersionType},
};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct HistoryScheduleEventProps {
    pub execution_id: ExecutionId,
    pub schedule_at: HistoryEventScheduleAt,
    pub params_hash: Option<Digest>,
    pub result: Result<(), ScheduleRequestError>,
    pub version: VersionType,
    pub link: ExecutionLink,
    pub is_selected: bool,
}

impl HistoryScheduleEventProps {
    fn construct_tree(&self) -> TreeData<u32> {
        let mut tree = TreeBuilder::new().build();
        let root_id = tree
            .insert(Node::new(NodeData::default()), InsertBehavior::AsRoot)
            .unwrap();
        let scheduled_execution_id = &self.execution_id;
        let event_type = tree
            .insert(
                Node::new(NodeData {
                    icon: if self.result.is_ok() {
                        Icon::History
                    } else {
                        Icon::Error
                    },
                    label: html!{<>
                        { self.version }
                        {". Scheduled execution "}
                        { self.link.link(scheduled_execution_id.clone(), &scheduled_execution_id.to_string()) }
                    </>},
                    has_caret: true,
                    is_selected: self.is_selected,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&root_id),
            )
            .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Time,
                label: format!("At: {}", self.schedule_at).into(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&event_type),
        )
        .unwrap();

        if let Some(params_hash) = &self.params_hash {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::IdNumber,
                    label: format!("Parameters Hash: {params_hash}").into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&event_type),
            )
            .unwrap();
        }

        // Error detail
        if let Err(err) = &self.result {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Error,
                    label: format!("Error: {err}").into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&event_type),
            )
            .unwrap();
        }

        TreeData::from(tree)
    }
}

#[component(HistoryScheduleEvent)]
pub fn history_schedule_event(props: &HistoryScheduleEventProps) -> Html {
    let tree = props.construct_tree();
    html! {
        <TreeComponent {tree} />
    }
}
