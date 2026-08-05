use super::data::TraceData;
use crate::grpc::version::VersionType;
use chrono::{DateTime, Utc};
use yew::prelude::*;

/// Toggle the linked-highlight class on every tree node and detail-side event in a
/// correlation group (submit + its join-next), so hovering any one highlights them all.
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
    let on_row_enter = link.map(|link| {
        let group = link.group.clone();
        Callback::from(move |_: MouseEvent| set_linked_highlight(&group, true))
    });
    let on_row_leave = link.map(|link| {
        let group = link.group.clone();
        Callback::from(move |_: MouseEvent| set_linked_highlight(&group, false))
    });

    html! {
        <div class="execution-trace">
            <div class="step-row" id={row_id} onmouseenter={on_row_enter} onmouseleave={on_row_leave}>
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
            </div>
            {children_html}
        </div>
    }
}
