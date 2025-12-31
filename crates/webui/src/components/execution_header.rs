use crate::app::Route;
use crate::components::execution_list_page::ExecutionQuery;
use crate::components::execution_status::ExecutionStatus;
use crate::grpc::execution_id::ExecutionIdExt;
use crate::grpc::grpc_client::ExecutionId;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq)]
pub struct ExecutionHeaderProps {
    pub execution_id: ExecutionId,
    pub link: ExecutionLink,
}
#[function_component(ExecutionHeader)]
pub fn execution_header(
    ExecutionHeaderProps { execution_id, link }: &ExecutionHeaderProps,
) -> Html {
    html! {
        <div class="execution-header">
            <div class="header-and-links">
                <h3>{ execution_id.render_execution_parts(false, *link) }</h3>

                <div class="execution-links">
                    <Link<Route, ExecutionQuery>
                        to={Route::ExecutionList}
                        query={ExecutionQuery { execution_id_prefix: Some(execution_id.to_string()), show_derived: true, ..Default::default() }}
                    >
                        {"Child executions"}
                    </Link<Route, ExecutionQuery>>

                    { ExecutionLink::Trace.link(execution_id.clone(), "Trace") }
                    { ExecutionLink::Log.link(execution_id.clone(), "Log") }
                    { ExecutionLink::Debug.link(execution_id.clone(), "Debug") }
                </div>
            </div>

            <ExecutionStatus execution_id={execution_id.clone()} status={None} print_finished_status={true} />

        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionLink {
    Trace,
    Log,
    Debug,
}

impl ExecutionLink {
    pub fn link(&self, execution_id: ExecutionId, title: &str) -> Html {
        match self {
            ExecutionLink::Trace => html! {
                <Link<Route> to={Route::ExecutionTrace { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Log => html! {
                <Link<Route> to={Route::ExecutionLog { execution_id }}>
                    {title}
                </Link<Route>>
            },
            ExecutionLink::Debug => html! {
                <Link<Route> to={Route::ExecutionDebugger { execution_id }}>
                    {title}
                </Link<Route>>
            },
        }
    }
}
