use crate::app::Route;
use crate::components::execution_actions::{
    CancelActivityButton, PauseButton, ReplayButton, UnpauseButton, UpgradeForm,
};
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
    let is_finished = use_state(|| false);

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

    // Callback when execution finishes
    let on_finished = {
        let is_finished = is_finished.clone();
        Callback::from(move |()| {
            is_finished.set(true);
        })
    };

    let workflow_digest = exec_info.as_ref().and_then(|exec_info| {
        if exec_info.component_type == ComponentType::Workflow {
            Some(exec_info.component_digest.clone())
        } else {
            None
        }
    });

    let is_activity = exec_info.as_ref().is_some_and(|exec_info| {
        matches!(
            exec_info.component_type,
            ComponentType::ActivityWasm
                | ComponentType::ActivityExternal
                | ComponentType::ActivityStub
        )
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

            <ExecutionStatus execution_id={execution_id.clone()} status={None} print_finished_status={true} on_summary={on_summary} on_finished={on_finished} />

            if let Some(workflow_digest) = workflow_digest {
                <div class="execution-actions">
                    if !*is_finished {
                        <PauseButton
                            execution_id={execution_id.clone()}
                        />
                        <UnpauseButton
                            execution_id={execution_id.clone()}
                        />
                        <UpgradeForm
                            execution_id={execution_id.clone()}
                            current_digest={workflow_digest}
                        />
                    }
                    <ReplayButton
                        execution_id={execution_id.clone()}
                    />
                </div>
            }

            if is_activity && !*is_finished {
                <div class="execution-actions">
                    <CancelActivityButton
                        execution_id={execution_id.clone()}
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
