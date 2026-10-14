use crate::tree::{Icon, InsertBehavior, Node, NodeData, NodeId};
use concepts::storage::http_client_trace::HttpClientTrace;
use id_tree::Tree;

pub fn attach_http_traces(
    tree: &mut Tree<NodeData<u32>>,
    root_id: &NodeId,
    traces: &[HttpClientTrace],
) {
    if traces.is_empty() {
        return;
    }

    let traces_node = tree
        .insert(
            Node::new(NodeData {
                icon: Icon::Exchange,
                label: "HTTP Traces".into(),
                has_caret: true,
                ..Default::default()
            }),
            InsertBehavior::UnderNode(root_id),
        )
        .unwrap();

    for trace in traces {
        let sent_at = trace.req.sent_at;
        let finished_at = trace.resp.as_ref().map(|resp| resp.finished_at);
        let trace_node_label = format!(
            "{} {} ({})",
            trace.req.method,
            trace.req.uri,
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
                    label: trace_node_label.into(),
                    has_caret: true,
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&traces_node),
            )
            .unwrap();

        tree.insert(
            Node::new(NodeData {
                icon: Icon::Time,
                label: format!("Sent at: {sent_at}").into(),
                ..Default::default()
            }),
            InsertBehavior::UnderNode(&trace_node),
        )
        .unwrap();
        if let Some(finished_at) = finished_at {
            tree.insert(
                Node::new(NodeData {
                    icon: Icon::Time,
                    label: format!("Finished at: {finished_at}").into(),
                    ..Default::default()
                }),
                InsertBehavior::UnderNode(&trace_node),
            )
            .unwrap();
        }

        match trace.resp.as_ref().map(|resp| &resp.status) {
            Some(Ok(status)) => {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Tag,
                        label: format!("Status: {status}").into(),
                        ..Default::default()
                    }),
                    InsertBehavior::UnderNode(&trace_node),
                )
                .unwrap();
            }
            Some(Err(error)) => {
                tree.insert(
                    Node::new(NodeData {
                        icon: Icon::Error,
                        label: format!("Error: {error}").into(),
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
