use crate::{
    app::{AppState, Route},
    components::{
        code::code_block::CodeBlock,
        deployment_config_view::{
            CollapsibleSource, MANIFEST_SECTIONS, SourceContent, SourceMetadata, SourceView,
            build_sections_from_manifest, component_display_name, component_to_toml, toml_block,
        },
        deployment_detail_page::DeploymentQuery,
        execution_list_page::ExecutionQuery,
        ffqn_with_links::FfqnWithLinks,
        function_signature::FunctionSignature,
        notification::{Notification, NotificationContext},
    },
    grpc::{
        ffqn::FunctionFqn,
        function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
        grpc_client::{self, ComponentFileRole, DeploymentId, FunctionDetail},
        ifc_fqn::IfcFqn,
    },
    rest,
    util::wit_highlighter,
};
use hashbrown::{HashMap, HashSet};
use log::{error, warn};
use std::ops::Deref;
use std::rc::Rc;
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq)]
pub struct ComponentDetailProps {
    pub component: Rc<grpc_client::Component>,
    pub deployment_id: DeploymentId,
    /// All components of the deployment by name, used to resolve dependencies and callers.
    pub deployment_components: Rc<HashMap<String, grpc_client::Component>>,
}

#[derive(Clone, Copy, PartialEq)]
enum ComponentDetailTab {
    Exports,
    Dependencies,
    Sources,
    Wit,
    Toml,
}

#[derive(Clone, PartialEq)]
struct ComponentDeploymentConfig {
    toml: String,
    sources: Vec<SourceView>,
}

/// Another component of the same deployment and the interfaces connecting it to the inspected one.
#[derive(Debug, PartialEq)]
pub(crate) struct ComponentConnection<'a> {
    pub(crate) component: &'a grpc_client::Component,
    pub(crate) interfaces: Vec<IfcFqn>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ComponentConnections<'a> {
    pub(crate) dependencies: Vec<ComponentConnection<'a>>,
    callers: Vec<ComponentConnection<'a>>,
    /// Imports no other component of the deployment exports, e.g. those provided by the runtime.
    other_imports: Vec<IfcFqn>,
}

pub(crate) fn component_name(component: &grpc_client::Component) -> &str {
    &component
        .component_id
        .as_ref()
        .expect("`component_id` is sent")
        .name
}

/// Whether the component imports a dynamic support interface, letting it call any function of
/// the deployment by name. JS components do when their code imports the dynamic module.
pub(crate) fn calls_dynamically(component: &grpc_client::Component) -> bool {
    interfaces(&component.imports).iter().any(|ifc| {
        let ifc_name = ifc
            .ifc_name
            .strip_suffix("-backtrace")
            .unwrap_or(&ifc.ifc_name);
        ifc.pkg_fqn.is_namespace_obelisk()
            && matches!(
                (ifc.pkg_fqn.package_name.as_str(), ifc_name),
                ("workflow", "workflow-dynamic-support") | ("webhook", "webhook-dynamic-support")
            )
    })
}

/// Link opening the component on its deployment page.
pub(crate) fn component_link(
    component: &grpc_client::Component,
    deployment_id: &DeploymentId,
) -> Html {
    let name = component_name(component).to_string();
    html! {
        <Link<Route, DeploymentQuery>
            to={Route::DeploymentDetail { deployment_id: deployment_id.clone() }}
            query={Some(DeploymentQuery { component: Some(name.clone()) })}
        >
            { component.as_type().as_icon_html() }
            {" "}
            {name}
        </Link<Route, DeploymentQuery>>
    }
}

fn interfaces(functions: &[FunctionDetail]) -> Vec<IfcFqn> {
    map_interfaces_to_fn_details(functions, InterfaceFilter::All)
        .into_keys()
        .collect()
}

pub(crate) fn component_connections<'a>(
    component: &grpc_client::Component,
    deployment_components: &'a HashMap<String, grpc_client::Component>,
) -> ComponentConnections<'a> {
    let name = component_name(component);
    let imports = interfaces(&component.imports);
    let exports = interfaces(&component.exports);
    let mut others = deployment_components
        .iter()
        .filter(|(other_name, _)| other_name.as_str() != name)
        .map(|(_, other)| {
            (
                other,
                interfaces(&other.imports),
                interfaces(&other.exports),
            )
        })
        .collect::<Vec<_>>();
    others.sort_by(|(a, ..), (b, ..)| component_name(a).cmp(component_name(b)));

    let connect = |ours: &[IfcFqn], theirs: &[IfcFqn]| {
        ours.iter()
            .filter(|ifc| theirs.contains(ifc))
            .cloned()
            .collect::<Vec<_>>()
    };
    let dependencies = others
        .iter()
        .map(|(other, _, other_exports)| ComponentConnection {
            component: other,
            interfaces: connect(&imports, other_exports),
        })
        .filter(|connection| !connection.interfaces.is_empty())
        .collect::<Vec<_>>();
    let callers = others
        .iter()
        .map(|(other, other_imports, _)| ComponentConnection {
            component: other,
            interfaces: connect(&exports, other_imports),
        })
        .filter(|connection| !connection.interfaces.is_empty())
        .collect();
    let other_imports = imports
        .into_iter()
        .filter(|ifc| {
            !dependencies
                .iter()
                .any(|dependency| dependency.interfaces.contains(ifc))
        })
        .collect();
    ComponentConnections {
        dependencies,
        callers,
        other_imports,
    }
}

fn component_file_sources(files: &[grpc_client::ComponentFileRef]) -> Vec<SourceView> {
    let mut sources = files
        .iter()
        .filter_map(|file_ref| {
            let role = ComponentFileRole::try_from(file_ref.role)
                .unwrap_or(ComponentFileRole::Unspecified);
            // WIT sources are shown in the dedicated "WIT" tab, not here.
            if role == ComponentFileRole::WitSource {
                return None;
            }
            let file = file_ref.file.as_ref().expect("`file` is sent");
            Some(SourceView {
                file_name: file.path.clone(),
                content: SourceContent::FetchFile {
                    digest: file.digest.clone(),
                },
                metadata: Some(SourceMetadata {
                    role: component_file_role_label(role),
                }),
            })
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
        ComponentFileRole::WitSource => "WIT source",
        ComponentFileRole::Unspecified => "unspecified",
    }
}

fn render_connections(
    title: &'static str,
    help: &'static str,
    empty: &'static str,
    note: Html,
    connections: &[ComponentConnection],
    deployment_id: &DeploymentId,
) -> Html {
    html! {
        <section class="component-connections">
            <h4>{title}</h4>
            <p class="component-section-help">{help}</p>
            {note}
            if connections.is_empty() {
                <p class="component-empty-state">{empty}</p>
            } else {
                <ul>
                    { for connections.iter().map(|connection| {
                        html! {
                            <li>
                                { component_link(connection.component, deployment_id) }
                                <ul class="component-connection-interfaces">
                                    { for connection.interfaces.iter().map(|ifc| html! {
                                        <li>{ifc.to_string()}</li>
                                    }) }
                                </ul>
                            </li>
                        }
                    }) }
                </ul>
            }
        </section>
    }
}

#[component(ComponentDetail)]
pub fn component_detail(
    ComponentDetailProps {
        component,
        deployment_id,
        deployment_components,
    }: &ComponentDetailProps,
) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let is_active_deployment = app_state.current_deployment_id.as_ref() == Some(deployment_id);

    let wit_state = use_state(|| None);
    let wit_loaded = use_state(|| false);
    let selected_tab = use_state(|| ComponentDetailTab::Exports);
    let deployment_config = use_state(|| None::<Result<Option<ComponentDeploymentConfig>, String>>);

    // Fetch raw WIT only when its tab is selected.
    use_effect_with(
        (
            component.clone(),
            deployment_id.clone(),
            is_active_deployment,
            *selected_tab,
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
                let deployment_id = deployment_id.id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let response = rest::get_text(
                        &format!("/v1/components/{}/wit", component_digest.digest),
                        &[("deployment_id", deployment_id)],
                    )
                    .await;
                    match response {
                        Ok(wit) => {
                            let rendered = wit_highlighter::print_all(&wit, render_ffqn_with_links)
                                .unwrap_or_else(|err| {
                                    warn!("Cannot render WIT, showing raw text - {err:?}");
                                    wit_highlighter::print_raw(&wit)
                                });
                            wit_state.set(Some(rendered));
                            wit_loaded.set(true);
                        }
                        Err(e) => {
                            error!("Failed to get WIT: {:?}", e);
                            notifications
                                .push(Notification::error(format!("Failed to get WIT: {}", e)));
                            wit_loaded.set(true);
                        }
                    }
                });
            }
        },
    );

    use_effect_with((component.clone(), deployment_id.clone(), *selected_tab), {
        let deployment_config = deployment_config.clone();
        let notifications = notifications.clone();
        move |(component, deployment_id, selected_tab)| {
            deployment_config.set(None);
            let needs_toml = *selected_tab == ComponentDetailTab::Toml;
            // backcompat: old deployments in the DB (e.g. 0.41.0) lack component file refs; derive their sources from the deployment manifest instead.
            let needs_source_fallback =
                *selected_tab == ComponentDetailTab::Sources && component.files.is_empty();
            if !needs_toml && !needs_source_fallback {
                return;
            }
            let component_name = component_name(component).to_string();
            let deployment_id = deployment_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let response = rest::deployments::get(&deployment_id.id).await;
                match response {
                    Ok(deployment) => {
                        let result = deployment
                            .deployment_toml
                            .as_deref()
                            .map(|manifest| {
                                let manifest = toml::from_str::<serde_json::Value>(manifest)
                                    .map_err(|error| error.to_string())?;
                                let sources =
                                    build_sections_from_manifest(&manifest, &deployment.files)
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
                                                component_display_name(component) == component_name
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
                            error
                        )));
                        deployment_config.set(Some(Err(error)));
                    }
                }
            });
        }
    });

    let exports = map_interfaces_to_fn_details(&component.exports, InterfaceFilter::All);
    let render_exported_ifc_with_fns = |ifc_fqn: &IfcFqn, fn_details: &[FunctionDetail]| {
        let exported_fn_details = fn_details
            .iter()
            .map(|fn_detail| {
                let ffqn =
                    FunctionFqn::from_fn_detail(fn_detail).expect("ffqn should be parseable");
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

    let dependencies = || {
        let connections = component_connections(component, deployment_components);
        let mut dynamic_callers = deployment_components
            .values()
            .filter(|other| {
                component_name(other) != component_name(component) && calls_dynamically(other)
            })
            .collect::<Vec<_>>();
        dynamic_callers.sort_by(|a, b| component_name(a).cmp(component_name(b)));
        let dependencies_note = if calls_dynamically(component) {
            html! {
                <p class="component-dynamic-note">
                    {"This component can also call any function of this deployment by name, so it may depend on components not listed here."}
                </p>
            }
        } else {
            html! {}
        };
        let callers_note = if !component.exports.is_empty() && !dynamic_callers.is_empty() {
            html! {
                <p class="component-dynamic-note">
                    {"Also callable by name from components calling dynamically: "}
                    { for dynamic_callers.iter().enumerate().map(|(idx, caller)| html! { <>
                        if idx > 0 { {", "} }
                        { component_link(caller, deployment_id) }
                    </> }) }
                </p>
            }
        } else {
            html! {}
        };
        html! { <>
            {render_connections(
                "Depends on",
                "Components of this deployment exporting interfaces this component imports.",
                "This component does not import interfaces of other components.",
                dependencies_note,
                &connections.dependencies,
                deployment_id,
            )}
            {render_connections(
                "Called by",
                "Components of this deployment importing interfaces this component exports.",
                "No other component imports interfaces of this component.",
                callers_note,
                &connections.callers,
                deployment_id,
            )}
            if !connections.other_imports.is_empty() {
                <section class="component-connections">
                    <h4>{"Other imports"}</h4>
                    <p class="component-section-help">
                        {"Imported interfaces no component of this deployment exports, e.g. those provided by the runtime."}
                    </p>
                    <ul class="component-connection-interfaces">
                        { for connections.other_imports.iter().map(|ifc| html! {
                            <li>{ifc.to_string()}</li>
                        }) }
                    </ul>
                </section>
            }
        </>}
    };

    let component_id = component.component_id.clone();
    let component_sources = component_file_sources(&component.files);
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
        ComponentDetailTab::Exports => exported_functions,
        ComponentDetailTab::Dependencies => dependencies(),
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
        // The component carries file refs but none are displayable here (e.g.
        // WIT-only sources, which live in the "WIT" tab). No fallback fetch runs,
        // so show the empty state directly instead of a perpetual "Loading...".
        ComponentDetailTab::Sources if !component.files.is_empty() => html! {
            <p class="component-empty-state">{"No sources are available for this component."}</p>
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
        <div class="view-tabs component-detail-tabs">
            {tab_button("Exports", ComponentDetailTab::Exports)}
            {tab_button("Dependencies", ComponentDetailTab::Dependencies)}
            {tab_button("Sources", ComponentDetailTab::Sources)}
            {tab_button("WIT", ComponentDetailTab::Wit)}
            {tab_button("TOML", ComponentDetailTab::Toml)}
        </div>

        <section class="component-detail-tab-content">
            {tab_content}
        </section>
    </>}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
            // WIT sources belong in the "WIT" tab and must be excluded here.
            component_file(
                "wit/component.wit",
                "sha256:wit",
                7,
                ComponentFileRole::WitSource,
            ),
        ];

        let sources = component_file_sources(&files);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].file_name, "src/entry.js");
        assert!(matches!(
            &sources[0].content,
            SourceContent::FetchFile { digest } if digest == "sha256:entry"
        ));
        let metadata = sources[0].metadata.as_ref().unwrap();
        assert_eq!(metadata.role, "JS entrypoint");
        assert_eq!(sources[1].file_name, "src/module.js");
    }

    #[test]
    fn connections_match_imports_to_exports_of_other_components() {
        let components = [
            component(
                "workflow",
                &["app:wf/api"],
                &[
                    "app:act/api",
                    "app:act-obelisk-ext/api",
                    "obelisk:log/log@1.0.0",
                ],
            ),
            component("activity", &["app:act/api", "app:act-obelisk-ext/api"], &[]),
            component("webhook", &[], &["app:wf/api"]),
            component("unrelated", &["app:other/api"], &[]),
        ]
        .into_iter()
        .map(|component| (component_name(&component).to_string(), component))
        .collect::<HashMap<_, _>>();

        let connections = component_connections(&components["workflow"], &components);

        let names = |connections: &[ComponentConnection]| {
            connections
                .iter()
                .map(|connection| {
                    (
                        component_name(connection.component).to_string(),
                        connection
                            .interfaces
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&connections.dependencies),
            [(
                "activity".to_string(),
                vec![
                    "app:act/api".to_string(),
                    "app:act-obelisk-ext/api".to_string()
                ]
            )]
        );
        assert_eq!(
            names(&connections.callers),
            [("webhook".to_string(), vec!["app:wf/api".to_string()])]
        );
        assert_eq!(
            connections.other_imports,
            [IfcFqn::from_str("obelisk:log/log@1.0.0").unwrap()]
        );
    }

    #[test]
    fn dynamic_support_imports_mark_dynamic_callers() {
        assert!(calls_dynamically(&component(
            "js-workflow",
            &[],
            &["obelisk:workflow/workflow-dynamic-support-backtrace@7.0.0"],
        )));
        assert!(calls_dynamically(&component(
            "js-webhook",
            &[],
            &["obelisk:webhook/webhook-dynamic-support@7.0.0"],
        )));
        assert!(!calls_dynamically(&component(
            "wasm-workflow",
            &[],
            &["obelisk:workflow/workflow-support@7.0.0", "app:act/api"],
        )));
    }

    fn component(name: &str, exports: &[&str], imports: &[&str]) -> grpc_client::Component {
        let functions = |interfaces: &[&str]| {
            interfaces
                .iter()
                .map(|interface| FunctionDetail {
                    function_name: Some(grpc_client::FunctionName {
                        interface_name: interface.to_string(),
                        function_name: "f".to_string(),
                    }),
                    ..Default::default()
                })
                .collect()
        };
        grpc_client::Component {
            component_id: Some(grpc_client::ComponentId {
                name: name.to_string(),
                ..Default::default()
            }),
            exports: functions(exports),
            imports: functions(imports),
            files: Vec::new(),
        }
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
