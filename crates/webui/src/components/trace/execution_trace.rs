use super::data::TraceData;
use chrono::{DateTime, Utc};
use yew::prelude::*;

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
    let has_children = !props.data.children().is_empty();
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

    html! {
        <div class="execution-trace">
            <div class="step-row">
                <span class="step-icon">
                    if has_children {
                        <span class={caret_class} onclick={toggle}>
                            { if props.data.is_expanded() { "▼" } else { "▶" } }
                        </span>
                    } else {
                        <span class="tree-caret tree-caret-none">{"\u{00a0}\u{00a0}"}</span>
                    }
                </span>
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
