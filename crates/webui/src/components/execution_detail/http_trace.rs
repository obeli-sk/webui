use crate::grpc::grpc_client;
use crate::tree::{Icon, InsertBehavior, Node, NodeData, NodeId};
use chrono::DateTime;
use id_tree::Tree;
use yew::prelude::*;

pub fn attach_http_traces(
    tree: &mut Tree<NodeData<u32>>,
    root_id: &NodeId,
    traces: &[grpc_client::HttpClientTrace],
) {
    if traces.is_empty() {
        return;
    }

    let traces_node = tree
        .insert(
            Node::new(NodeData {
                icon: Icon::Exchange,
                label: "HTTP Traces".into_html(),
                has_caret: true,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(root_id),
        )
        .unwrap();

    for trace in traces {
        let sent_at = DateTime::from(
            trace
                .sent_at
                .expect("HttpClientTrace.sent_at is always sent"),
        );
        let finished_at = trace
            .finished_at
            .as_ref()
            .map(|finished_at| DateTime::from(*finished_at));
        let trace_node_label = format!(
            "{} {} ({})",
            trace.method,
            trace.uri,
            if let Some(finished_at) = finished_at {
                let duration = (finished_at - sent_at)
                    .to_std()
                    .expect("duration should never be negative");
                format!("{duration:?}")
            } else {
                "No response".to_string()
            }
        );

        let trace_node = tree
            .insert(
                Node::new(NodeData {
                    icon: Icon::Exchange,
                    label: trace_node_label.into_html(),
                    has_caret: true,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&traces_node),
            )
            .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Time,
                label: format!("Sent at: {sent_at}").into_html(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&trace_node),
        )
        .unwrap();
        if let Some(finished_at) = finished_at {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Time,
                    label: format!("Finished at: {finished_at}").into_html(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&trace_node),
            )
            .unwrap();
        }

        match &trace.result {
            Some(grpc_client::http_client_trace::Result::Status(status)) => {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Tag,
                        label: format!("Status: {status}").into_html(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&trace_node),
                )
                .unwrap();
            }
            Some(grpc_client::http_client_trace::Result::Error(error)) => {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Error,
                        label: format!("Error: {error}").into_html(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&trace_node),
                )
                .unwrap();
            }
            None => {}
        }
    }
}
