use crate::{
    app::query::BacktraceVersionsPath,
    components::{
        component_list_page::ComponentListPage, debugger::debugger_view::DebuggerView,
        deployment_list_page::DeploymentListPage, execution_detail_page::ExecutionLogPage,
        execution_list_page::ExecutionListPage, execution_logs_page::LogsPage,
        execution_stub_submit_page::ExecutionStubResultPage,
        execution_submit_page::ExecutionSubmitPage, not_found::NotFound,
        trace::trace_view::TraceView,
    },
    grpc::{
        ffqn::FunctionFqn,
        grpc_client::{self, ComponentId, DeploymentId, ExecutionId},
        ifc_fqn::IfcFqn,
        version::VersionType,
    },
    loader::{LoadedComponents, get_current_deployment_id, load_components},
};
use gloo::timers::callback::Interval;
use hashbrown::HashMap;
use log::{debug, error};
use std::{fmt::Display, ops::Deref, rc::Rc, str::FromStr};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

/// Interval in milliseconds between deployment ID checks.
const DEPLOYMENT_POLL_INTERVAL_MS: u32 = 5000;

#[derive(Clone, PartialEq)]
pub struct AppState {
    pub components_by_id: HashMap<ComponentId, Rc<grpc_client::Component>>,
    pub components_by_exported_ifc: HashMap<IfcFqn, Rc<grpc_client::Component>>,
    pub ffqns_to_details:
        hashbrown::HashMap<FunctionFqn, (grpc_client::FunctionDetail, grpc_client::ComponentId)>,
    pub current_deployment_id: Option<DeploymentId>,
}

impl AppState {
    /// Creates a new AppState from loaded components.
    pub fn from_loaded(loaded: &LoadedComponents, deployment_id: Option<DeploymentId>) -> Self {
        let mut ffqns_to_details = hashbrown::HashMap::new();
        for (component_id, component) in &loaded.components_by_id {
            for exported_fn_detail in component.exports.iter() {
                let ffqn = FunctionFqn::from_fn_detail(exported_fn_detail)
                    .expect("ffqn should be parseable");
                ffqns_to_details.insert(ffqn, (exported_fn_detail.clone(), component_id.clone()));
            }
        }
        Self {
            components_by_id: loaded.components_by_id.clone(),
            components_by_exported_ifc: loaded.components_by_exported_ifc.clone(),
            ffqns_to_details,
            current_deployment_id: deployment_id,
        }
    }
}

pub mod query {
    use super::*;

    #[derive(Clone, PartialEq)]
    pub struct BacktraceVersionsPath(pub Vec<VersionType>);
    const BACKTRACE_VERSIONS_SEPARATOR: char = '_';
    impl Display for BacktraceVersionsPath {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for (idx, version) in self.0.iter().enumerate() {
                if idx == 0 {
                    write!(f, "{version}")?;
                } else {
                    write!(f, "{BACKTRACE_VERSIONS_SEPARATOR}{version}")?;
                }
            }
            Ok(())
        }
    }

    impl FromStr for BacktraceVersionsPath {
        type Err = ();

        fn from_str(input: &str) -> Result<Self, Self::Err> {
            let mut versions = Vec::new();
            for split in input.split(BACKTRACE_VERSIONS_SEPARATOR) {
                let version: VersionType = split.parse().map_err(|_| ())?;
                versions.push(version);
            }
            Ok(BacktraceVersionsPath(versions))
        }
    }
    impl From<VersionType> for BacktraceVersionsPath {
        fn from(value: VersionType) -> Self {
            BacktraceVersionsPath(vec![value])
        }
    }
    impl BacktraceVersionsPath {
        pub fn last(&self) -> VersionType {
            *self.0.last().expect("must contain at least one element")
        }
        pub fn step_into(&self) -> BacktraceVersionsPath {
            let mut ret = self.clone();
            ret.0.push(0);
            ret
        }
        pub fn change(&self, version: VersionType) -> BacktraceVersionsPath {
            let mut ret = self.clone();
            *ret.0.last_mut().expect("must contain at least one element") = version;
            ret
        }
        pub fn step_out(&self) -> Option<BacktraceVersionsPath> {
            let mut ret = self.clone();
            ret.0.pop();
            if ret.0.is_empty() { None } else { Some(ret) }
        }
    }
    impl Default for BacktraceVersionsPath {
        fn default() -> Self {
            BacktraceVersionsPath(vec![0])
        }
    }
}

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/components")]
    ComponentList,
    #[at("/component/:component_id")]
    Component {
        component_id: grpc_client::ComponentId,
    },
    #[at("/deployments")]
    DeploymentList,
    #[at("/execution/submit/:ffqn")]
    ExecutionSubmit { ffqn: FunctionFqn },
    #[at("/execution/stub/:ffqn/:execution_id")]
    ExecutionStubResult {
        ffqn: FunctionFqn,
        execution_id: ExecutionId,
    },
    #[at("/execution/list")]
    ExecutionList,
    #[at("/execution/:execution_id")]
    ExecutionLog {
        execution_id: grpc_client::ExecutionId,
    },
    #[at("/execution/:execution_id/trace")]
    ExecutionTrace {
        execution_id: grpc_client::ExecutionId,
    },
    #[at("/execution/:execution_id/debug")]
    ExecutionDebugger {
        execution_id: grpc_client::ExecutionId,
    },
    #[at("/execution/:execution_id/debug/:versions")]
    ExecutionDebuggerWithVersions {
        execution_id: grpc_client::ExecutionId,
        versions: BacktraceVersionsPath,
    },
    #[at("/execution/:execution_id/logs")]
    Logs {
        execution_id: grpc_client::ExecutionId,
    },
    #[not_found]
    #[at("/404")]
    NotFound,
}

impl Route {
    pub fn render(route: Route) -> Html {
        match route {
            Route::Home | Route::ExecutionList => html! { <ExecutionListPage /> },
            Route::ComponentList => html! { <ComponentListPage /> },
            Route::Component { component_id } => {
                html! { <ComponentListPage maybe_component_id={Some(component_id)}/> }
            }
            Route::DeploymentList => html! { <DeploymentListPage /> },
            Route::ExecutionSubmit { ffqn } => html! { <ExecutionSubmitPage {ffqn} /> },
            Route::ExecutionStubResult { ffqn, execution_id } => {
                html! { <ExecutionStubResultPage {ffqn}  {execution_id} /> }
            }
            Route::ExecutionLog { execution_id } => {
                html! { <ExecutionLogPage {execution_id} /> }
            }
            Route::ExecutionTrace { execution_id } => {
                html! { <TraceView {execution_id} /> }
            }
            Route::ExecutionDebugger { execution_id } => {
                html! { <DebuggerView {execution_id} versions={BacktraceVersionsPath::from(0)} /> }
            }
            Route::ExecutionDebuggerWithVersions {
                execution_id,
                versions,
            } => {
                html! { <DebuggerView {execution_id} versions={versions} /> }
            }
            Route::Logs { execution_id } => {
                html! { <LogsPage {execution_id} />}
            }
            Route::NotFound => html! { <NotFound /> },
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct AppProps {
    pub initial_components: LoadedComponents,
}

#[function_component(App)]
pub fn app(AppProps { initial_components }: &AppProps) -> Html {
    let app_state = use_state(|| AppState::from_loaded(initial_components, None));

    // Poll for deployment changes
    {
        let app_state = app_state.clone();
        use_effect_with((), move |()| {
            let interval = Interval::new(DEPLOYMENT_POLL_INTERVAL_MS, move || {
                let app_state = app_state.clone();
                spawn_local(async move {
                    match get_current_deployment_id().await {
                        Ok(new_deployment_id) => {
                            let current_id = app_state.current_deployment_id.as_ref();
                            if current_id != Some(&new_deployment_id) {
                                debug!(
                                    "Deployment changed from {:?} to {:?}, reloading components",
                                    current_id, new_deployment_id
                                );
                                match load_components().await {
                                    Ok(loaded) => {
                                        app_state.set(AppState::from_loaded(
                                            &loaded,
                                            Some(new_deployment_id),
                                        ));
                                    }
                                    Err(e) => {
                                        error!("Failed to reload components: {:?}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to get current deployment ID: {:?}", e);
                        }
                    }
                });
            });
            // Return cleanup closure that drops the interval
            move || drop(interval)
        });
    }

    html! {
        <ContextProvider<AppState> context={app_state.deref().clone()}>
            <div class="container">
                <BrowserRouter>
                    <nav>
                        <Link<Route> to={Route::DeploymentList }>
                            {"Deployments"}
                        </Link<Route>>
                        {" "}
                        <Link<Route> to={Route::ExecutionList }>
                            {"Executions"}
                        </Link<Route>>
                        {" "}
                        <Link<Route> to={Route::ComponentList }>
                            {"Components"}
                        </Link<Route>>

                    </nav>
                    <Switch<Route> render={Route::render} />
                </BrowserRouter>
            </div>
        </ContextProvider<AppState>>
    }
}
