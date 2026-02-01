use crate::BASE_URL;
use crate::app::Route;
use crate::components::execution_actions::{ReplayButton, UpgradeForm};
use crate::components::execution_list_page::ExecutionQuery;
use crate::components::execution_status::ExecutionStatus;
use crate::grpc::grpc_client::{
    self, ComponentType, ContentDigest, ExecutionId,
    execution_repository_client::ExecutionRepositoryClient,
};
use log::error;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Clone, PartialEq, Default)]
struct ExecutionInfo {
    component_type: Option<ComponentType>,
    component_digest: Option<ContentDigest>,
}

#[derive(Properties, PartialEq)]
pub struct ExecutionHeaderProps {
    pub execution_id: ExecutionId,
    pub link: ExecutionLink,
}

#[function_component(ExecutionHeader)]
pub fn execution_header(
    ExecutionHeaderProps { execution_id, link }: &ExecutionHeaderProps,
) -> Html {
    let exec_info = use_state(ExecutionInfo::default);

    // Fetch the Created event to get component type and digest
    {
        let execution_id = execution_id.clone();
        let exec_info = exec_info.clone();

        use_effect_with(execution_id.clone(), move |execution_id| {
            let execution_id = execution_id.clone();
            spawn_local(async move {
                let mut client = ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));

                let result = client
                    .list_execution_events(grpc_client::ListExecutionEventsRequest {
                        execution_id: Some(execution_id.clone()),
                        version_from: 0,
                        length: 1,
                        include_backtrace_id: false,
                    })
                    .await;

                match result {
                    Ok(response) => {
                        let events = response.into_inner().events;
                        if let Some(event) = events.first()
                            && let Some(grpc_client::execution_event::Event::Created(created)) =
                                &event.event
                            && let Some(component_id) = &created.component_id
                        {
                            exec_info.set(ExecutionInfo {
                                component_type: Some(component_id.component_type()),
                                component_digest: component_id.digest.clone(),
                            });
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch execution events: {:?}", e);
                    }
                }
            });
        });
    }

    let is_workflow = exec_info
        .component_type
        .is_some_and(|t| t == ComponentType::Workflow);

    html! {
        <div class="execution-header">
            <div class="header-and-links">
                <h3>{ execution_id.render_execution_parts(false, *link) }</h3>

                <div class="execution-links">
                    { ExecutionLink::Trace.link(execution_id.clone(), "Trace") }
                    { ExecutionLink::ExecutionLog.link(execution_id.clone(), "Execution Log") }
                    { ExecutionLink::Debug.link(execution_id.clone(), "Debugger") }
                    { ExecutionLink::Logs.link(execution_id.clone(), "Logs") }
                    <Link<Route, ExecutionQuery>
                        to={Route::ExecutionList}
                        query={ExecutionQuery { execution_id_prefix: Some(execution_id.to_string()), show_derived: true, ..Default::default() }}
                    >
                        {"Child executions"}
                    </Link<Route, ExecutionQuery>>
                </div>
            </div>

            <ExecutionStatus execution_id={execution_id.clone()} status={None} print_finished_status={true} />

            <div class="execution-actions">
                <ReplayButton
                    execution_id={execution_id.clone()}
                    is_workflow={is_workflow}
                />
                <UpgradeForm
                    execution_id={execution_id.clone()}
                    current_digest={exec_info.component_digest.clone()}
                    is_workflow={is_workflow}
                />
            </div>
        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionLink {
    Trace,
    ExecutionLog,
    Debug,
    Logs,
}

impl ExecutionLink {
    pub fn link(self, execution_id: ExecutionId, title: &str) -> Html {
        match self {
            ExecutionLink::Trace => html! {
                <Link<Route> to={Route::ExecutionTrace { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::ExecutionLog => html! {
                <Link<Route> to={Route::ExecutionLog { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Debug => html! {
                <Link<Route> to={Route::ExecutionDebugger { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Logs => html! {
                <Link<Route> to={Route::Logs { execution_id }}>
                    {title}
                </Link<Route>>
            },
        }
    }
}
