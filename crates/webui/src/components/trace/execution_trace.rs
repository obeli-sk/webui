use super::data::TraceData;
use super::highlight::{highlighted_version_from_hash, set_highlight_hash};
use crate::grpc::version::VersionType;
use chrono::{DateTime, Utc};
use wasm_bindgen::JsCast;
use yew::prelude::*;

/// Toggle the linked-highlight class on every tree node and detail-side event in a
/// correlation group (submit + its join-next), so clicking any one highlights them all.
pub fn set_linked_highlight(versions: &[VersionType], on: bool) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    for version in versions {
        for id in [
            format!("trace-node-{version}"),
            format!("trace-event-{version}"),
        ] {
            if let Some(element) = document.get_element_by_id(&id) {
                let class_list = element.class_list();
                if on {
                    let _ = class_list.add_1("trace-linked-highlight");
                } else {
                    let _ = class_list.remove_1("trace-linked-highlight");
                }
            }
        }
    }
}

/// Remove the linked-highlight class from every element that currently carries it, so a
/// new click starts from a clean slate.
pub fn clear_all_linked_highlights() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let collection = document.get_elements_by_class_name("trace-linked-highlight");
    // Collect up front: removing the class mutates this live collection.
    let elements: Vec<_> = (0..collection.length())
        .filter_map(|i| collection.item(i))
        .collect();
    for element in elements {
        let _ = element.class_list().remove_1("trace-linked-highlight");
    }
}

pub fn scroll_linked_item(pane_id: &str, item_id: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(pane) = document
        .get_element_by_id(pane_id)
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let Some(item) = document
        .get_element_by_id(item_id)
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let centered_offset = (pane.client_height() - item.offset_height()) / 2;
    pane.set_scroll_top(item.offset_top() - centered_offset);
}

#[derive(Properties, PartialEq)]
pub struct ExecutionStepProps {
    pub data: TraceData,
    pub root_scheduled_at: DateTime<Utc>,
    pub root_last_event_at: DateTime<Utc>,
    pub on_toggle: Callback<String>,
}

#[component(ExecutionTrace)]
pub fn execution_trace(props: &ExecutionStepProps) -> Html {
    let intervals: Vec<_> = props
        .data
        .busy()
        .iter()
        .map(|interval| {
            let (start_percentage, busy_percentage) =
                interval.as_percentage(props.root_scheduled_at, props.root_last_event_at);
            html! {
                <div
                class={classes!("busy-duration-line", interval.status.get_css_class())}
                title={interval.title.clone()}
                style={format!("margin-left: {start_percentage}%; width: {busy_percentage}%;")}
            >
            </div>
            }
        })
        .collect();

    let children_html = if props.data.is_expanded() && !props.data.children().is_empty() {
        html! {
                <div class="indented-children"> // Wrap children in a container
                    { for props.data.children().iter().map(|child| html! {
                        <ExecutionTrace
                        key={format!("{}:{}", child.node_key(), child.title())}
                        data={child.clone()}
                        root_scheduled_at={props.root_scheduled_at}
                        root_last_event_at={props.root_last_event_at}
                        on_toggle={props.on_toggle.clone()}
                    />
                })}
            </div>
        }
    } else {
        Html::default()
    };
    let tooltip = if let TraceData::Root(root) = &props.data {
        format!(
            "Total: {:?}, busy: {:?}",
            root.total_duration(),
            props.data.busy_duration(props.root_last_event_at)
        )
    } else {
        format!("{:?}", props.data.busy_duration(props.root_last_event_at))
    };
    let last_status = props.data.current_status();
    let has_children = props.data.can_expand();
    let caret_class = if props.data.is_expanded() {
        "tree-caret tree-caret-open"
    } else {
        "tree-caret tree-caret-closed"
    };
    let toggle = {
        let on_toggle = props.on_toggle.clone();
        let node_key = props.data.node_key().to_string();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_toggle.emit(node_key.clone());
        })
    };

    let link = props.data.link();
    let link_versions = link.map(|link| {
        link.group
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    });
    let row_id = link.map(|link| format!("trace-node-{}", link.version));
    // A dedicated button (not a row click) toggles the linked highlight via the URL
    // fragment, so the tree's own expand/collapse behavior is left untouched. Unlinked rows
    // (the root and leaf descendants) render an invisible placeholder so every duration bar
    // stays aligned.
    let highlight_button = match link {
        Some(link) => {
            let group = link.group.clone();
            let starting_version = link.version;
            let on_highlight = Callback::from(move |e: MouseEvent| {
                e.stop_propagation();
                let already_active =
                    highlighted_version_from_hash().is_some_and(|version| group.contains(&version));
                set_highlight_hash((!already_active).then_some(starting_version));
            });
            html! {
                <button
                    type="button"
                    class="trace-link-button"
                    title="Highlight matching event in the detail pane"
                    onclick={on_highlight}
                >
                    {"\u{21C4}"}
                </button>
            }
        }
        None => html! {
            <span class="trace-link-button trace-link-button-placeholder" aria-hidden="true">
                {"\u{21C4}"}
            </span>
        },
    };

    html! {
        <div class="execution-trace">
            <div class="step-row" id={row_id}>
                <span class="step-icon">
                    if has_children {
                        <span class={caret_class} onclick={toggle}>
                            { if props.data.is_expanded() { "▼" } else { "▶" } }
                        </span>
                    } else {
                        <span class="tree-caret tree-caret-none">{"\u{00a0}\u{00a0}"}</span>
                    }
                </span>
                if let Some(versions) = link_versions {
                    <span class="step-version" title="Event versions">{versions}</span>
                }
                <span class="step-name" title={props.data.title().to_string()}>{props.data.name().clone()}</span>
                if let Some(status) = last_status {
                    <span class="step-status">
                        {props.data.load_button()}
                        {status}
                    </span>
                }
                <div class="relative-duration-container">
                    if !intervals.is_empty() {
                        <div class="total-duration-line" style="width: 100%" title={tooltip}>
                            {intervals}
                        </div>
                    }
                </div>
                { highlight_button }
            </div>
            {children_html}
        </div>
    }
}
