//! Modal for reviewing captured writes and advancing an execution.
//!
//! Layout: vertically split — left pane lists the captured writes,
//! right pane shows the backtrace + source code for the selected write.

use crate::{
    BASE_URL,
    components::code::syntect_code_block::{
        DEFAULT_CONTEXT_LINES, SyntectCodeBlock, highlight_code_line_by_line,
    },
    grpc::grpc_client::{
        self, CapturedBacktrace, CapturedWrite, ComponentId, ExecutionId,
        GetBacktraceSourceRequest, captured_write, execution_event, execution_event::history_event,
        execution_repository_client::ExecutionRepositoryClient,
    },
};
use hashbrown::HashMap;
use log::trace;
use std::path::PathBuf;
use std::rc::Rc;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

// ============================================================================
// Helpers: summarise a CapturedWrite for the left-pane list
// ============================================================================

/// One-line label for an `ExecutionEvent` variant.
fn event_type_name(event: &grpc_client::ExecutionEvent) -> &'static str {
    match &event.event {
        Some(execution_event::Event::Created(_)) => "Created",
        Some(execution_event::Event::Locked(_)) => "Locked",
        Some(execution_event::Event::Unlocked(_)) => "Unlocked",
        Some(execution_event::Event::TemporarilyFailed(_)) => "TemporarilyFailed",
        Some(execution_event::Event::TemporarilyTimedOut(_)) => "TemporarilyTimedOut",
        Some(execution_event::Event::Finished(_)) => "Finished",
        Some(execution_event::Event::Paused(_)) => "Paused",
        Some(execution_event::Event::Unpaused(_)) => "Unpaused",
        Some(execution_event::Event::HistoryVariant(h)) => match &h.event {
            Some(history_event::Event::Persist(_)) => "Persist",
            Some(history_event::Event::JoinSetCreated(_)) => "JoinSetCreated",
            Some(history_event::Event::JoinSetRequest(_)) => "JoinSetRequest",
            Some(history_event::Event::JoinNext(_)) => "JoinNext",
            Some(history_event::Event::JoinNextTooMany(_)) => "JoinNextTooMany",
            Some(history_event::Event::JoinNextTry(_)) => "JoinNextTry",
            Some(history_event::Event::Schedule(_)) => "Schedule",
            Some(history_event::Event::Stub(_)) => "Stub",
            None => "History(?)",
        },
        None => "(?)",
    }
}

struct WriteSummary {
    version: u32,
    kind: &'static str,
    detail: String,
}

fn summarise_write(cw: &CapturedWrite) -> WriteSummary {
    match &cw.write {
        Some(captured_write::Write::Append(a)) => {
            let name = a.event.as_ref().map(event_type_name).unwrap_or("(empty)");
            WriteSummary {
                version: a.version,
                kind: "Append",
                detail: name.to_string(),
            }
        }
        Some(captured_write::Write::AppendBatch(b)) => WriteSummary {
            version: b.version,
            kind: "AppendBatch",
            detail: format!("{} events", b.events.len()),
        },
        Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => WriteSummary {
            version: b.version,
            kind: "AppendBatch+Create",
            detail: format!(
                "{} events + {} child exec",
                b.events.len(),
                b.child_requests.len()
            ),
        },
        Some(captured_write::Write::AppendStubResponse(s)) => WriteSummary {
            version: s.version,
            kind: "StubResponse",
            detail: s
                .child_execution_id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
        },
        Some(captured_write::Write::AppendFinished(f)) => WriteSummary {
            version: f.version,
            kind: "Finished",
            detail: String::new(),
        },
        None => WriteSummary {
            version: 0,
            kind: "(empty)",
            detail: String::new(),
        },
    }
}

/// Extract backtraces from a captured write.
fn backtraces_of(cw: &CapturedWrite) -> &[CapturedBacktrace] {
    match &cw.write {
        Some(captured_write::Write::Append(a)) => &a.backtraces,
        Some(captured_write::Write::AppendBatch(b)) => &b.backtraces,
        Some(captured_write::Write::AppendBatchCreateNewExecution(b)) => &b.backtraces,
        // StubResponse and AppendFinished don't carry backtraces
        _ => &[],
    }
}

// ============================================================================
// Source cache (keyed by ComponentId + file path)
// ============================================================================

type SourceKey = (ComponentId, String);

#[derive(Clone, PartialEq)]
enum SourceState {
    Requested,
    InFlight,
    Found(Rc<[(Html, usize)]>),
    NotFound,
}

// ============================================================================
// AdvanceModal component
// ============================================================================

#[derive(Properties, PartialEq)]
pub struct AdvanceModalProps {
    pub execution_id: ExecutionId,
    pub captured_writes: Vec<CapturedWrite>,
    pub on_advance: Callback<Vec<CapturedWrite>>,
    pub on_close: Callback<()>,
}

#[component(AdvanceModal)]
pub fn advance_modal(props: &AdvanceModalProps) -> Html {
    let selected_idx = use_state(|| 0usize);
    let advancing = use_state(|| false);

    // Reset selection and advancing state when captured_writes change (e.g. after re-replay)
    {
        let selected_idx = selected_idx.clone();
        let advancing = advancing.clone();
        use_effect_with(props.captured_writes.clone(), move |_| {
            selected_idx.set(0);
            advancing.set(false);
        });
    }

    // Source cache: ComponentId+file -> highlighted source
    let sources = use_state(HashMap::<SourceKey, SourceState>::new);
    // Expansion state for code blocks, keyed by "component:file"
    let expansion_map = use_state(HashMap::<String, (usize, usize)>::new);

    // Fetch sources for the currently selected write's backtraces
    let selected_backtraces: Vec<CapturedBacktrace> = props
        .captured_writes
        .get(*selected_idx)
        .map(backtraces_of)
        .unwrap_or(&[])
        .to_vec();

    // Trigger source fetches for any new files in the selected backtraces
    {
        let sources = sources.clone();
        let selected_backtraces = selected_backtraces.clone();
        use_effect_with((*selected_idx, selected_backtraces.clone()), move |_| {
            let mut needed: Vec<(ComponentId, String)> = Vec::new();
            for bt in &selected_backtraces {
                let component_id = match &bt.component_id {
                    Some(id) => id.clone(),
                    None => continue,
                };
                let wasm_bt = match &bt.wasm_backtrace {
                    Some(b) => b,
                    None => continue,
                };
                for frame in &wasm_bt.frames {
                    for symbol in &frame.symbols {
                        if let Some(file) = &symbol.file {
                            let key = (component_id.clone(), file.clone());
                            if !sources.contains_key(&key) {
                                needed.push(key);
                            }
                        }
                    }
                }
            }

            if !needed.is_empty() {
                // Mark all as Requested
                let mut next = (*sources).clone();
                for key in &needed {
                    next.entry(key.clone()).or_insert(SourceState::Requested);
                }
                sources.set(next);

                // Fetch each
                for (component_id, file) in needed {
                    let sources = sources.clone();
                    spawn_local(async move {
                        // Mark InFlight
                        {
                            let mut next = (*sources).clone();
                            next.insert(
                                (component_id.clone(), file.clone()),
                                SourceState::InFlight,
                            );
                            sources.set(next);
                        }

                        let mut client =
                            ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));
                        let result = client
                            .get_backtrace_source(tonic::Request::new(GetBacktraceSourceRequest {
                                component_id: Some(component_id.clone()),
                                file: file.clone(),
                            }))
                            .await;

                        let state = match result {
                            Ok(resp) => {
                                let language = PathBuf::from(&file)
                                    .extension()
                                    .map(|e| e.to_string_lossy().to_string());
                                SourceState::Found(Rc::from(highlight_code_line_by_line(
                                    &resp.into_inner().content,
                                    language.as_deref(),
                                )))
                            }
                            Err(err) => {
                                trace!("Cannot obtain source {file}: {err:?}");
                                SourceState::NotFound
                            }
                        };

                        let mut next = (*sources).clone();
                        next.insert((component_id, file), state);
                        sources.set(next);
                    });
                }
            }
        });
    }

    // Left pane: list of captured writes
    let write_list = props
        .captured_writes
        .iter()
        .enumerate()
        .map(|(idx, cw)| {
            let summary = summarise_write(cw);
            let is_selected = idx == *selected_idx;
            let class = classes!("captured-write-item", is_selected.then_some("selected"),);
            let on_click = {
                let selected_idx = selected_idx.clone();
                Callback::from(move |_: MouseEvent| {
                    selected_idx.set(idx);
                })
            };
            html! {
                <div {class} onclick={on_click}>
                    <span class="captured-write-version">{format!("v{}", summary.version)}</span>
                    <span class="captured-write-kind">{summary.kind}</span>
                    if !summary.detail.is_empty() {
                        <span class="captured-write-detail">{summary.detail}</span>
                    }
                </div>
            }
        })
        .collect::<Html>();

    // Right pane: backtrace + source for selected write
    let backtrace_view = if selected_backtraces.is_empty() {
        html! {
            <div class="advance-backtrace">
                <p class="backtrace-empty">{"No backtraces for this write"}</p>
            </div>
        }
    } else {
        let mut seen_positions = hashbrown::HashSet::new();
        html! {
            <div class="advance-backtrace">
                { for selected_backtraces.iter().map(|bt| {
                    let component_id = bt.component_id.as_ref();
                    let wasm_bt = match &bt.wasm_backtrace {
                        Some(b) => b,
                        None => return html! { <p class="backtrace-empty">{"No backtrace data"}</p> },
                    };
                    html! {
                        { for wasm_bt.frames.iter().map(|frame| {
                            let mut frame_html: Vec<Html> = Vec::new();
                            for symbol in &frame.symbols {
                                // Location line
                                let location = match (&symbol.file, symbol.line, symbol.col) {
                                    (Some(file), Some(line), Some(col)) => format!("{file}:{line}:{col}"),
                                    (Some(file), Some(line), None) => format!("{file}:{line}"),
                                    (Some(file), None, None) => file.clone(),
                                    _ => "unknown location".to_string(),
                                };
                                let mut loc_str = format!("at {location}");
                                match &symbol.func_name {
                                    Some(func_name) if *func_name != frame.func_name => {
                                        loc_str.push_str(&format!(" - {func_name}"));
                                    }
                                    _ => {}
                                }
                                frame_html.push(html! { <div class="symbol-info">{loc_str}</div> });

                                // Source code block
                                if let (Some(file), Some(line), Some(comp_id)) =
                                    (&symbol.file, symbol.line, component_id)
                                {
                                    let new_position = seen_positions.insert((file.clone(), line));
                                    if new_position {
                                        let key = (comp_id.clone(), file.clone());
                                        if let Some(SourceState::Found(source)) = sources.get(&key)
                                        {
                                            let map_key = format!("{}:{}", comp_id.name, file);
                                            let (cb_above, cb_below) = expansion_map
                                                .get(&map_key)
                                                .copied()
                                                .unwrap_or((
                                                    DEFAULT_CONTEXT_LINES,
                                                    DEFAULT_CONTEXT_LINES,
                                                ));
                                            let on_expand = {
                                                let expansion_map = expansion_map.clone();
                                                let map_key = map_key.clone();
                                                Callback::from(
                                                    move |(new_above, new_below): (
                                                        usize,
                                                        usize,
                                                    )| {
                                                        let mut next =
                                                            (*expansion_map).clone();
                                                        next.insert(
                                                            map_key.clone(),
                                                            (new_above, new_below),
                                                        );
                                                        expansion_map.set(next);
                                                    },
                                                )
                                            };
                                            frame_html.push(html! {
                                                <SyntectCodeBlock
                                                    key={map_key}
                                                    source={source.clone()}
                                                    focus_line={Some(line as usize)}
                                                    lines_above={cb_above}
                                                    lines_below={cb_below}
                                                    on_expand={on_expand}
                                                />
                                            });
                                        }
                                    }
                                }
                            }
                            html! { <div class="frame-container">{frame_html}</div> }
                        })}
                    }
                })}
            </div>
        }
    };

    // Footer: Advance + Cancel
    let on_advance = {
        let captured_writes = props.captured_writes.clone();
        let on_advance = props.on_advance.clone();
        let advancing = advancing.clone();
        Callback::from(move |_: MouseEvent| {
            advancing.set(true);
            on_advance.emit(captured_writes.clone());
        })
    };

    let on_cancel = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            on_close.emit(());
        })
    };

    // Close on overlay click (but not on modal body click)
    let on_overlay_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |e: MouseEvent| {
            // Only close if the click target is the overlay itself
            if let Some(target) = e.target_dyn_into::<web_sys::Element>()
                && target
                    .get_attribute("class")
                    .unwrap_or_default()
                    .contains("modal-overlay")
            {
                on_close.emit(());
            }
        })
    };

    let is_advancing = *advancing;

    html! {
        <div class="modal-overlay" onclick={on_overlay_click}>
            <div class="modal-window">
                <div class="modal-header">
                    <h3>{format!("Advance Execution — {} writes", props.captured_writes.len())}</h3>
                    <button class="modal-dismiss" onclick={on_cancel.clone()}>{"×"}</button>
                </div>
                <div class="modal-body">
                    <div class="modal-pane-left">
                        {write_list}
                    </div>
                    <div class="modal-pane-right">
                        {backtrace_view}
                    </div>
                </div>
                <div class="modal-footer">
                    <button class="action-button" onclick={on_cancel}>{"Cancel"}</button>
                    <button
                        class="action-button advance-button"
                        onclick={on_advance}
                        disabled={is_advancing}
                    >
                        if is_advancing {
                            {"Advancing..."}
                        } else {
                            {"Advance"}
                        }
                    </button>
                </div>
            </div>
        </div>
    }
}
