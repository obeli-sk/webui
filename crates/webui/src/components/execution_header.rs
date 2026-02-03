use crate::app::Route;
use crate::components::execution_actions::{PauseButton, ReplayButton, UnpauseButton, UpgradeForm};
use crate::components::execution_list_page::ExecutionQuery;
use crate::components::execution_status::ExecutionStatus;
use crate::grpc::grpc_client::{ComponentType, ContentDigest, ExecutionId, ExecutionSummary};
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Clone, PartialEq, Default)]
struct ExecutionInfo {
    component_type: ComponentType,
    component_digest: ContentDigest,
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
    let exec_info = use_state(|| None::<ExecutionInfo>);

    // Callback to receive the summary from ExecutionStatus
    let on_summary = {
        let exec_info = exec_info.clone();
        Callback::from(move |summary: ExecutionSummary| {
            exec_info.set(Some(ExecutionInfo {
                component_type: summary.component_type(),
                component_digest: summary.component_digest.unwrap(),
            }));
        })
    };

    let workflow_digest = exec_info.as_ref().and_then(|exec_info| {
        if exec_info.component_type == ComponentType::Workflow {
            Some(exec_info.component_digest.clone())
        } else {
            None
        }
    });

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

            <ExecutionStatus execution_id={execution_id.clone()} status={None} print_finished_status={true} on_summary={on_summary} />

            if let Some(workflow_digest) = workflow_digest {
                <div class="execution-actions">
                    <PauseButton
                        execution_id={execution_id.clone()}
                    />
                    <UnpauseButton
                        execution_id={execution_id.clone()}
                    />
                    <ReplayButton
                        execution_id={execution_id.clone()}
                    />
                    <UpgradeForm
                        execution_id={execution_id.clone()}
                        current_digest={workflow_digest}
                    />
                </div>
            }
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
