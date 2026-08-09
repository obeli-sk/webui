use crate::{
    app::{AppState, Route},
    components::{
        code::code_block::CodeBlock,
        component_tree::{ComponentTree, ComponentTreeConfig},
        deployment_config_view::{
            CollapsibleSource, MANIFEST_SECTIONS, SourceContent, SourceMetadata, SourceView,
            build_sections_from_manifest, component_display_name, component_to_toml, toml_block,
        },
        execution_list_page::ExecutionQuery,
        ffqn_with_links::FfqnWithLinks,
        function_signature::FunctionSignature,
        notification::{Notification, NotificationContext},
    },
    grpc::{
        ffqn::FunctionFqn,
        function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
        grpc_client::{self, ComponentFileRole, ComponentId, FunctionDetail},
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
    Routable,
    history::{BrowserHistory, History},
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
    Exports,
    Sources,
    Imports,
    Wit,
    Toml,
}

impl ComponentDetailTab {
    fn from_hash(hash: &str) -> Self {
        match hash.strip_prefix('#').unwrap_or(hash) {
            "sources" => Self::Sources,
            "imports" => Self::Imports,
            "wit" => Self::Wit,
            "toml" => Self::Toml,
            _ => Self::Exports,
        }
    }

    fn fragment(self) -> &'static str {
        match self {
            Self::Exports => "exports",
            Self::Sources => "sources",
            Self::Imports => "imports",
            Self::Wit => "wit",
            Self::Toml => "toml",
        }
    }
}

#[derive(Clone, PartialEq)]
struct ComponentDeploymentConfig {
    toml: String,
    sources: Vec<SourceView>,
}

fn component_file_sources(files: &[grpc_client::ComponentFileRef]) -> Vec<SourceView> {
    let mut sources = files
        .iter()
        .map(|file_ref| {
            let file = file_ref.file.as_ref().expect("`file` is sent");
            let role = ComponentFileRole::try_from(file_ref.role)
                .unwrap_or(ComponentFileRole::Unspecified);
            SourceView {
                file_name: file.path.clone(),
                content: SourceContent::FetchFile {
                    digest: file.digest.clone(),
                },
                metadata: Some(SourceMetadata {
                    role: component_file_role_label(role),
                }),
            }
        })
        .collect::<Vec<_>>();
    sources.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    sources
}

fn component_file_role_label(role: ComponentFileRole) -> &'static str {
    match role {
        ComponentFileRole::WasmComponent => "WASM component",
        ComponentFileRole::ExecProgram => "exec program",
        ComponentFileRole::JsEntrypoint => "JS entrypoint",
        ComponentFileRole::JsModule => "JS module",
        ComponentFileRole::BacktraceSource => "backtrace source",
        ComponentFileRole::Unspecified => "unspecified",
    }
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
    let is_active_deployment = current_deployment_id.as_ref().is_some_and(|current| {
        deployment_id.as_deref().map_or_else(
            || {
                maybe_component_id
                    .as_ref()
                    .is_some_and(|component_id| components_by_id.contains_key(component_id))
            },
            |deployment_id| deployment_id == current.id,
        )
    });

    let wit_state = use_state(|| None);
    let wit_loaded = use_state(|| false);
    let selected_tab = ComponentDetailTab::from_hash(location.hash());
    let deployment_config = use_state(|| None::<Result<Option<ComponentDeploymentConfig>, String>>);

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
                            crate::auth::client(),
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

    // Fetch raw WIT only when its tab is selected.
    use_effect_with(
        (
            (*component_state).clone(),
            deployment_id.clone(),
            is_active_deployment,
            selected_tab,
        ),
        {
            let wit_state = wit_state.clone();
            let wit_loaded = wit_loaded.clone();
            let notifications = notifications.clone();
            move |(component, deployment_id, is_active_deployment, selected_tab)| {
                wit_state.set(None);
                wit_loaded.set(false);
                if *selected_tab != ComponentDetailTab::Wit {
                    return;
                }
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
                    .filter(|fn_detail| *is_active_deployment && fn_detail.submittable)
                    .map(|fn_detail| {
                        FunctionFqn::from_fn_detail(fn_detail).expect("fn_detail must be parseable")
                    })
                    .collect::<HashSet<_>>();
                let deployment_id = deployment_id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut fn_client =
                        grpc_client::function_repository_client::FunctionRepositoryClient::new(
                            crate::auth::client(),
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
                                let rendered =
                                    wit_highlighter::print_all(&wit, render_ffqn_with_links)
                                        .unwrap_or_else(|err| {
                                            warn!("Cannot render WIT, showing raw text - {err:?}");
                                            wit_highlighter::print_raw(&wit)
                                        });
                                wit_state.set(Some(rendered));
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
        },
    );

    use_effect_with(
        (
            (*component_state).clone(),
            deployment_id.clone(),
            current_deployment_id.clone(),
            selected_tab,
        ),
        {
            let deployment_config = deployment_config.clone();
            let notifications = notifications.clone();
            move |(component, deployment_id, current_deployment_id, selected_tab)| {
                deployment_config.set(None);
                let Some(component) = component.clone() else {
                    deployment_config.set(Some(Ok(None)));
                    return;
                };
                let needs_toml = *selected_tab == ComponentDetailTab::Toml;
                // backcompat: 0.41 deployments lack component file refs; delete TOML source fallback in 0.43.
                let needs_source_fallback =
                    *selected_tab == ComponentDetailTab::Sources && component.files.is_empty();
                if !needs_toml && !needs_source_fallback {
                    return;
                }
                let Some(deployment_id) = deployment_id
                    .as_ref()
                    .map(|id| grpc_client::DeploymentId { id: id.clone() })
                    .or_else(|| current_deployment_id.clone())
                else {
                    deployment_config.set(Some(Ok(None)));
                    return;
                };
                let component_name = component
                    .component_id
                    .as_ref()
                    .expect("`component_id` is sent")
                    .name
                    .clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut client =
                        grpc_client::deployment_repository_client::DeploymentRepositoryClient::new(
                            crate::auth::client(),
                        );
                    let response = client
                        .get_deployment(grpc_client::GetDeploymentRequest {
                            deployment_id: Some(deployment_id),
                            include_generated_metadata: Some(false),
                        })
                        .await;
                    match response {
                        Ok(response) => {
                            let result = response
                                .into_inner()
                                .deployment
                                .and_then(|deployment| deployment.deployment_toml)
                                .map(|manifest| {
                                    let manifest = toml::from_str::<serde_json::Value>(&manifest)
                                        .map_err(|error| error.to_string())?;
                                    let sources = build_sections_from_manifest(&manifest)
                                        .into_iter()
                                        .find_map(|section| {
                                            section
                                                .components
                                                .into_iter()
                                                .find(|component| component.name == component_name)
                                                .map(|component| component.sources)
                                        });
                                    Ok(MANIFEST_SECTIONS.iter().find_map(|(toml_key, _)| {
                                        manifest
                                            .get(toml_key)
                                            .and_then(serde_json::Value::as_array)
                                            .and_then(|components| {
                                                components.iter().find(|component| {
                                                    component_display_name(component)
                                                        == component_name
                                                })
                                            })
                                            .map(|component| ComponentDeploymentConfig {
                                                toml: component_to_toml(toml_key, component),
                                                sources: sources.clone().unwrap_or_default(),
                                            })
                                    }))
                                })
                                .transpose()
                                .map(Option::flatten);
                            deployment_config.set(Some(result));
                        }
                        Err(error) => {
                            error!("Failed to load component configuration: {error:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to load component configuration: {}",
                                error.message()
                            )));
                            deployment_config.set(Some(Err(error.message().to_string())));
                        }
                    }
                });
            }
        },
    );

    let component_detail = component_state
        .as_ref()
        .map(|component| {
            let exports =
                map_interfaces_to_fn_details(&component.exports, InterfaceFilter::All);

            let render_exported_ifc_with_fns = |ifc_fqn: &IfcFqn, fn_details: &[FunctionDetail] | {
                let exported_fn_details = fn_details
                    .iter()
                    .map(|fn_detail| {
                        let ffqn = FunctionFqn::from_fn_detail(fn_detail).expect("ffqn should be parseable");
                        html! {
                            <li>
                                <FfqnWithLinks
                                    {ffqn}
                                    hide_submit={!is_active_deployment || !fn_detail.submittable}
                                />
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
                            {exported_fn_details}
                        </ul>
                    </section>
                }
            };

            let exported_ifcs_fns = exports
                .iter()
                .map(|(ifc_fqn, fn_details)| render_exported_ifc_with_fns(ifc_fqn, fn_details))
                .collect::<Vec<_>>();
            let exported_functions = if exported_ifcs_fns.is_empty() {
                html! {
                    <p class="component-empty-state">
                        {"This component does not export any functions."}
                    </p>
                }
            } else {
                html! { <>{ for exported_ifcs_fns }</> }
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
            let component_id = component.component_id.clone();
            let component_sources = component_file_sources(&component.files);
            // Link back to the deployment this component belongs to (from the query,
            // else the active deployment). "Deployments" already lives in the header nav.
            let component_deployment_id = deployment_id
                .as_ref()
                .map(|id| grpc_client::DeploymentId { id: id.clone() })
                .or_else(|| current_deployment_id.clone());
            let breadcrumb = component_deployment_id.as_ref().map_or_else(
                || html! {
                    <Link<Route> to={Route::ComponentList}>{"Components"}</Link<Route>>
                },
                |deployment_id| {
                    let deployment_url = format!(
                        "{}#components",
                        Route::DeploymentDetail {
                            deployment_id: deployment_id.clone(),
                        }
                        .to_path()
                    );
                    let target_url = deployment_url.clone();
                    html! {
                        <a
                            href={deployment_url}
                            onclick={Callback::from(move |event: MouseEvent| {
                                event.prevent_default();
                                BrowserHistory::new().push(&target_url);
                            })}
                        >
                            {"← "}{ &deployment_id.id }
                        </a>
                    }
                },
            );
            let tab_button = |label: &'static str, tab: ComponentDetailTab| {
                let tab_url = format!(
                    "{}{}#{}",
                    location.path(),
                    location.query_str(),
                    tab.fragment()
                );
                html! {
                    <button
                        class={classes!((selected_tab == tab).then_some("active"))}
                        onclick={Callback::from(move |_| {
                            if selected_tab != tab {
                                BrowserHistory::new().push(&tab_url);
                            }
                        })}
                    >
                        {label}
                    </button>
                }
            };
            let tab_content = match selected_tab {
                ComponentDetailTab::Exports => exported_functions,
                ComponentDetailTab::Sources if !component_sources.is_empty() => html! {
                    <div class="component-sources">
                        { for component_sources.into_iter().map(|source| html! {
                            <CollapsibleSource
                                {source}
                                component_id={component_id.clone()}
                            />
                        }) }
                    </div>
                },
                ComponentDetailTab::Sources => match deployment_config.as_ref() {
                    None => html! { <p class="component-empty-state">{"Loading sources..."}</p> },
                    Some(Ok(Some(config))) if config.sources.is_empty() => html! {
                        <p class="component-empty-state">{"No sources are available for this component."}</p>
                    },
                    Some(Ok(Some(config))) => html! {
                        <div class="component-sources">
                            { for config.sources.iter().map(|source| html! {
                                <CollapsibleSource
                                    source={source.clone()}
                                    component_id={component_id.clone()}
                                />
                            }) }
                        </div>
                    },
                    Some(Ok(None)) => html! {
                        <p class="component-empty-state">
                            {"No deployment sources are available for this component."}
                        </p>
                    },
                    Some(Err(error)) => html! {
                        <p class="error">{format!("Cannot load component sources: {error}")}</p>
                    },
                },
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
                },
                ComponentDetailTab::Toml => match deployment_config.as_ref() {
                    None => html! { <p class="component-empty-state">{"Loading TOML..."}</p> },
                    Some(Ok(Some(config))) => toml_block(config.toml.clone()),
                    Some(Ok(None)) => html! {
                        <p class="component-empty-state">
                            {"No deployment configuration is available for this component."}
                        </p>
                    },
                    Some(Err(error)) => html! {
                        <p class="error">{format!("Cannot load component TOML: {error}")}</p>
                    },
                },
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
                    {tab_button("Exports", ComponentDetailTab::Exports)}
                    {tab_button("Sources", ComponentDetailTab::Sources)}
                    {tab_button("Imports", ComponentDetailTab::Imports)}
                    {tab_button("WIT", ComponentDetailTab::Wit)}
                    {tab_button("TOML", ComponentDetailTab::Toml)}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_files_become_path_sorted_fetchable_sources_with_metadata() {
        let files = vec![
            component_file(
                "src/module.js",
                "sha256:module",
                1234,
                ComponentFileRole::JsModule,
            ),
            component_file(
                "src/entry.js",
                "sha256:entry",
                42,
                ComponentFileRole::JsEntrypoint,
            ),
        ];

        let sources = component_file_sources(&files);

        assert_eq!(sources[0].file_name, "src/entry.js");
        assert!(matches!(
            &sources[0].content,
            SourceContent::FetchFile { digest } if digest == "sha256:entry"
        ));
        let metadata = sources[0].metadata.as_ref().unwrap();
        assert_eq!(metadata.role, "JS entrypoint");
        assert_eq!(sources[1].file_name, "src/module.js");
    }

    fn component_file(
        path: &str,
        digest: &str,
        size: u64,
        role: ComponentFileRole,
    ) -> grpc_client::ComponentFileRef {
        grpc_client::ComponentFileRef {
            file: Some(grpc_client::FileRef {
                path: path.to_string(),
                digest: digest.to_string(),
                size,
            }),
            role: role as i32,
        }
    }
}
