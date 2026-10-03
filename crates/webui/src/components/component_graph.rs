use crate::{
    app::Route,
    components::{
        component_detail::{
            calls_dynamically, component_connections, component_link, component_name,
        },
        deployment_detail_page::DeploymentQuery,
    },
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{self, DeploymentId},
    },
};
use hashbrown::HashMap;
use log::error;
use std::rc::Rc;
use yew::prelude::*;
use yew_router::hooks::use_navigator;

const NODE_HEIGHT: f64 = 34.0;
const ROW_GAP: f64 = 14.0;
const LAYER_GAP: f64 = 96.0;
const MARGIN: f64 = 16.0;
// Approximate advance of the 13px monospace node label, plus room for the icon and padding.
const CHAR_WIDTH: f64 = 7.9;
const NODE_CHROME: f64 = 44.0;
const BACK_EDGE_BULGE: f64 = 40.0;

/// A caller importing functions exported by its dependency.
#[derive(Debug, PartialEq)]
struct GraphEdge {
    from: usize,
    to: usize,
    functions: Vec<FunctionFqn>,
}

#[derive(Debug, PartialEq)]
struct GraphLayout {
    /// Layer of each node, callers before their dependencies.
    layers: Vec<usize>,
    /// Nodes of each layer, top to bottom.
    rows: Vec<Vec<usize>>,
    /// Edges closing a cycle, drawn against the layer direction.
    back_edges: Vec<bool>,
}

fn layout_graph(node_count: usize, edges: &[(usize, usize)]) -> GraphLayout {
    // Depth-first search in node order: edges reaching a node still on the stack close a
    // cycle and are left out of layering; the reversed post-order is a topological order.
    let mut state = vec![0u8; node_count]; // 0 = new, 1 = on stack, 2 = done
    let mut back_edges = vec![false; edges.len()];
    let mut post_order = Vec::with_capacity(node_count);
    fn visit(
        node: usize,
        edges: &[(usize, usize)],
        state: &mut [u8],
        back_edges: &mut [bool],
        post_order: &mut Vec<usize>,
    ) {
        state[node] = 1;
        for (idx, &(from, to)) in edges.iter().enumerate() {
            if from == node {
                match state[to] {
                    0 => visit(to, edges, state, back_edges, post_order),
                    1 => back_edges[idx] = true,
                    _ => {}
                }
            }
        }
        state[node] = 2;
        post_order.push(node);
    }
    for node in 0..node_count {
        if state[node] == 0 {
            visit(node, edges, &mut state, &mut back_edges, &mut post_order);
        }
    }

    // Longest path layering over the acyclic edges.
    let mut layers = vec![0usize; node_count];
    for &node in post_order.iter().rev() {
        for (idx, &(from, to)) in edges.iter().enumerate() {
            if from == node && !back_edges[idx] {
                layers[to] = layers[to].max(layers[node] + 1);
            }
        }
    }
    let layer_count = layers.iter().max().map_or(0, |max| max + 1);
    let mut rows = vec![Vec::new(); layer_count];
    for (node, &layer) in layers.iter().enumerate() {
        rows[layer].push(node);
    }

    // Reduce crossings: order each layer by the mean row of its neighbors, sweeping
    // down using callers and up using dependencies.
    let mut row_of = vec![0f64; node_count];
    let index_rows = |rows: &[Vec<usize>], row_of: &mut [f64]| {
        for row in rows {
            for (position, &node) in row.iter().enumerate() {
                row_of[node] = position as f64;
            }
        }
    };
    index_rows(&rows, &mut row_of);
    for sweep in 0..4 {
        let downward = sweep % 2 == 0;
        let order: Vec<usize> = if downward {
            (1..layer_count).collect()
        } else {
            (0..layer_count.saturating_sub(1)).rev().collect()
        };
        for layer in order {
            let mut keyed = rows[layer]
                .iter()
                .map(|&node| {
                    let neighbors = edges
                        .iter()
                        .filter_map(|&(from, to)| match downward {
                            true if to == node && layers[from] < layer => Some(row_of[from]),
                            false if from == node && layers[to] > layer => Some(row_of[to]),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    let key = if neighbors.is_empty() {
                        row_of[node]
                    } else {
                        neighbors.iter().sum::<f64>() / neighbors.len() as f64
                    };
                    (key, node)
                })
                .collect::<Vec<_>>();
            keyed.sort_by(|(a, _), (b, _)| a.total_cmp(b));
            rows[layer] = keyed.into_iter().map(|(_, node)| node).collect();
            index_rows(&rows[layer..=layer], &mut row_of);
        }
    }

    GraphLayout {
        layers,
        rows,
        back_edges,
    }
}

#[derive(Properties, PartialEq)]
pub struct ComponentGraphProps {
    pub deployment_id: DeploymentId,
    pub components_by_name: Rc<HashMap<String, grpc_client::Component>>,
}

#[component(ComponentGraph)]
pub fn component_graph(
    ComponentGraphProps {
        deployment_id,
        components_by_name,
    }: &ComponentGraphProps,
) -> Html {
    let hovered = use_state(|| None::<usize>);
    let navigator = use_navigator().expect("navigator must be available inside a router");

    let mut components = components_by_name.values().collect::<Vec<_>>();
    components.sort_by(|a, b| component_name(a).cmp(component_name(b)));
    let index_of = components
        .iter()
        .enumerate()
        .map(|(idx, component)| (component_name(component), idx))
        .collect::<HashMap<_, _>>();
    let all_edges = components
        .iter()
        .enumerate()
        .flat_map(|(from, component)| {
            component_connections(component, components_by_name)
                .dependencies
                .into_iter()
                .map(|dependency| GraphEdge {
                    from,
                    to: index_of[component_name(dependency.component)],
                    functions: dependency.functions,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    // Only connected components are drawn; the rest are listed below the graph.
    let mut connected = vec![false; components.len()];
    for edge in &all_edges {
        connected[edge.from] = true;
        connected[edge.to] = true;
    }
    let graph_nodes = (0..components.len())
        .filter(|&idx| connected[idx])
        .collect::<Vec<_>>();
    let dynamic = components
        .iter()
        .map(|component| calls_dynamically(component))
        .collect::<Vec<_>>();
    let dynamic_components = (0..components.len())
        .filter(|&idx| dynamic[idx])
        .map(|idx| components[idx])
        .collect::<Vec<_>>();
    let unconnected = (0..components.len())
        .filter(|&idx| !connected[idx] && !dynamic[idx])
        .map(|idx| components[idx])
        .collect::<Vec<_>>();
    let graph_index = graph_nodes
        .iter()
        .enumerate()
        .map(|(graph_idx, &idx)| (idx, graph_idx))
        .collect::<HashMap<_, _>>();
    let edges = all_edges
        .iter()
        .map(|edge| (graph_index[&edge.from], graph_index[&edge.to]))
        .collect::<Vec<_>>();
    let layout = layout_graph(graph_nodes.len(), &edges);

    // Geometry: each layer is as wide as its longest label, layers are centered vertically.
    let node_width = |graph_idx: usize| {
        component_name(components[graph_nodes[graph_idx]])
            .chars()
            .count() as f64
            * CHAR_WIDTH
            + NODE_CHROME
    };
    let layer_widths = layout
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|&node| node_width(node))
                .fold(0f64, f64::max)
        })
        .collect::<Vec<_>>();
    let mut layer_x = Vec::with_capacity(layer_widths.len());
    let mut x = MARGIN;
    for width in &layer_widths {
        layer_x.push(x);
        x += width + LAYER_GAP;
    }
    let width = x - LAYER_GAP + MARGIN;
    let max_rows = layout.rows.iter().map(Vec::len).max().unwrap_or(0) as f64;
    let column_height = |rows: f64| rows * NODE_HEIGHT + (rows - 1.0).max(0.0) * ROW_GAP;
    let height = column_height(max_rows) + 2.0 * MARGIN + BACK_EDGE_BULGE;
    let mut positions = vec![(0f64, 0f64); graph_nodes.len()];
    for (layer, row) in layout.rows.iter().enumerate() {
        let top = MARGIN + (column_height(max_rows) - column_height(row.len() as f64)) / 2.0;
        for (position, &node) in row.iter().enumerate() {
            positions[node] = (
                layer_x[layer],
                top + position as f64 * (NODE_HEIGHT + ROW_GAP),
            );
        }
    }

    let is_adjacent = |node: usize| {
        hovered.is_none_or(|hovered| {
            hovered == node
                || edges
                    .iter()
                    .any(|&edge| edge == (hovered, node) || edge == (node, hovered))
        })
    };

    let edges_html = edges.iter().enumerate().map(|(idx, &(from, to))| {
        let (from_x, from_y) = positions[from];
        let (to_x, to_y) = positions[to];
        let path = if layout.back_edges[idx] {
            // Against the layer direction: leave and enter from below.
            let (start_x, start_y) = (from_x + node_width(from) / 2.0, from_y + NODE_HEIGHT);
            let (end_x, end_y) = (to_x + node_width(to) / 2.0, to_y + NODE_HEIGHT);
            let bottom = start_y.max(end_y) + BACK_EDGE_BULGE;
            format!("M{start_x},{start_y} C{start_x},{bottom} {end_x},{bottom} {end_x},{end_y}")
        } else {
            let (start_x, start_y) = (from_x + node_width(from), from_y + NODE_HEIGHT / 2.0);
            let (end_x, end_y) = (to_x, to_y + NODE_HEIGHT / 2.0);
            let control = (end_x - start_x) / 2.0;
            format!(
                "M{start_x},{start_y} C{},{start_y} {},{end_y} {end_x},{end_y}",
                start_x + control,
                end_x - control
            )
        };
        let state = hovered.map(|hovered| {
            if hovered == from || hovered == to {
                "active"
            } else {
                "dimmed"
            }
        });
        let edge = &all_edges[idx];
        let tooltip = std::iter::once(format!(
            "{} → {}",
            component_name(components[edge.from]),
            component_name(components[edge.to])
        ))
        .chain(edge.functions.iter().map(ToString::to_string))
        .collect::<Vec<_>>()
        .join("\n");
        let marker = if state == Some("active") {
            "url(#component-graph-arrow-active)"
        } else {
            "url(#component-graph-arrow)"
        };
        html! {
            <path class={classes!("component-graph-edge", state)} d={path} marker-end={marker}>
                <title>{tooltip}</title>
            </path>
        }
    });

    let nodes_html = graph_nodes.iter().enumerate().map(|(graph_idx, &idx)| {
        let component = components[idx];
        let name = component_name(component).to_string();
        let (x, y) = positions[graph_idx];
        let component_type = component.as_type();
        let onclick = {
            let navigator = navigator.clone();
            let deployment_id = deployment_id.clone();
            let name = name.clone();
            Callback::from(move |_: MouseEvent| {
                let route = Route::DeploymentDetail {
                    deployment_id: deployment_id.clone(),
                };
                let query = DeploymentQuery {
                    component: Some(name.clone()),
                };
                if let Err(err) = navigator.push_with_query(&route, &query) {
                    error!("Cannot open the component: {err:?}");
                }
            })
        };
        let onmouseenter = {
            let hovered = hovered.clone();
            Callback::from(move |_: MouseEvent| hovered.set(Some(graph_idx)))
        };
        let onmouseleave = {
            let hovered = hovered.clone();
            Callback::from(move |_: MouseEvent| hovered.set(None))
        };
        html! {
            <g
                class={classes!(
                    "component-graph-node",
                    component_type.to_string(),
                    dynamic[idx].then_some("dynamic"),
                    (!is_adjacent(graph_idx)).then_some("dimmed"),
                )}
                transform={format!("translate({x},{y})")}
                {onclick}
                {onmouseenter}
                {onmouseleave}
            >
                <title>
                    {format!("{} {name}", component_type.as_label())}
                    if dynamic[idx] {
                        {", calls functions dynamically"}
                    }
                </title>
                <rect width={node_width(graph_idx).to_string()} height={NODE_HEIGHT.to_string()} rx="6" />
                <text x="10" y={(NODE_HEIGHT / 2.0).to_string()}>
                    {format!("{} {name}", component_type.as_icon().as_char())}
                </text>
            </g>
        }
    });

    let link = |component: &grpc_client::Component| {
        html! { <li>{ component_link(component, deployment_id) }</li> }
    };

    html! {
        <section class="component-graph">
            if graph_nodes.is_empty() {
                <p class="component-empty-state">{"No component imports functions of another component."}</p>
            } else {
                <p class="component-section-help">
                    {"Arrows point from a caller to the component exporting the functions it imports. Dashed components call functions dynamically. Click a component to open it."}
                </p>
                <div class="component-graph-scroll">
                    <svg
                        width={width.to_string()}
                        height={height.to_string()}
                        viewBox={format!("0 0 {width} {height}")}
                        role="img"
                        aria-label="Component dependency graph"
                    >
                        <defs>
                            <marker id="component-graph-arrow" viewBox="0 0 10 10" refX="10" refY="5"
                                markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                                <path d="M0,0 L10,5 L0,10 z" />
                            </marker>
                            <marker id="component-graph-arrow-active" class="active" viewBox="0 0 10 10" refX="10" refY="5"
                                markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                                <path d="M0,0 L10,5 L0,10 z" />
                            </marker>
                        </defs>
                        { for edges_html }
                        { for nodes_html }
                    </svg>
                </div>
            }
            if !dynamic_components.is_empty() {
                <h4>{"Calls functions dynamically"}</h4>
                <p class="component-section-help">
                    {"These components can call any function of this deployment by name, e.g. from JS, so the graph may miss some of their dependencies."}
                </p>
                <ul class="component-graph-unconnected">
                    { for dynamic_components.into_iter().map(link) }
                </ul>
            }
            if !unconnected.is_empty() {
                <h4>{"Not connected"}</h4>
                <p class="component-section-help">
                    {"Components neither importing from nor exporting to another component of this deployment, nor calling functions dynamically."}
                </p>
                <ul class="component-graph-unconnected">
                    { for unconnected.into_iter().map(link) }
                </ul>
            }
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_layers_callers_first_and_breaks_cycles() {
        // 0 -> 1 -> 2 -> 1 (cycle), 0 -> 2, 3 -> 2
        let edges = [(0, 1), (1, 2), (2, 1), (0, 2), (3, 2)];

        let layout = layout_graph(4, &edges);

        assert_eq!(layout.back_edges, [false, false, true, false, false]);
        assert_eq!(layout.layers, [0, 1, 2, 0]);
        assert_eq!(layout.rows[0].len(), 2);
        assert_eq!(layout.rows[1], [1]);
        assert_eq!(layout.rows[2], [2]);
    }

    #[test]
    fn layout_orders_layers_to_follow_their_callers() {
        // Callers 0 and 1 each use one dependency, named in the opposite order.
        let edges = [(0, 3), (1, 2)];

        let layout = layout_graph(4, &edges);

        assert_eq!(layout.rows[0], [0, 1]);
        assert_eq!(layout.rows[1], [3, 2]);
    }
}
