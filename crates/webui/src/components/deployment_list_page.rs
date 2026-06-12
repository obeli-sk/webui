use crate::{
    BASE_URL,
    app::{AppState, Route},
    components::notification::{Notification, NotificationContext},
    grpc::grpc_client::{
        self, DeploymentId, DeploymentStatus, DeploymentSummary,
        deployment_repository_client::DeploymentRepositoryClient,
        list_deployments_request::{NewerThan, OlderThan, Pagination},
    },
};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{ops::Deref, str::FromStr};
use tonic_web_wasm_client::Client;
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
                let mut deployment_client =
                    DeploymentRepositoryClient::new(Client::new(BASE_URL.to_string()));

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
                    include_config_json: false,
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
                let status_badge = match deployment.status() {
                    DeploymentStatus::Active => {
                        html! { <span class="badge current">{"Current"}</span> }
                    }
                    DeploymentStatus::Enqueued => {
                        html! { <span class="badge enqueued">{"Enqueued"}</span> }
                    }
                    DeploymentStatus::Inactive | DeploymentStatus::Unspecified => html! {},
                };

                // Link to execution list filtered by this deployment
                let execution_link_query = crate::components::execution_list_page::ExecutionQuery {
                    deployment_id: Some(deployment_id.clone()),
                    ..Default::default()
                };

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
                            <Link<Route, crate::components::execution_list_page::ExecutionQuery>
                                to={Route::ExecutionList}
                                query={execution_link_query}
                            >
                                {"executions"}
                            </Link<Route, crate::components::execution_list_page::ExecutionQuery>>
                        </td>
                        <td class="number">{deployment_summary.locked}</td>
                        <td class="number">{deployment_summary.pending}</td>
                        <td class="number">{deployment_summary.scheduled}</td>
                        <td class="number">{deployment_summary.blocked}</td>
                        <td class="number">{deployment_summary.finished}</td>
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

                <table class="deployment_list">
                    <thead>
                        <tr>
                            <th title="Select two deployments to compare">{"Diff"}</th>
                            <th>{"Deployment ID"}</th>
                            <th class="number">{"Locked"}</th>
                            <th class="number">{"Pending"}</th>
                            <th class="number">{"Scheduled"}</th>
                            <th class="number">{"Blocked"}</th>
                            <th class="number">{"Finished"}</th>
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
