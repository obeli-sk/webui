use crate::{
    app::Route,
    components::{
        notification::{Notification, NotificationContext},
        system_nav::SystemNav,
    },
    rest::{self, admin::RetentionPolicy},
};
use gloo::timers::callback::Timeout;
use rest_types::admin::CleanupResponse;
use serde_json::Value;
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlInputElement, HtmlSelectElement};
use yew::prelude::*;

const DELETE_BATCH_SIZE: u32 = 1000;
/// Server-side maximum; a dry run reports at most this many.
const PREVIEW_BATCH_SIZE: u32 = 10_000;

#[derive(Clone, Copy, PartialEq)]
enum RetentionKind {
    Executions,
    Deployments,
    SystemEvents,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Count,
    MaxAge,
}

#[derive(Clone, Copy, PartialEq)]
enum AgeUnit {
    Minutes,
    Hours,
    Days,
}

impl AgeUnit {
    const ALL: [AgeUnit; 3] = [AgeUnit::Minutes, AgeUnit::Hours, AgeUnit::Days];

    fn seconds(self) -> u64 {
        match self {
            AgeUnit::Minutes => 60,
            AgeUnit::Hours => 60 * 60,
            AgeUnit::Days => 24 * 60 * 60,
        }
    }

    fn label(self) -> &'static str {
        match self {
            AgeUnit::Minutes => "minutes",
            AgeUnit::Hours => "hours",
            AgeUnit::Days => "days",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Options {
    policy: RetentionPolicy,
    delete_executions: bool,
    force_non_terminal: bool,
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Totals {
    execution_trees: u64,
    deployments: u64,
    system_events: u64,
    blocked_non_terminal: u64,
    blocked_by_execution_reference: u64,
    has_more: bool,
}

#[derive(Clone, PartialEq)]
struct Batch {
    totals: Totals,
    next_cursor: Option<Value>,
}

impl Batch {
    fn can_continue(&self, cursor: &Option<Value>) -> bool {
        self.totals.has_more
            && (self.totals.progressed()
                || self
                    .next_cursor
                    .as_ref()
                    .is_some_and(|next| Some(next) != cursor.as_ref()))
    }
}

#[derive(Clone, PartialEq)]
struct Preview {
    cursor: Option<Value>,
    batch: Batch,
}

impl Preview {
    fn next_cursor(&self) -> Option<Value> {
        self.batch
            .next_cursor
            .clone()
            .filter(|next| self.batch.totals.has_more && Some(next) != self.cursor.as_ref())
    }
}

impl Totals {
    fn add(&mut self, batch: Totals, follows_cursor: bool) {
        self.execution_trees += batch.execution_trees;
        self.deployments += batch.deployments;
        self.system_events += batch.system_events;
        if follows_cursor {
            self.blocked_non_terminal += batch.blocked_non_terminal;
            self.blocked_by_execution_reference += batch.blocked_by_execution_reference;
        } else {
            // backcompat: 0.43.0-rc.11 reselects blockers on every batch.
            self.blocked_non_terminal = batch.blocked_non_terminal;
            self.blocked_by_execution_reference = batch.blocked_by_execution_reference;
        }
        self.has_more = batch.has_more;
    }

    fn progressed(&self) -> bool {
        self.execution_trees + self.deployments + self.system_events > 0
    }
}

impl From<CleanupResponse> for Totals {
    fn from(response: CleanupResponse) -> Self {
        Self {
            execution_trees: response.deleted_execution_trees,
            deployments: response.deleted_deployments,
            system_events: 0,
            blocked_non_terminal: response.blocked_non_terminal,
            blocked_by_execution_reference: response.blocked_by_execution_reference,
            has_more: response.has_more,
        }
    }
}

impl RetentionKind {
    fn title(self) -> &'static str {
        match self {
            RetentionKind::Executions => "Executions",
            RetentionKind::Deployments => "Deployments",
            RetentionKind::SystemEvents => "System events",
        }
    }

    fn description(self) -> &'static str {
        match self {
            RetentionKind::Executions => {
                "Deletes top-level execution trees, children included. Only finished trees are considered unless non-terminal ones are included; unfinished trees of the active deployment stay protected."
            }
            RetentionKind::Deployments => {
                "Deletes inactive deployments. A deployment still referenced by execution trees blocks deletion unless its trees are deleted as well."
            }
            RetentionKind::SystemEvents => {
                "Deletes system events older than the given age. Events of the current node run are kept."
            }
        }
    }

    fn count_label(self) -> Option<&'static str> {
        match self {
            RetentionKind::Executions => Some("Keep newest execution trees"),
            RetentionKind::Deployments => Some("Keep newest inactive deployments"),
            RetentionKind::SystemEvents => None,
        }
    }

    /// Mirrors the server's periodic GC `max_count` default.
    fn default_count(self) -> Option<u32> {
        match self {
            RetentionKind::Executions => Some(1000),
            RetentionKind::Deployments => Some(100),
            RetentionKind::SystemEvents => None,
        }
    }

    fn age_label(self) -> &'static str {
        match self {
            RetentionKind::Executions => "Delete trees last updated more than",
            RetentionKind::Deployments => "Delete deployments inactive for more than",
            RetentionKind::SystemEvents => "Delete events older than",
        }
    }

    async fn run(
        self,
        options: Options,
        batch_size: u32,
        dry_run: bool,
        cursor: Option<&Value>,
    ) -> Result<Batch, String> {
        match self {
            RetentionKind::Executions => rest::admin::retain_executions(
                options.policy,
                batch_size,
                options.force_non_terminal,
                dry_run,
                cursor,
            )
            .await
            .map(|response| Batch {
                next_cursor: response
                    .next_execution_cursor
                    .as_ref()
                    .and_then(|cursor| serde_json::to_value(cursor).ok()),
                totals: Totals::from(response),
            }),
            RetentionKind::Deployments => rest::admin::retain_deployments(
                options.policy,
                batch_size,
                options.delete_executions,
                options.force_non_terminal,
                dry_run,
                cursor,
            )
            .await
            .map(|response| Batch {
                next_cursor: response
                    .next_deployment_cursor
                    .as_ref()
                    .and_then(|cursor| serde_json::to_value(cursor).ok()),
                totals: Totals::from(response),
            }),
            RetentionKind::SystemEvents => {
                let RetentionPolicy::MaxAgeSeconds(max_age_seconds) = options.policy else {
                    return Err("System events can only be retained by age".to_string());
                };
                let response =
                    rest::admin::retain_system_events(max_age_seconds, batch_size, dry_run).await?;
                Ok(Batch {
                    totals: Totals {
                        system_events: response.deleted,
                        has_more: response.has_more,
                        ..Totals::default()
                    },
                    next_cursor: None,
                })
            }
        }
    }

    fn summary(self, totals: &Totals, dry_run: bool) -> String {
        let more = if dry_run && totals.has_more { "+" } else { "" };
        let verb = if dry_run { "Would delete" } else { "Deleted" };
        let mut message = match self {
            RetentionKind::Executions => {
                format!("{verb} {}{more} execution tree(s)", totals.execution_trees)
            }
            RetentionKind::Deployments => format!(
                "{verb} {}{more} deployment(s) and {} execution tree(s)",
                totals.deployments, totals.execution_trees
            ),
            RetentionKind::SystemEvents => {
                format!("{verb} {}{more} system event(s)", totals.system_events)
            }
        };
        if totals.blocked_non_terminal > 0 {
            message.push_str(&format!(
                "; {} blocked by non-terminal executions",
                totals.blocked_non_terminal
            ));
        }
        if totals.blocked_by_execution_reference > 0 {
            message.push_str(&format!(
                "; {} blocked by referencing execution trees",
                totals.blocked_by_execution_reference
            ));
        }
        if totals.has_more {
            message.push_str("; more work remains");
        }
        message
    }
}

#[component(RetentionPage)]
pub fn retention_page() -> Html {
    html! {
        <main>
            <SystemNav active={Route::Retention} />
            <h1>{"Retention"}</h1>
            <p class="retention-intro">
                {"Deletion only tombstones rows; storage is reclaimed by the background garbage collector. All runs are recorded as system events."}
            </p>
            <p class="retention-intro">
                {"Previews show one batch. Deletion continues through all remaining batches, starting from the displayed preview."}
            </p>
            <RetentionForm kind={RetentionKind::Executions} />
            <RetentionForm kind={RetentionKind::Deployments} />
            <RetentionForm kind={RetentionKind::SystemEvents} />
        </main>
    }
}

#[derive(PartialEq, Properties)]
struct RetentionFormProps {
    kind: RetentionKind,
}

#[component(RetentionForm)]
fn retention_form(RetentionFormProps { kind }: &RetentionFormProps) -> Html {
    let kind = *kind;
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let mode = use_state(|| {
        if kind.count_label().is_some() {
            Mode::Count
        } else {
            Mode::MaxAge
        }
    });
    let count = use_state(|| kind.default_count().unwrap_or_default().to_string());
    let age = use_state(|| "31".to_string());
    let age_unit = use_state(|| AgeUnit::Days);
    let delete_executions = use_state(|| false);
    let force_non_terminal = use_state(|| false);
    let preview = use_state(|| None::<Preview>);
    let cursor = use_state(|| None::<Value>);
    let outcome = use_state(|| None::<String>);
    let armed = use_state(|| false);
    let in_flight = use_state(|| false);
    let disarm_timer = use_mut_ref(|| None::<Timeout>);

    let options: Result<Options, String> = (|| {
        let policy = match *mode {
            Mode::Count => RetentionPolicy::Count(
                count
                    .trim()
                    .parse()
                    .map_err(|_| "Count must be a non-negative integer".to_string())?,
            ),
            Mode::MaxAge => {
                let value: u64 = age
                    .trim()
                    .parse()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "Age must be a positive integer".to_string())?;
                RetentionPolicy::MaxAgeSeconds(
                    value
                        .checked_mul(age_unit.seconds())
                        .ok_or_else(|| "Age is too large".to_string())?,
                )
            }
        };
        let delete_executions = kind == RetentionKind::Deployments && *delete_executions;
        Ok(Options {
            policy,
            delete_executions,
            force_non_terminal: *force_non_terminal
                && (kind == RetentionKind::Executions || delete_executions),
        })
    })();

    let reset = {
        let preview = preview.clone();
        let cursor = cursor.clone();
        let outcome = outcome.clone();
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        move || {
            preview.set(None);
            cursor.set(None);
            outcome.set(None);
            armed.set(false);
            *disarm_timer.borrow_mut() = None;
        }
    };

    let set_mode = |new_mode: Mode| {
        let mode = mode.clone();
        let reset = reset.clone();
        Callback::from(move |_| {
            mode.set(new_mode);
            reset();
        })
    };
    let on_text = |state: UseStateHandle<String>| {
        let reset = reset.clone();
        Callback::from(move |event: InputEvent| {
            let input: HtmlInputElement = event.target_unchecked_into();
            state.set(input.value());
            reset();
        })
    };
    let on_checkbox = |state: UseStateHandle<bool>| {
        let reset = reset.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            state.set(input.checked());
            reset();
        })
    };
    let on_unit = {
        let age_unit = age_unit.clone();
        let reset = reset.clone();
        Callback::from(move |event: Event| {
            let select: HtmlSelectElement = event.target_unchecked_into();
            if let Some(unit) = AgeUnit::ALL
                .into_iter()
                .find(|unit| unit.label() == select.value())
            {
                age_unit.set(unit);
            }
            reset();
        })
    };

    let request_preview = {
        let options = options.clone();
        let preview = preview.clone();
        let cursor = cursor.clone();
        let in_flight = in_flight.clone();
        let notifications = notifications.clone();
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        Callback::from(move |request_cursor: Option<Value>| {
            let Ok(options) = options.clone() else {
                return;
            };
            armed.set(false);
            *disarm_timer.borrow_mut() = None;
            in_flight.set(true);
            let preview = preview.clone();
            let cursor = cursor.clone();
            let in_flight = in_flight.clone();
            let notifications = notifications.clone();
            spawn_local(async move {
                match kind
                    .run(options, PREVIEW_BATCH_SIZE, true, request_cursor.as_ref())
                    .await
                {
                    Ok(batch) => {
                        cursor.set(request_cursor.clone());
                        preview.set(Some(Preview {
                            cursor: request_cursor,
                            batch,
                        }));
                    }
                    Err(error) => notifications.push(Notification::error(format!(
                        "Failed to preview {} retention: {error}",
                        kind.title().to_lowercase()
                    ))),
                }
                in_flight.set(false);
            });
        })
    };
    let on_preview = {
        let request_preview = request_preview.clone();
        let cursor = cursor.clone();
        Callback::from(move |_| request_preview.emit((*cursor).clone()))
    };
    let on_next_preview = {
        let request_preview = request_preview.clone();
        let next_cursor = preview.as_ref().and_then(Preview::next_cursor);
        Callback::from(move |_| request_preview.emit(next_cursor.clone()))
    };
    let on_restart = {
        let reset = reset.clone();
        Callback::from(move |_| reset())
    };

    let on_delete = {
        let options = options.clone();
        let is_armed = *armed;
        let armed = armed.clone();
        let disarm_timer = disarm_timer.clone();
        let in_flight = in_flight.clone();
        let outcome = outcome.clone();
        let preview = preview.clone();
        let cursor = cursor.clone();
        Callback::from(move |_| {
            let Ok(options) = options.clone() else {
                return;
            };
            if !is_armed {
                armed.set(true);
                let armed = armed.clone();
                *disarm_timer.borrow_mut() = Some(Timeout::new(10000, move || armed.set(false)));
                return;
            }
            armed.set(false);
            *disarm_timer.borrow_mut() = None;
            in_flight.set(true);
            let mut request_cursor = preview
                .as_ref()
                .map_or_else(|| (*cursor).clone(), |preview| preview.cursor.clone());
            preview.set(None);
            let cursor = cursor.clone();
            let in_flight = in_flight.clone();
            let outcome = outcome.clone();
            let notifications = notifications.clone();
            spawn_local(async move {
                let mut totals = Totals::default();
                let error = loop {
                    match kind
                        .run(options, DELETE_BATCH_SIZE, false, request_cursor.as_ref())
                        .await
                    {
                        Ok(batch) => {
                            let can_continue = batch.can_continue(&request_cursor);
                            let follows_cursor =
                                request_cursor.is_some() || batch.next_cursor.is_some();
                            totals.add(batch.totals, follows_cursor);
                            if batch.totals.has_more {
                                if let Some(next) = batch.next_cursor {
                                    request_cursor = Some(next);
                                }
                            } else {
                                request_cursor = None;
                            }
                            cursor.set(request_cursor.clone());
                            outcome
                                .set(Some(format!("{} (running…)", kind.summary(&totals, false))));
                            if !can_continue {
                                break None;
                            }
                        }
                        Err(error) => break Some(error),
                    }
                };
                let mut summary = kind.summary(&totals, false);
                if error.is_none() && totals.has_more {
                    summary.push_str("; stopped because the last batch made no progress");
                }
                outcome.set(Some(summary.clone()));
                match error {
                    None => notifications.push(Notification::success(summary)),
                    Some(error) => notifications.push(Notification::error(format!(
                        "{} retention failed after partial progress ({summary}): {error}",
                        kind.title()
                    ))),
                }
                in_flight.set(false);
            });
        })
    };

    let busy = *in_flight;
    let invalid = options.as_ref().err().cloned();
    let force_enabled = kind == RetentionKind::Executions || *delete_executions;

    html! {
        <section class="retention-section">
            <h2>{kind.title()}</h2>
            <p class="retention-description">{kind.description()}</p>
            if kind.count_label().is_some() {
                <div class="view-tabs retention-mode">
                    <button class={classes!((*mode == Mode::Count).then_some("active"))}
                        onclick={set_mode(Mode::Count)} disabled={busy}>{"By count"}</button>
                    <button class={classes!((*mode == Mode::MaxAge).then_some("active"))}
                        onclick={set_mode(Mode::MaxAge)} disabled={busy}>{"By age"}</button>
                </div>
            }
            <div class="retention-form">
                if let Some(count_label) = kind.count_label() {
                    if *mode == Mode::Count {
                        <label class="retention-field">
                            {count_label}
                            <input type="number" min="0" value={(*count).clone()}
                                oninput={on_text(count.clone())} disabled={busy} />
                        </label>
                    }
                }
                if *mode == Mode::MaxAge {
                    <label class="retention-field">
                        {kind.age_label()}
                        <input type="number" min="1" value={(*age).clone()}
                            oninput={on_text(age.clone())} disabled={busy} />
                        <select onchange={on_unit} disabled={busy}>
                            {for AgeUnit::ALL.into_iter().map(|unit| html! {
                                <option value={unit.label()} selected={*age_unit == unit}>{unit.label()}</option>
                            })}
                        </select>
                    </label>
                }
                if kind == RetentionKind::Deployments {
                    <label class="retention-checkbox">
                        <input type="checkbox" checked={*delete_executions}
                            onchange={on_checkbox(delete_executions.clone())} disabled={busy} />
                        {"Also delete execution trees"}
                    </label>
                }
                if kind != RetentionKind::SystemEvents {
                    <label class="retention-checkbox" title="Non-terminal trees belonging to the active deployment are never deleted">
                        <input type="checkbox" checked={*force_non_terminal && force_enabled}
                            onchange={on_checkbox(force_non_terminal.clone())}
                            disabled={busy || !force_enabled} />
                        {"Include non-terminal executions"}
                    </label>
                }
            </div>
            if let Some(invalid) = invalid.clone() {
                <p class="error">{invalid}</p>
            }
            <div class="retention-actions">
                <button class="action-button" onclick={on_preview} disabled={busy || invalid.is_some()}>
                    {if preview.is_some() { "Refresh preview" } else { "Preview" }}
                </button>
                if preview.as_ref().and_then(Preview::next_cursor).is_some() {
                    <button class="action-button" onclick={on_next_preview} disabled={busy || invalid.is_some()}>
                        {"Preview next batch"}
                    </button>
                }
                <button class={classes!("action-button", "retention-delete-button", (*armed).then_some("armed"))}
                    onclick={on_delete} disabled={busy || invalid.is_some()}>
                    {if busy { "Working..." } else if *armed { "Confirm delete" } else if cursor.is_some() { "Delete from this batch" } else { "Delete" }}
                </button>
                if preview.is_some() || outcome.is_some() || cursor.is_some() {
                    <button class="action-button" onclick={on_restart} disabled={busy}>{"Start over"}</button>
                }
            </div>
            if let Some(preview) = preview.as_ref() {
                <p class="retention-result">{kind.summary(&preview.batch.totals, true)}</p>
            }
            if let Some(outcome) = outcome.as_ref() {
                <p class="retention-result">{outcome}</p>
            }
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn blocked_batches_continue_when_the_cursor_advances() {
        let mut batch = Batch {
            totals: Totals {
                blocked_non_terminal: 2,
                has_more: true,
                ..Totals::default()
            },
            next_cursor: Some(json!({"timestamp": "2026-10-28T12:00:00Z", "execution_id": "E_1"})),
        };
        assert!(batch.can_continue(&None));
        assert!(!batch.can_continue(&batch.next_cursor));
        batch.next_cursor = None;
        assert!(!batch.can_continue(&None));
        batch.totals.execution_trees = 1;
        assert!(batch.can_continue(&None));
        batch.totals.has_more = false;
        assert!(!batch.can_continue(&None));
    }

    #[test]
    fn preview_does_not_advance_past_partial_execution_tree_work() {
        let cursor = Some(json!({"timestamp": "2026-10-28T12:00:00Z", "deployment_id": "Dep_1"}));
        let mut preview = Preview {
            cursor: cursor.clone(),
            batch: Batch {
                totals: Totals {
                    execution_trees: u64::from(PREVIEW_BATCH_SIZE),
                    has_more: true,
                    ..Totals::default()
                },
                next_cursor: cursor,
            },
        };
        assert!(preview.next_cursor().is_none());
        let next = json!({"timestamp": "2026-10-28T13:00:00Z", "deployment_id": "Dep_2"});
        preview.batch.next_cursor = Some(next.clone());
        assert_eq!(preview.next_cursor(), Some(next));
        assert_ne!(preview.cursor, preview.next_cursor());
        preview.batch.totals.has_more = false;
        assert!(preview.next_cursor().is_none());
    }
}
