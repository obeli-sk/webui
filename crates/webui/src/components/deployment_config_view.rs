use crate::{
    BASE_URL,
    app::Route,
    components::{
        code::syntect_code_block::{SyntectCodeBlock, highlight_code_line_by_line},
        copy_button::CopyButton,
    },
    grpc::{ffqn::FunctionFqn, grpc_client},
};
use deployment_config::config::{self as cfg, DeploymentCanonical};
use hashbrown::HashMap;
use serde_json::Value;
use std::{path::PathBuf, rc::Rc, str::FromStr};
use yew::prelude::*;
use yew_router::prelude::*;

/// One top-level section of the deployment config (e.g. `workflows_js`).
#[derive(PartialEq, Clone)]
pub struct SectionView {
    pub title: &'static str,
    /// Field name of the section in [`DeploymentCanonical`], used as the TOML key.
    pub toml_key: &'static str,
    pub components: Vec<ComponentView>,
}

#[derive(PartialEq, Clone)]
pub struct ComponentView {
    pub name: String,
    pub ffqn: Option<FunctionFqn>,
    /// Component configuration with source contents replaced by a marker.
    pub config: Value,
    pub sources: Vec<SourceView>,
}

#[derive(PartialEq, Clone)]
pub struct SourceView {
    pub file_name: String,
    pub content: SourceContent,
}

#[derive(PartialEq, Clone)]
pub enum SourceContent {
    Inline(String),
    Oci {
        image: String,
    },
    /// Source must be fetched via the `GetBacktraceSource` RPC.
    Fetch {
        file: String,
    },
}

const SOURCE_MARKER: &str = "(source rendered below)";

fn to_stripped_value<T: serde::Serialize>(component: &T) -> Value {
    serde_json::to_value(component).expect("config components are serializable")
}

fn parse_ffqn(ffqn: &deployment_config::FunctionFqn) -> Option<FunctionFqn> {
    FunctionFqn::from_str(&ffqn.to_string()).ok()
}

fn strip_path(config: &mut Value, path: &[&str]) {
    let mut current = &mut *config;
    for key in &path[..path.len() - 1] {
        match current.get_mut(key) {
            Some(next) => current = next,
            None => return,
        }
    }
    if let Some(leaf) = current.get_mut(path[path.len() - 1]) {
        *leaf = Value::String(SOURCE_MARKER.to_string());
    }
}

fn js_location_source(location: &cfg::JsLocationCanonical) -> SourceView {
    match location {
        cfg::JsLocationCanonical::Content { content, file_name } => SourceView {
            file_name: file_name.clone(),
            content: SourceContent::Inline(content.clone()),
        },
        cfg::JsLocationCanonical::Oci { image } => SourceView {
            file_name: image.clone(),
            content: SourceContent::Oci {
                image: image.clone(),
            },
        },
    }
}

fn backtrace_sources(backtrace: &cfg::ComponentBacktraceConfigCanonical) -> Vec<SourceView> {
    let mut sources: Vec<_> = backtrace
        .frame_files_to_sources
        .iter()
        .map(|(file, content)| SourceView {
            file_name: file.clone(),
            content: if content.is_empty() {
                SourceContent::Fetch { file: file.clone() }
            } else {
                SourceContent::Inline(content.clone())
            },
        })
        .collect();
    sources.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    sources
}

fn strip_backtrace_sources(config: &mut Value) {
    if let Some(Value::Object(map)) = config
        .get_mut("backtrace")
        .and_then(|b| b.get_mut("frame_files_to_sources"))
    {
        for (_, content) in map.iter_mut() {
            *content = Value::String(SOURCE_MARKER.to_string());
        }
    }
}

/// Build the per-section view model from a parsed canonical deployment config.
pub fn build_sections(config: &DeploymentCanonical) -> Vec<SectionView> {
    let mut sections = Vec::new();

    sections.push(SectionView {
        title: "Workflows (WASM)",
        toml_key: "workflows_wasm",
        components: config
            .workflows_wasm
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_backtrace_sources(&mut value);
                ComponentView {
                    name: c.common.name.to_string(),
                    ffqn: None,
                    config: value,
                    sources: backtrace_sources(&c.backtrace),
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Workflows (JS)",
        toml_key: "workflows_js",
        components: config
            .workflows_js
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_path(&mut value, &["location", "content", "content"]);
                ComponentView {
                    name: c.name.to_string(),
                    ffqn: parse_ffqn(&c.ffqn),
                    config: value,
                    sources: vec![js_location_source(&c.location)],
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Activities (WASM)",
        toml_key: "activities_wasm",
        components: config
            .activities_wasm
            .iter()
            .map(|c| ComponentView {
                name: c.common.name.to_string(),
                ffqn: None,
                config: to_stripped_value(c),
                sources: Vec::new(),
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Activities (JS)",
        toml_key: "activities_js",
        components: config
            .activities_js
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_path(&mut value, &["location", "content", "content"]);
                ComponentView {
                    name: c.name.to_string(),
                    ffqn: parse_ffqn(&c.ffqn),
                    config: value,
                    sources: vec![js_location_source(&c.location)],
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Activities (Exec)",
        toml_key: "activities_exec",
        components: config
            .activities_exec
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_path(&mut value, &["source", "content"]);
                let sources = vec![match &c.source {
                    cfg::ExecSourceCanonical::Content(content) => SourceView {
                        file_name: c.name.to_string(),
                        content: SourceContent::Inline(content.clone()),
                    },
                    cfg::ExecSourceCanonical::Oci { image } => SourceView {
                        file_name: image.clone(),
                        content: SourceContent::Oci {
                            image: image.clone(),
                        },
                    },
                }];
                ComponentView {
                    name: c.name.to_string(),
                    ffqn: parse_ffqn(&c.ffqn),
                    config: value,
                    sources,
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Activity Stubs",
        toml_key: "activities_stub",
        components: config
            .activities_stub
            .iter()
            .map(|c| ComponentView {
                name: c.name_str().to_string(),
                ffqn: match c {
                    cfg::ActivityStubComponentConfigCanonical::Inline(i) => parse_ffqn(&i.ffqn),
                    cfg::ActivityStubComponentConfigCanonical::File(_) => None,
                },
                config: to_stripped_value(c),
                sources: Vec::new(),
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "External Activities",
        toml_key: "activities_external",
        components: config
            .activities_external
            .iter()
            .map(|c| ComponentView {
                name: c.name_str().to_string(),
                ffqn: match c {
                    cfg::ActivityExternalComponentConfigCanonical::Inline(i) => parse_ffqn(&i.ffqn),
                    cfg::ActivityExternalComponentConfigCanonical::File(_) => None,
                },
                config: to_stripped_value(c),
                sources: Vec::new(),
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Webhooks (WASM)",
        toml_key: "webhooks_wasm",
        components: config
            .webhooks_wasm
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_backtrace_sources(&mut value);
                ComponentView {
                    name: c.common.name.to_string(),
                    ffqn: None,
                    config: value,
                    sources: backtrace_sources(&c.backtrace),
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Webhooks (JS)",
        toml_key: "webhooks_js",
        components: config
            .webhooks_js
            .iter()
            .map(|c| {
                let mut value = to_stripped_value(c);
                strip_path(&mut value, &["location", "content", "content"]);
                ComponentView {
                    name: c.name.to_string(),
                    ffqn: None,
                    config: value,
                    sources: vec![js_location_source(&c.location)],
                }
            })
            .collect(),
    });
    sections.push(SectionView {
        title: "Crons",
        toml_key: "crons",
        components: config
            .crons
            .iter()
            .map(|c| ComponentView {
                name: c.name.to_string(),
                ffqn: parse_ffqn(&c.ffqn),
                config: to_stripped_value(c),
                sources: Vec::new(),
            })
            .collect(),
    });

    sections.retain(|section| !section.components.is_empty());
    sections
}

/// Convert a JSON config value to TOML, dropping `null`s (TOML cannot express them).
fn json_to_toml(value: &Value) -> Option<toml::Value> {
    Some(match value {
        Value::Null => return None,
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        }
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(arr) => toml::Value::Array(arr.iter().filter_map(json_to_toml).collect()),
        Value::Object(obj) => toml::Value::Table(
            obj.iter()
                .filter_map(|(k, v)| Some((k.clone(), json_to_toml(v)?)))
                .collect(),
        ),
    })
}

fn toml_table_to_string(root: toml::value::Table) -> String {
    toml::to_string_pretty(&root).unwrap_or_else(|e| format!("# cannot serialize to TOML: {e}"))
}

/// Serialize one component config as a `[[section]]` TOML snippet.
pub fn component_to_toml(toml_key: &str, config: &Value) -> String {
    let Some(component) = json_to_toml(config) else {
        return String::new();
    };
    let mut root = toml::value::Table::new();
    root.insert(toml_key.to_string(), toml::Value::Array(vec![component]));
    toml_table_to_string(root)
}

/// Serialize all sections as one TOML document.
pub fn sections_to_toml(sections: &[SectionView]) -> String {
    let mut root = toml::value::Table::new();
    for section in sections {
        root.insert(
            section.toml_key.to_string(),
            toml::Value::Array(
                section
                    .components
                    .iter()
                    .filter_map(|component| json_to_toml(&component.config))
                    .collect(),
            ),
        );
    }
    toml_table_to_string(root)
}

/// A copyable, syntax-highlighted TOML snippet.
pub fn toml_block(toml_text: String) -> Html {
    let highlighted: Rc<[(Html, usize)]> =
        Rc::from(highlight_code_line_by_line(&toml_text, Some("toml")));
    html! {
        <div class="toml-block">
            <CopyButton text={toml_text} />
            <SyntectCodeBlock
                source={highlighted}
                focus_line={None}
                lines_above={0}
                lines_below={0}
                on_expand={Callback::from(|_| {})}
            />
        </div>
    }
}

/// Render a JSON config value as nested tables.
pub fn render_config_value(value: &Value) -> Html {
    match value {
        Value::Null => html! { <span class="config-null">{"null"}</span> },
        Value::Bool(b) => html! { <span class="config-scalar">{b.to_string()}</span> },
        Value::Number(n) => html! { <span class="config-scalar">{n.to_string()}</span> },
        Value::String(s) => html! { <span class="config-scalar">{s}</span> },
        Value::Array(arr) if arr.is_empty() => html! { <span class="config-null">{"[]"}</span> },
        Value::Array(arr) => html! {
            <ol class="config-list">
                { for arr.iter().map(|item| html! { <li>{ render_config_value(item) }</li> }) }
            </ol>
        },
        Value::Object(obj) => html! {
            <table class="config-table">
                { for obj.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| html! {
                    <tr>
                        <th>{k}</th>
                        <td>{ render_config_value(v) }</td>
                    </tr>
                })}
            </table>
        },
    }
}

#[derive(Properties, PartialEq)]
pub struct CollapsibleSourceProps {
    pub source: SourceView,
    /// Needed for fetching sources via the `GetBacktraceSource` RPC.
    pub component_id: Option<grpc_client::ComponentId>,
}

/// A `<details>` block that renders (and for `Fetch` sources downloads) the
/// source code lazily on first expansion.
#[component(CollapsibleSource)]
pub fn collapsible_source(
    CollapsibleSourceProps {
        source,
        component_id,
    }: &CollapsibleSourceProps,
) -> Html {
    let opened = use_state(|| false);
    let fetched = use_state(|| None::<Result<String, String>>);

    let ontoggle = {
        let opened = opened.clone();
        Callback::from(move |event: Event| {
            let details: web_sys::HtmlElement = event.target_unchecked_into();
            let is_open = details.has_attribute("open");
            opened.set(is_open);
        })
    };

    // Fetch the source via RPC when first opened.
    {
        let fetched = fetched.clone();
        let source = source.clone();
        let component_id = component_id.clone();
        use_effect_with(*opened, move |opened| {
            if *opened
                && fetched.is_none()
                && let SourceContent::Fetch { file } = &source.content
            {
                let Some(component_id) = component_id else {
                    fetched.set(Some(Err(
                        "component not found in this deployment".to_string()
                    )));
                    return;
                };
                let file = file.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut client =
                        grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                        );
                    let response = client
                        .get_backtrace_source(grpc_client::GetBacktraceSourceRequest {
                            component_id: Some(component_id),
                            file,
                        })
                        .await;
                    fetched.set(Some(
                        response
                            .map(|resp| resp.into_inner().content)
                            .map_err(|err| err.message().to_string()),
                    ));
                });
            }
        });
    }

    let body = if !*opened {
        html! {}
    } else {
        let inline_content = match &source.content {
            SourceContent::Inline(content) => Some(content.clone()),
            SourceContent::Oci { image } => {
                return html! {
                    <details {ontoggle} class="source-block">
                        <summary>{ &source.file_name }</summary>
                        <p>{ format!("Source is stored in the OCI image `{image}`.") }</p>
                    </details>
                };
            }
            SourceContent::Fetch { .. } => match fetched.as_ref() {
                None => None,
                Some(Ok(content)) => Some(content.clone()),
                Some(Err(err)) => {
                    return html! {
                        <details {ontoggle} class="source-block">
                            <summary>{ &source.file_name }</summary>
                            <p class="error">{ format!("Cannot fetch source: {err}") }</p>
                        </details>
                    };
                }
            },
        };
        match inline_content {
            None => html! { <p>{"Loading..."}</p> },
            Some(content) => {
                let language = PathBuf::from(&source.file_name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string());
                let highlighted: Rc<[(Html, usize)]> =
                    Rc::from(highlight_code_line_by_line(&content, language.as_deref()));
                html! {
                    <SyntectCodeBlock
                        source={highlighted}
                        focus_line={None}
                        lines_above={0}
                        lines_below={0}
                        on_expand={Callback::from(|_| {})}
                    />
                }
            }
        }
    };

    html! {
        <details {ontoggle} class="source-block">
            <summary>{ &source.file_name }</summary>
            { body }
        </details>
    }
}

#[derive(Properties, PartialEq)]
pub struct DeploymentConfigViewProps {
    pub sections: Vec<SectionView>,
    /// Component name -> component id resolved via `ListComponents` for this deployment.
    pub components_by_name: HashMap<String, grpc_client::ComponentId>,
}

#[component(DeploymentConfigView)]
pub fn deployment_config_view(
    DeploymentConfigViewProps {
        sections,
        components_by_name,
    }: &DeploymentConfigViewProps,
) -> Html {
    if sections.is_empty() {
        return html! { <p>{"This deployment contains no components."}</p> };
    }
    sections
        .iter()
        .map(|section| {
            html! {
                <section class="deployment-section">
                    <h4>{ section.title } { format!(" ({})", section.components.len()) }</h4>
                    { for section.components.iter().map(|component| {
                        let component_id = components_by_name.get(&component.name).cloned();
                        html!{
                            <details class="component-config">
                                <summary>
                                    <span class="component-name">{ &component.name }</span>
                                    if let Some(component_id) = &component_id {
                                        <span
                                            class="component-link"
                                            onclick={Callback::from(|event: MouseEvent| event.stop_propagation())}
                                        >
                                            <Link<Route> to={Route::Component { component_id: component_id.clone() }}>
                                                {"Component details"}
                                            </Link<Route>>
                                        </span>
                                    }
                                </summary>
                                { toml_block(component_to_toml(section.toml_key, &component.config)) }
                                if !component.sources.is_empty() {
                                    <h5>{"Sources"}</h5>
                                    { for component.sources.iter().map(|source| html!{
                                        <CollapsibleSource
                                            source={source.clone()}
                                            component_id={component_id.clone()}
                                        />
                                    })}
                                }
                            </details>
                        }
                    })}
                </section>
            }
        })
        .collect()
}
