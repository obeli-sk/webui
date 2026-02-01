use crate::{
    BASE_URL,
    app::Route,
    grpc::grpc_client::{
        self, DeploymentId, DeploymentState,
        deployment_repository_client::DeploymentRepositoryClient,
        list_deployment_states_request::{NewerThan, OlderThan, Pagination},
    },
};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{ops::Deref, str::FromStr};
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
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

    fn from_deployment(deployment: &DeploymentState) -> Self {
        DeploymentCursor(
            deployment
                .deployment_id
                .as_ref()
                .expect("`deployment_id` is sent by the server")
                .id
                .clone(),
        )
    }
}

#[function_component(DeploymentListPage)]
pub fn deployment_list_page() -> Html {
    let location = use_location().expect("should be called inside a router");
    let navigator = use_navigator().expect("should be called inside a router");

    // Deserialize query from URL or use default
    let query = location.query::<DeploymentQuery>().unwrap_or_default();

    // State to hold the API response
    let response_state = use_state(|| None);

    let refresh_counter_state = use_state(|| 0); // Force calling use_effect

    // Effect: Fetch data when the URL query changes
    {
        let query = query.clone();
        let response_state = response_state.clone();
        let refresh_counter_state = refresh_counter_state.clone();

        use_effect_with((query, *refresh_counter_state), move |(query_params, _)| {
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
                let req = grpc_client::ListDeploymentStatesRequest { pagination };
                debug!("Fetching deployments with query: {req:?}");
                let response = deployment_client.list_deployment_states(req).await;

                match response {
                    Ok(resp) => response_state.set(Some(resp.into_inner())),
                    Err(e) => log::error!("Failed to list deployments: {:?}", e),
                }
            })
        });
    }

    // Clicked on "Refresh"
    let on_refresh = {
        let navigator = navigator.clone();
        let refresh_counter_state = refresh_counter_state.clone();
        Callback::from(move |_| {
            let new_query = DeploymentQuery::default();
            refresh_counter_state.set(*refresh_counter_state + 1);
            let _ = navigator.push_with_query(&Route::DeploymentList, &new_query);
        })
    };

    // Render logic
    if let Some(response) = response_state.deref() {
        let rows = response
            .deployments
            .iter()
            .map(|deployment| {
                let deployment_id = deployment
                    .deployment_id
                    .as_ref()
                    .expect("`deployment_id` is sent")
                    .id
                    .clone();
                let current_badge = if deployment.current {
                    html! { <span class="badge current">{"Current"}</span> }
                } else {
                    html! {}
                };

                // Link to execution list filtered by this deployment
                let execution_link_query = crate::components::execution_list_page::ExecutionQuery {
                    deployment_id: Some(deployment_id.clone()),
                    ..Default::default()
                };

                html! {
                    <tr key={deployment_id.clone()}>
                        <td>
                            <Link<Route, crate::components::execution_list_page::ExecutionQuery>
                                to={Route::ExecutionList}
                                query={execution_link_query}
                            >
                                {&deployment_id}
                            </Link<Route, crate::components::execution_list_page::ExecutionQuery>>
                            {" "}
                            {current_badge}
                        </td>
                        <td class="number">{deployment.locked}</td>
                        <td class="number">{deployment.pending}</td>
                        <td class="number">{deployment.scheduled}</td>
                        <td class="number">{deployment.blocked}</td>
                        <td class="number">{deployment.finished}</td>
                    </tr>
                }
            })
            .collect::<Vec<_>>();

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

                <div class="deployments-filter">
                    <button onclick={&on_refresh}>{"Refresh"}</button>
                </div>

                <table class="deployment_list">
                    <thead>
                        <tr>
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
                    <button onclick={&on_refresh}>
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
