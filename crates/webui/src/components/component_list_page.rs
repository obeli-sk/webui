use crate::{
    BASE_URL,
    app::{AppState, Route},
    components::{
        code::code_block::CodeBlock,
        component_tree::{ComponentTree, ComponentTreeConfig},
        execution_list_page::ExecutionQuery,
        ffqn_with_links::FfqnWithLinks,
        function_signature::FunctionSignature,
        notification::{Notification, NotificationContext},
    },
    grpc::{
        ffqn::FunctionFqn,
        function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
        grpc_client::{self, ComponentId, FunctionDetail},
        ifc_fqn::IfcFqn,
    },
    util::wit_highlighter,
};
use hashbrown::HashSet;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::rc::Rc;
use yew::prelude::*;
use yew_router::{
    hooks::{use_location, use_navigator},
    prelude::Link,
};

/// Optional query for the component detail page: which deployment the component
/// belongs to. Absent means the active deployment (the default the server uses).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ComponentQuery {
    pub deployment_id: Option<String>,
}

#[derive(Properties, PartialEq)]
pub struct ComponentListPageProps {
    #[prop_or_default]
    pub maybe_component_id: Option<ComponentId>,
}

#[derive(Clone, Copy, PartialEq)]
enum ComponentDetailTab {
    SubmittableFunctions,
    Imports,
    Wit,
}

#[component(ComponentListPage)]
pub fn component_list_page(
    ComponentListPageProps { maybe_component_id }: &ComponentListPageProps,
) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let current_deployment_id = app_state.current_deployment_id.clone();
    let components_by_id = app_state.components_by_id;
    let components_by_exported_ifc = app_state.components_by_exported_ifc;

    let location = use_location().expect("location must be available inside a router");
    let component_query = location.query::<ComponentQuery>().unwrap_or_default();
    let deployment_id = component_query.deployment_id;

    let wit_state = use_state(|| None);
    let wit_loaded = use_state(|| false);
    let selected_tab = use_state(|| ComponentDetailTab::SubmittableFunctions);

    // Resolve the selected component. Prefer the active deployment's already-loaded
    // components; otherwise fetch it from its (possibly historical) deployment.
    let component_state = use_state(|| None::<Rc<grpc_client::Component>>);
    {
        // Fast path: the component belongs to the active deployment.
        let preloaded = maybe_component_id
            .as_ref()
            .and_then(|id| components_by_id.get(id))
            .cloned();
        let component_state = component_state.clone();
        let notifications = notifications.clone();
        use_effect_with(
            (maybe_component_id.clone(), deployment_id.clone()),
            move |(maybe_component_id, deployment_id)| {
                component_state.set(None);
                let Some(component_id) = maybe_component_id.clone() else {
                    return;
                };
                if let Some(found) = preloaded {
                    component_state.set(Some(found));
                    return;
                }
                let deployment_id = deployment_id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut fn_client =
                        grpc_client::function_repository_client::FunctionRepositoryClient::new(
                            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                        );
                    let response = fn_client
                        .list_components(grpc_client::ListComponentsRequest {
                            component_digest: component_id.digest.clone(),
                            deployment_id: deployment_id.map(|id| grpc_client::DeploymentId { id }),
                            extensions: true,
                            ..Default::default()
                        })
                        .await;
                    match response {
                        Ok(resp) => match resp.into_inner().components.into_iter().next() {
                            Some(component) => component_state.set(Some(Rc::new(component))),
                            None => notifications.push(Notification::error(
                                "Component not found in this deployment".to_string(),
                            )),
                        },
                        Err(e) => {
                            error!("Failed to fetch component: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to fetch component: {}",
                                e.message()
                            )));
                        }
                    }
                });
            },
        );
    }

    // Fetch the WIT once the component is resolved.
    use_effect_with(((*component_state).clone(), deployment_id.clone()), {
        let wit_state = wit_state.clone();
        let wit_loaded = wit_loaded.clone();
        let selected_tab = selected_tab.clone();
        let notifications = notifications.clone();
        move |(component, deployment_id)| {
            selected_tab.set(ComponentDetailTab::SubmittableFunctions);
            wit_state.set(None);
            wit_loaded.set(false);
            let Some(component) = component.clone() else {
                wit_loaded.set(true);
                return;
            };
            let component_digest = component
                .component_id
                .as_ref()
                .expect("`component_id` is sent")
                .digest
                .clone()
                .expect("`digest` is sent");
            let render_ffqn_with_links = component
                .exports
                .iter()
                .filter(|fn_detail| fn_detail.submittable)
                .map(|fn_detail| {
                    FunctionFqn::from_fn_detail(fn_detail).expect("fn_detail must be parseable")
                })
                .collect::<HashSet<_>>();
            let deployment_id = deployment_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut fn_client =
                    grpc_client::function_repository_client::FunctionRepositoryClient::new(
                        tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                    );
                let response = fn_client
                    .get_wit(grpc_client::GetWitRequest {
                        component_digest: Some(component_digest),
                        deployment_id: deployment_id.map(|id| grpc_client::DeploymentId { id }),
                    })
                    .await;
                match response {
                    Ok(resp) => {
                        if let Some(wit) = resp.into_inner().content {
                            let wit = wit_highlighter::print_all(&wit, render_ffqn_with_links)
                                .inspect_err(|err| warn!("Cannot render WIT - {err:?}"))
                                .ok();
                            wit_state.set(wit);
                        } // else - no WIT is associated with the component.
                        wit_loaded.set(true);
                    }
                    Err(e) => {
                        error!("Failed to get WIT: {:?}", e);
                        notifications.push(Notification::error(format!(
                            "Failed to get WIT: {}",
                            e.message()
                        )));
                        wit_loaded.set(true);
                    }
                }
            });
        }
    });

    let component_detail = component_state
        .as_ref()
        .map(|component| {
            let exports =
                map_interfaces_to_fn_details(&component.exports, InterfaceFilter::All);

            let render_exported_ifc_with_fns = |ifc_fqn: &IfcFqn, fn_details: &[FunctionDetail] | {
                let submittable_fn_details = fn_details
                    .iter()
                    .filter(|fn_detail| fn_detail.submittable)
                    .map(|fn_detail| {
                        let ffqn = FunctionFqn::from_fn_detail(fn_detail).expect("ffqn should be parseable");
                        html! {
                            <li>
                                <FfqnWithLinks {ffqn} />
                                {": "}
                                <span>
                                    <FunctionSignature params = {fn_detail.params.clone()} return_type={fn_detail.return_type.clone()} />
                                </span>
                            </li>
                        }
                    })
                    .collect::<Vec<_>>();

                html! {
                    <section class="types-interface">
                        <h4>
                            // show searchable interface link
                            <Link<Route, ExecutionQuery>
                                to={Route::ExecutionList}
                                query={ExecutionQuery { ffqn_prefix: Some(ifc_fqn.to_string()), show_derived: true, ..Default::default() }}
                            >
                                {ifc_fqn.to_string()}
                            </Link<Route, ExecutionQuery>>
                        </h4>
                        <ul>
                            {submittable_fn_details}
                        </ul>
                    </section>
                }
            };

            let submittable_ifcs_fns = exports
                .iter()
                .filter(|(_, fn_details)| fn_details.iter().any(|f_d| f_d.submittable))
                .map(|(ifc_fqn, fn_details)| render_exported_ifc_with_fns(ifc_fqn, fn_details))
                .collect::<Vec<_>>();
            let submittable_functions = if submittable_ifcs_fns.is_empty() {
                html! {
                    <p class="component-empty-state">
                        {"This component does not expose functions that can be submitted directly."}
                    </p>
                }
            } else {
                html! { <>{ for submittable_ifcs_fns }</> }
            };

            // imports:
            let imports =
                map_interfaces_to_fn_details(&component.imports, InterfaceFilter::All);
            let imports: Vec<_> = imports.keys().map(|ifc| html!{ <>
                <h4>{ifc.to_string()}
                if let Some(found) = components_by_exported_ifc.get(ifc) {
                    {" "}
                    <Link<Route> to={Route::Component { component_id: found.component_id.clone().expect("`component_id` is sent") } }>
                        { found.as_type().as_icon_html() }
                        {" "}
                        {&found.component_id.as_ref().expect("`component_id` is sent").name}
                    </Link<Route>>
                }
                </h4>
            </>}).collect();
            let imported_interfaces = if imports.is_empty() {
                html! { <p class="component-empty-state">{"No imported interfaces."}</p> }
            } else {
                html! { <>{ for imports }</> }
            };

            let component_name = &component
                .component_id
                .as_ref()
                .expect("`component_id` is sent")
                .name;
            let breadcrumb = current_deployment_id.as_ref().map_or_else(
                || html! {
                    <Link<Route> to={Route::ComponentList}>{"Components"}</Link<Route>>
                },
                |deployment_id| html! {<>
                    <Link<Route> to={Route::DeploymentList}>{"Deployments"}</Link<Route>>
                    <span class="breadcrumb-separator">{"/"}</span>
                    <Link<Route> to={Route::DeploymentDetail {
                        deployment_id: deployment_id.clone(),
                    }}>
                        {"Active deployment"}
                    </Link<Route>>
                </>},
            );
            let tab_button = |label: &'static str, tab: ComponentDetailTab| {
                let selected_tab = selected_tab.clone();
                html! {
                    <button
                        class={classes!((*selected_tab == tab).then_some("active"))}
                        onclick={Callback::from(move |_| selected_tab.set(tab))}
                    >
                        {label}
                    </button>
                }
            };
            let tab_content = match *selected_tab {
                ComponentDetailTab::SubmittableFunctions => submittable_functions,
                ComponentDetailTab::Imports => html! {<>
                    <p class="component-section-help">
                        {"Dependencies this component expects the deployment to provide."}
                    </p>
                    {imported_interfaces}
                </>},
                ComponentDetailTab::Wit => {
                    if let Some(wit) = wit_state.deref() {
                        html! { <CodeBlock source={wit.clone()} /> }
                    } else if *wit_loaded {
                        html! {
                            <p class="component-empty-state">
                                {"No WIT definition is available for this component."}
                            </p>
                        }
                    } else {
                        html! { <p class="component-empty-state">{"Loading WIT..."}</p> }
                    }
                }
            };

            html! { <>
                <header class="component-detail-header">
                    <p class="breadcrumbs">{breadcrumb}</p>
                    <h1>
                        {component_name}
                        <span class="component-type-label">
                            { component.as_type().as_icon_html() }
                            {component.as_type().as_label()}
                        </span>
                    </h1>
                    <p class="component-intro">
                        {"Inspect the functions and interfaces exposed or required by this component."}
                    </p>
                </header>

                <div class="view-tabs component-detail-tabs">
                    {tab_button("Submittable functions", ComponentDetailTab::SubmittableFunctions)}
                    {tab_button("Imports", ComponentDetailTab::Imports)}
                    {tab_button("WIT", ComponentDetailTab::Wit)}
                </div>

                <section class="component-detail-tab-content">
                    {tab_content}
                </section>
            </>}
        });

    let navigator = use_navigator().unwrap();
    let on_component_selected =
        Callback::from(move |component_id| navigator.push(&Route::Component { component_id }));

    if let Some(component_detail) = component_detail {
        component_detail
    } else if maybe_component_id.is_some() {
        // A component is selected but not yet resolved (fetching from its deployment).
        html! { <p class="component-empty-state">{"Loading component..."}</p> }
    } else {
        html! {<>
            <header>
                <h1>{"Components"}</h1>
                <p class="component-intro">
                    {"Browse the interfaces currently available to this Obelisk server. Components are normally accessed from the active deployment."}
                </p>
                if let Some(deployment_id) = &current_deployment_id {
                    <p>
                        <Link<Route> to={Route::DeploymentDetail {
                            deployment_id: deployment_id.clone(),
                        }}>
                            {"View active deployment"}
                        </Link<Route>>
                    </p>
                }
            </header>

            <section class="component-selection">
                <ComponentTree config={ComponentTreeConfig::ComponentsOnly {
                    on_component_selected
                }
                } />
            </section>
        </>}
    }
}
