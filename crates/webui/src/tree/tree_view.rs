//! Tree view component that renders the tree structure.

use super::{NodeData, TreeData};
use id_tree::NodeId;
use yew::prelude::*;

/// Properties for the Tree component.
#[derive(Properties, PartialEq)]
pub struct TreeProps<T: Clone + PartialEq + 'static> {
    /// The tree data to render.
    pub tree: TreeData<T>,
    /// Callback when a node is clicked.
    #[prop_or_default]
    pub onclick: Option<Callback<(NodeId, MouseEvent)>>,
    /// Callback when a node is expanded.
    #[prop_or_default]
    pub on_expand: Option<Callback<(NodeId, MouseEvent)>>,
    /// Callback when a node is collapsed.
    #[prop_or_default]
    pub on_collapse: Option<Callback<(NodeId, MouseEvent)>>,
}

/// A tree view component.
#[function_component(Tree)]
pub fn tree<T: Clone + PartialEq + 'static>(props: &TreeProps<T>) -> Html {
    let tree = props.tree.borrow();

    // Get the root node ID
    let Some(root_id) = tree.root_node_id() else {
        return html! { <div class="tree-empty">{"Empty tree"}</div> };
    };

    // Render children of root (root itself is usually invisible)
    let children = tree.children_ids(root_id).expect("root should exist");

    html! {
        <ul class="tree-root">
            { for children.map(|child_id| render_node(&tree, child_id, props)) }
        </ul>
    }
}

fn render_node<T: Clone + PartialEq + 'static>(
    tree: &id_tree::Tree<NodeData<T>>,
    node_id: &NodeId,
    props: &TreeProps<T>,
) -> Html {
    let node = tree.get(node_id).expect("node should exist");
    let data = node.data();

    let has_children = tree
        .children_ids(node_id)
        .map(|mut c| c.next().is_some())
        .unwrap_or(false);
    let show_caret = data.has_caret || has_children;

    let node_id_clone = node_id.clone();
    let onclick = props.onclick.clone();
    let on_expand = props.on_expand.clone();
    let on_collapse = props.on_collapse.clone();
    let is_expanded = data.is_expanded;

    let handle_click = Callback::from(move |e: MouseEvent| {
        if let Some(ref cb) = onclick {
            cb.emit((node_id_clone.clone(), e.clone()));
        }
        if is_expanded {
            if let Some(ref cb) = on_collapse {
                cb.emit((node_id_clone.clone(), e));
            }
        } else if let Some(ref cb) = on_expand {
            cb.emit((node_id_clone.clone(), e));
        }
    });

    let mut node_classes = classes!("tree-node");
    if data.is_selected {
        node_classes.push("tree-node-selected");
    }
    if data.is_expanded {
        node_classes.push("tree-node-expanded");
    }

    let caret_class = if data.is_expanded {
        "tree-caret tree-caret-open"
    } else {
        "tree-caret tree-caret-closed"
    };

    html! {
        <li class={node_classes}>
            <div class="tree-node-content" onclick={handle_click}>
                if show_caret {
                    <span class={caret_class}>
                        { if data.is_expanded { "▼" } else { "▶" } }
                    </span>
                } else {
                    <span class="tree-caret tree-caret-none">{"\u{00a0}\u{00a0}"}</span>
                }
                <span class="tree-icon">{ data.icon.as_char() }</span>
                <span class="tree-label">{ data.label.clone() }</span>
            </div>
            if data.is_expanded && has_children {
                <ul class="tree-children">
                    { for tree.children_ids(node_id).expect("node exists").map(|child_id| {
                        render_node(tree, child_id, props)
                    })}
                </ul>
            }
        </li>
    }
}
