use crate::{
    app::{AppState, Route},
    components::{
        execution_list_page::{ExecutionQuery, StatusFilter, StatusFilterList},
        notification::{Notification, NotificationContext},
    },
    grpc::grpc_client::{
        self, DeploymentComponentType, DeploymentId, DeploymentStatus, DeploymentSummary,
        deployment_repository_client::DeploymentRepositoryClient,
        list_deployments_request::{NewerThan, OlderThan, Pagination},
    },
};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{ops::Deref, str::FromStr};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DeploymentQuery {
    pub cursor: Option<DeploymentCursor>,
    pub direction: Option<Direction>,
    #[serde(default)]
    pub include_cursor: bool,
    /// Count (and link to) child executions as well; top-level only by default.
    #[serde(default)]
    pub show_derived: bool,
}

impl DeploymentQuery {
    fn flip(mut self, old_direction: Direction) -> DeploymentQuery {
        self.direction = Some(old_direction.flip());
        self.include_cursor = !self.include_cursor;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Direction {
    #[default]
    Older,
    Newer,
}

impl Direction {
    fn flip(&self) -> Direction {
        match self {
            Direction::Older => Direction::Newer,
            Direction::Newer => Direction::Older,
        }
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    derive_more::Display,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
)]
#[display("{_0}")]
pub struct DeploymentCursor(String);

impl FromStr for DeploymentCursor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DeploymentCursor(s.to_string()))
    }
}

impl DeploymentCursor {
    fn into_grpc_deployment_id(self) -> DeploymentId {
        DeploymentId { id: self.0 }
    }

    fn from_deployment(deployment: &DeploymentSummary) -> Self {
        DeploymentCursor(
            deployment
                .deployment
                .as_ref()
                .expect("`deployment` is sent by the server")
                .deployment_id
                .as_ref()
                .expect("`deployment_id` is sent by the server")
                .id
                .clone(),
        )
    }
}

fn exec_activity_count(summary: &DeploymentSummary) -> Option<u32> {
    summary.component_summary.as_ref().map(|component_summary| {
        component_summary
            .components
            .iter()
            .filter(|component| component.component_type() == DeploymentComponentType::ActivityExec)
            .map(|component| component.count)
            .sum()
    })
}

/// A deployment is "empty" when its component summary reports no components.
fn is_empty_deployment(summary: &DeploymentSummary) -> bool {
    summary
        .component_summary
        .as_ref()
        .is_some_and(|component_summary| {
            component_summary
                .components
                .iter()
                .all(|component| component.count == 0)
        })
}

#[component(DeploymentListPage)]
pub fn deployment_list_page() -> Html {
    let location = use_location().expect("should be called inside a router");
    let navigator = use_navigator().expect("should be called inside a router");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");

    // Get current deployment ID from AppState to trigger refresh on change
    let app_state = use_context::<AppState>().expect("AppState context must be provided");
    let current_deployment_id = app_state.current_deployment_id.clone();

    // Deserialize query from URL or use default
    let query = location.query::<DeploymentQuery>().unwrap_or_default();

    // State to hold the API response
    let response_state = use_state(|| None);

    // Deployment IDs selected for the diff tool.
    let selected_for_diff = use_state(BTreeSet::<String>::new);

    // Effect: Fetch data when the URL query changes or deployment changes
    {
        let query = query.clone();
        let response_state = response_state.clone();
        let notifications = notifications.clone();

        use_effect_with((query, current_deployment_id), move |(query_params, _)| {
            let query_params = query_params.clone();

            spawn_local(async move {
                let mut deployment_client = DeploymentRepositoryClient::new(crate::auth::client());

                let page_size = 20;

                let cursor = query_params
                    .cursor
                    .as_ref()
                    .map(|c| c.clone().into_grpc_deployment_id());

                // Determine pagination based on direction
                let pagination = match query_params.direction.unwrap_or_default() {
                    Direction::Older => Some(Pagination::OlderThan(OlderThan {
                        cursor,
                        length: page_size,
                        including_cursor: query_params.include_cursor,
                    })),
                    Direction::Newer => Some(Pagination::NewerThan(NewerThan {
                        cursor,
                        length: page_size,
                        including_cursor: query_params.include_cursor,
                    })),
                };

                // Send request
                let req = grpc_client::ListDeploymentsRequest {
                    pagination,
                    include_deployment_toml: false,
                    include_derived: query_params.show_derived,
                    include_execution_counts: true,
                    include_component_summary: true,
                };
                debug!("Fetching deployments with query: {req:?}");
                let response = deployment_client.list_deployments(req).await;

                match response {
                    Ok(resp) => response_state.set(Some(resp.into_inner())),
                    Err(e) => {
                        error!("Failed to list deployments: {:?}", e);
                        notifications.push(Notification::error(format!(
                            "Failed to list deployments: {}",
                            e.message()
                        )));
                    }
                }
            })
        });
    }

    let on_toggle_derived = {
        let navigator = navigator.clone();
        let query = query.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let mut new_query = query.clone();
            new_query.show_derived = input.checked();
            let _ = navigator.push_with_query(&Route::DeploymentList, &new_query);
        })
    };

    // Clicked on "Latest" - reset to default query
    let on_latest = {
        let navigator = navigator.clone();
        Callback::from(move |_| {
            let new_query = DeploymentQuery::default();
            let _ = navigator.push_with_query(&Route::DeploymentList, &new_query);
        })
    };

    // Render logic
    if let Some(response) = response_state.deref() {
        let rows = response
            .deployments
            .iter()
            .map(|deployment_summary| {
                let deployment = deployment_summary
                    .deployment
                    .as_ref()
                    .expect("`deployment` is sent");
                let deployment_id = deployment
                    .deployment_id
                    .as_ref()
                    .expect("`deployment_id` is sent")
                    .id
                    .clone();
                let description = deployment
                    .description
                    .as_deref()
                    .filter(|description| !description.trim().is_empty());
                let status_badge = match deployment.status() {
                    DeploymentStatus::Active => {
                        html! { <span class="badge current">{"Current"}</span> }
                    }
                    DeploymentStatus::Enqueued => {
                        html! { <span class="badge enqueued">{"Enqueued"}</span> }
                    }
                    DeploymentStatus::Inactive | DeploymentStatus::Unspecified => html! {},
                };
                let exec_badge = match exec_activity_count(deployment_summary) {
                    Some(0) => html! {},
                    Some(count) => {
                        let activity_label = if count == 1 { "activity" } else { "activities" };
                        html! {
                            <span
                                class="badge dangerous-exec"
                                title={format!(
                                    "This deployment includes {count} exec {activity_label}, which run outside the component sandbox"
                                )}
                            >
                                {"⚠ Exec"}
                            </span>
                        }
                    },
                    None => html! {
                        <span
                            class="badge dangerous-exec"
                            title="Component summary was not returned; exec activity status is unknown"
                        >
                            {"⚠ Exec unknown"}
                        </span>
                    },
                };
                let empty_badge = if is_empty_deployment(deployment_summary) {
                    html! {
                        <span class="badge empty" title="This deployment contains no components">
                            {"empty"}
                        </span>
                    }
                } else {
                    html! {}
                };

                // Cell linking to the execution list filtered by this deployment and status.
                // The links inherit `show_derived` so the list matches the count.
                let show_derived = query.show_derived;
                let count_cell = |count: u32, status: Option<StatusFilterList>| {
                    let query = ExecutionQuery {
                        deployment_id: Some(deployment_id.clone()),
                        status,
                        show_derived,
                        ..Default::default()
                    };
                    html! {
                        <td class="number">
                            if count > 0 {
                                <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={query}>
                                    {count}
                                </Link<Route, ExecutionQuery>>
                            } else {
                                {count}
                            }
                        </td>
                    }
                };
                let execution_summary = deployment_summary.execution_summary.as_ref();
                let locked = execution_summary.map_or(0, |summary| summary.locked);
                let pending = execution_summary.map_or(0, |summary| summary.pending);
                let scheduled = execution_summary.map_or(0, |summary| summary.scheduled);
                let blocked = execution_summary.map_or(0, |summary| summary.blocked);
                let paused = execution_summary.map_or(0, |summary| summary.paused);
                let cancelling = execution_summary.map_or(0, |summary| summary.cancelling);
                let finished_ok = execution_summary.map_or(0, |summary| summary.finished_ok);
                let finished_error =
                    execution_summary.map_or(0, |summary| summary.finished_error);
                let finished_execution_failure =
                    execution_summary.map_or(0, |summary| summary.finished_execution_failure);
                let in_progress = locked + pending + blocked;
                let total = locked
                    + pending
                    + scheduled
                    + blocked
                    + paused
                    + cancelling
                    + finished_ok
                    + finished_error
                    + finished_execution_failure;

                let on_diff_toggle = {
                    let selected_for_diff = selected_for_diff.clone();
                    let deployment_id = deployment_id.clone();
                    Callback::from(move |event: Event| {
                        let input: HtmlInputElement = event.target_unchecked_into();
                        let mut selected = selected_for_diff.deref().clone();
                        if input.checked() {
                            selected.insert(deployment_id.clone());
                        } else {
                            selected.remove(&deployment_id);
                        }
                        selected_for_diff.set(selected);
                    })
                };

                html! {
                    <tr key={deployment_id.clone()}>
                        <td>
                            <input
                                type="checkbox"
                                title="Select for diff"
                                checked={selected_for_diff.contains(&deployment_id)}
                                onchange={on_diff_toggle}
                            />
                        </td>
                        <td>
                            <Link<Route> to={Route::DeploymentDetail {
                                deployment_id: DeploymentId { id: deployment_id.clone() },
                            }}>
                                {&deployment_id}
                            </Link<Route>>
                            {" "}
                            {status_badge}
                            {" "}
                            {exec_badge}
                            {" "}
                            {empty_badge}
                            if let Some(description) = description {
                                <div class="description">{ description }</div>
                            }
                        </td>
                        { count_cell(total, None) }
                        { count_cell(in_progress, Some(StatusFilterList::in_progress())) }
                        { count_cell(
                            scheduled,
                            Some(StatusFilterList::single(StatusFilter::Scheduled)),
                        ) }
                        { count_cell(
                            paused,
                            Some(StatusFilterList::single(StatusFilter::Paused)),
                        ) }
                        { count_cell(
                            cancelling,
                            Some(StatusFilterList::single(StatusFilter::Cancelling)),
                        ) }
                        { count_cell(
                            finished_ok,
                            Some(StatusFilterList::single(StatusFilter::FinishedOk)),
                        ) }
                        { count_cell(
                            finished_error,
                            Some(StatusFilterList::single(StatusFilter::FinishedError)),
                        ) }
                        { count_cell(
                            finished_execution_failure,
                            Some(StatusFilterList::single(StatusFilter::FinishedExecutionFailure)),
                        ) }
                    </tr>
                }
            })
            .collect::<Vec<_>>();

        // Deployment IDs are `Dep_<ULID>` so the lexicographic order matches creation order;
        // diff from the older to the newer deployment.
        let diff_route = {
            let mut selected = selected_for_diff.iter();
            match (selected.next(), selected.next(), selected.next()) {
                (Some(older), Some(newer), None) => Some(Route::DeploymentDiff {
                    from: DeploymentId { id: older.clone() },
                    to: DeploymentId { id: newer.clone() },
                }),
                _ => None,
            }
        };
        let navigator_for_diff = navigator.clone();

        // Calculate cursors for pagination
        let newer_page_query = if let Some(deployment) = response.deployments.first() {
            let mut query = query.clone();
            query.cursor = Some(DeploymentCursor::from_deployment(deployment));
            query.direction = Some(Direction::Newer);
            query.include_cursor = false;
            Some(query)
        } else if let Some(direction) = query.direction
            && direction == Direction::Older
            && !query.include_cursor
            && query.cursor.is_some()
        {
            Some(query.clone().flip(direction))
        } else {
            None
        };

        let older_page_query = if let Some(deployment) = response.deployments.last() {
            let mut query = query.clone();
            query.cursor = Some(DeploymentCursor::from_deployment(deployment));
            query.direction = Some(Direction::Older);
            query.include_cursor = false;
            Some(query)
        } else if let Some(direction) = query.direction
            && direction == Direction::Newer
            && !query.include_cursor
            && query.cursor.is_some()
        {
            Some(query.clone().flip(direction))
        } else {
            None
        };

        let on_page_change = {
            let navigator = navigator.clone();
            Callback::from(move |query: DeploymentQuery| {
                let _ = navigator.push_with_query(&Route::DeploymentList, &query);
            })
        };

        html! {
            <>
                <h3>{"Deployments"}</h3>

                <div class="executions-filter">
                    <div class="checkboxes">
                        <label>
                            <input
                                type="checkbox"
                                checked={query.show_derived}
                                onchange={on_toggle_derived}
                            />
                            {" Show Derived Executions"}
                        </label>
                    </div>
                </div>

                <table class="deployment_list">
                    <thead>
                        <tr>
                            <th title="Select two deployments to compare">{"Diff"}</th>
                            <th>{"Deployment ID"}</th>
                            <th class="number" title="All executions of this deployment">{"Executions"}</th>
                            <th class="number" title="Locked, pending or blocked">{"In progress"}</th>
                            <th class="number" title="Pending with the scheduled time in the future">{"Scheduled"}</th>
                            <th class="number" title="Paused, regardless of the underlying state">{"Paused"}</th>
                            <th class="number" title="Cancellation requested; teardown in progress">{"Cancelling"}</th>
                            <th class="number" title="Finished successfully">{"OK"}</th>
                            <th class="number" title="Finished with the err variant of the result type">{"Errors"}</th>
                            <th class="number" title="Execution failures: traps, timeouts, nondeterminism, cancellations">{"Failures"}</th>
                        </tr>
                    </thead>
                    <tbody>
                        { rows }
                    </tbody>
                </table>

                <div class="pagination">
                    if let Some(diff_route) = diff_route {
                        <button onclick={move |_| navigator_for_diff.push(&diff_route)}>
                            {"Compare selected"}
                        </button>
                    } else {
                        <button disabled={true} title="Select exactly two deployments">
                            {"Compare selected"}
                        </button>
                    }
                    <button onclick={&on_latest}>
                        {"<< Latest"}
                    </button>

                    if let Some(query) = newer_page_query {
                        <button onclick={
                            let on_page_change = on_page_change.clone();
                            move |_| on_page_change.emit(query.clone())
                        }>
                            {"< Newer"}
                        </button>
                    } else {
                        <button disabled={true}>
                            {"< Newer"}
                        </button>
                    }

                    if let Some(query) = older_page_query {
                        <button onclick={
                            let on_page_change = on_page_change.clone();
                            move |_| on_page_change.emit(query.clone())
                        }>
                            {"Older >"}
                        </button>
                    } else {
                        <button disabled={true}>
                            {"Older >"}
                        </button>
                    }
                </div>
            </>
        }
    } else {
        html! { <p>{"Loading..."}</p> }
    }
}
