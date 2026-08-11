//! URL-fragment-encoded highlighting shared by the trace view (which applies it) and the
//! execution-log / debugger views (which link into it). The fragment `#trace-highlight-<v>`
//! names a version; the trace view resolves it to a correlated submit/await group and marks
//! both panes, so the highlight is bookmarkable and reachable from other views.

use super::execution_trace::{
    clear_all_linked_highlights, scroll_linked_item, set_linked_highlight,
};
use crate::app::Route;
use crate::grpc::grpc_client::ExecutionId;
use crate::grpc::version::VersionType;
use hashbrown::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use yew::prelude::*;
use yew_router::prelude::use_navigator;

const HIGHLIGHT_HASH_PREFIX: &str = "trace-highlight-";

/// The currently highlighted version parsed from the URL fragment, if any.
pub fn highlighted_version_from_hash() -> Option<VersionType> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let hash = hash.strip_prefix('#').unwrap_or(&hash);
    hash.strip_prefix(HIGHLIGHT_HASH_PREFIX)?.parse().ok()
}

/// Write (or clear) the highlight fragment. This fires a `hashchange` event, which the trace
/// view listens for to (re)apply the highlight.
pub fn set_highlight_hash(version: Option<VersionType>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let fragment = match version {
        Some(version) => format!("{HIGHLIGHT_HASH_PREFIX}{version}"),
        None => String::new(),
    };
    let _ = window.location().set_hash(&fragment);
}

/// Apply the highlight named by the current fragment: clear any previous marks, then mark
/// every node/event in the resolved group and scroll both panes to it. `scrolled` remembers
/// the version we last scrolled to so incremental data loads re-mark without re-scrolling.
pub fn apply_highlight_from_hash(
    version_to_group: &HashMap<VersionType, Vec<VersionType>>,
    scrolled: &Rc<RefCell<Option<VersionType>>>,
) {
    clear_all_linked_highlights();
    let Some(version) = highlighted_version_from_hash() else {
        *scrolled.borrow_mut() = None;
        return;
    };
    let Some(group) = version_to_group.get(&version) else {
        // The group's events are not loaded yet; a later data update will retry.
        return;
    };
    set_linked_highlight(group, true);
    if *scrolled.borrow() != Some(version) {
        if let Some(starting_version) = group.first().copied() {
            scroll_linked_item("trace-tree-pane", &format!("trace-node-{starting_version}"));
            scroll_linked_item(
                "trace-detail-pane",
                &format!("trace-event-{starting_version}"),
            );
        }
        *scrolled.borrow_mut() = Some(version);
    }
}

#[derive(Properties, PartialEq)]
pub struct TraceHighlightJumpProps {
    pub execution_id: ExecutionId,
    pub version: VersionType,
}

/// Button shown on a correlated event in the log/debugger views that opens the trace view
/// with this event highlighted.
#[component(TraceHighlightJump)]
pub fn trace_highlight_jump(props: &TraceHighlightJumpProps) -> Html {
    let navigator = use_navigator().expect("navigator should be available");
    let onclick = {
        let execution_id = props.execution_id.clone();
        let version = props.version;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            navigator.push(&Route::ExecutionTrace {
                execution_id: execution_id.clone(),
            });
            set_highlight_hash(Some(version));
        })
    };
    html! {
        <button
            type="button"
            class="trace-link-button"
            title="Show and highlight this event in the trace view"
            {onclick}
        >
            {"\u{21C4}"}
        </button>
    }
}
