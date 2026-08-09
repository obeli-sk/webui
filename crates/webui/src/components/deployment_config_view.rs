use crate::{
    app::Route,
    components::{
        code::syntect_code_block::{SyntectCodeBlock, highlight_code_line_by_line},
        component_list_page::ComponentQuery,
        copy_button::CopyButton,
        execution_list_page::ExecutionQuery,
        ffqn_with_links::FfqnWithLinks,
        function_signature::FunctionSignature,
    },
    grpc::{
        ffqn::FunctionFqn,
        function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
        grpc_client,
    },
};
use hashbrown::HashMap;
use serde_json::Value;
use std::{path::PathBuf, rc::Rc};
use yew::prelude::*;
use yew_router::prelude::*;

/// Manifest (`deployment.toml`) section keys, in display order, paired with a human title.
/// These are the `[[section]]` table-array keys the server stores verbatim.
pub const MANIFEST_SECTIONS: &[(&str, &str)] = &[
    ("workflow_wasm", "Workflows (WASM)"),
    ("workflow_js", "Workflows (JS)"),
    ("activity_wasm", "Activities (WASM)"),
    ("activity_js", "Activities (JS)"),
    ("activity_exec", "Activities (Exec)"),
    ("activity_stub", "Activity Stubs"),
    ("activity_external", "External Activities"),
    ("webhook_endpoint_wasm", "Webhooks (WASM)"),
    ("webhook_endpoint_js", "Webhooks (JS)"),
    ("cron", "Crons"),
];

/// One top-level section of the deployment manifest (e.g. `workflow_js`).
#[derive(PartialEq, Clone)]
pub struct SectionView {
    pub title: &'static str,
    /// The manifest table-array key, used to regenerate a `[[key]]` TOML snippet.
    pub toml_key: &'static str,
    pub components: Vec<ComponentView>,
}

#[derive(PartialEq, Clone)]
pub struct ComponentView {
    pub name: String,
    /// Component configuration with source contents replaced by a marker.
    pub config: Value,
    pub sources: Vec<SourceView>,
}

#[derive(PartialEq, Clone)]
pub struct SourceView {
    pub file_name: String,
    pub content: SourceContent,
    pub metadata: Option<SourceMetadata>,
}

#[derive(PartialEq, Clone)]
pub struct SourceMetadata {
    pub role: &'static str,
}

#[derive(PartialEq, Clone)]
pub enum SourceContent {
    Inline(String),
    Oci {
        image: String,
    },
    /// External local file, read at runtime; its content is not part of the deployment.
    ExternalPath {
        path: String,
    },
    /// A deployment-owned file in the content-addressed store, fetched via the `GetFile` RPC.
    FetchFile {
        digest: String,
    },
    /// Backtrace source fetched via the `GetBacktraceSource` RPC.
    Fetch {
        file: String,
    },
}

const SOURCE_MARKER: &str = "(source rendered below)";

/// The display name of a manifest component: its explicit `name`, else the
/// auto-derived `{interface}.{function}` tail of its `ffqn`, else `<unnamed>`.
pub fn component_display_name(table: &Value) -> String {
    if let Some(name) = table.get("name").and_then(Value::as_str) {
        return name.to_string();
    }
    if let Some(ffqn) = table.get("ffqn").and_then(Value::as_str) {
        // ffqn is `namespace:package/interface.function`; the default name is the `/` tail.
        return ffqn.rsplit('/').next().unwrap_or(ffqn).to_string();
    }
    "<unnamed>".to_string()
}

/// Replace a string leaf at `config[path]` with the source marker, if present.
fn strip_path(config: &mut Value, path: &[&str]) {
    let mut current = &mut *config;
    for key in &path[..path.len() - 1] {
        match current.get_mut(key) {
            Some(next) => current = next,
            None => return,
        }
    }
    if let Some(leaf) = current.get_mut(path[path.len() - 1])
        && leaf.is_string()
    {
        *leaf = Value::String(SOURCE_MARKER.to_string());
    }
}

/// Replace every value under `[backtrace] sources = { ... }` with the source marker.
fn strip_backtrace_sources(config: &mut Value) {
    if let Some(Value::Object(map)) = config
        .get_mut("backtrace")
        .and_then(|b| b.get_mut("sources"))
    {
        for (_, content) in map.iter_mut() {
            *content = Value::String(SOURCE_MARKER.to_string());
        }
    }
}

/// Backtrace sources of a WASM component (`[backtrace] sources`), each fetched lazily
/// via the `GetBacktraceSource` RPC by its frame-file key.
fn backtrace_sources(table: &Value) -> Vec<SourceView> {
    let Some(map) = table
        .get("backtrace")
        .and_then(|b| b.get("sources"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut sources: Vec<_> = map
        .keys()
        .map(|file| SourceView {
            file_name: file.clone(),
            content: SourceContent::Fetch { file: file.clone() },
            metadata: None,
        })
        .collect();
    sources.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    sources
}

/// The script source of a JS/exec component: inline `content`, or its `location`
/// resolved to an OCI image, a CAS blob (by `content_digest`), or an external path.
fn script_source(table: &Value, inline_extension: Option<&str>) -> Option<SourceView> {
    if let Some(content) = table.get("content").and_then(Value::as_str) {
        let file_name = table.get("location").and_then(Value::as_str).map_or_else(
            || {
                inline_extension.map_or_else(
                    || "inline source".to_string(),
                    |extension| format!("inline source.{extension}"),
                )
            },
            file_name_of,
        );
        return Some(SourceView {
            file_name,
            content: SourceContent::Inline(content.to_string()),
            metadata: None,
        });
    }
    let location = table.get("location").and_then(Value::as_str)?;
    if location.starts_with("oci://") {
        return Some(SourceView {
            file_name: location.to_string(),
            content: SourceContent::Oci {
                image: location.to_string(),
            },
            metadata: None,
        });
    }
    let file_name = file_name_of(location);
    match table.get("content_digest").and_then(Value::as_str) {
        Some(digest) => Some(SourceView {
            file_name,
            content: SourceContent::FetchFile {
                digest: digest.to_string(),
            },
            metadata: None,
        }),
        None => Some(SourceView {
            file_name,
            content: SourceContent::ExternalPath {
                path: location.to_string(),
            },
            metadata: None,
        }),
    }
}

/// The trailing path segment of a (possibly `${DEPLOYMENT_DIR}/`-prefixed) location.
fn file_name_of(location: &str) -> String {
    location
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(location)
        .to_string()
}

/// Build the per-section view model from a parsed `deployment.toml` manifest.
pub fn build_sections_from_manifest(manifest: &Value) -> Vec<SectionView> {
    let mut sections = Vec::new();
    for (toml_key, title) in MANIFEST_SECTIONS {
        let Some(tables) = manifest.get(toml_key).and_then(Value::as_array) else {
            continue;
        };
        let has_backtrace = matches!(*toml_key, "workflow_wasm" | "webhook_endpoint_wasm");
        let has_script = matches!(
            *toml_key,
            "workflow_js" | "activity_js" | "activity_exec" | "webhook_endpoint_js"
        );
        let components = tables
            .iter()
            .map(|table| {
                let mut config = table.clone();
                let mut sources = Vec::new();
                if has_script {
                    let inline_extension = toml_key.ends_with("_js").then_some("js");
                    if let Some(source) = script_source(table, inline_extension) {
                        sources.push(source);
                    }
                    strip_path(&mut config, &["content"]);
                }
                if has_backtrace {
                    sources.extend(backtrace_sources(table));
                    strip_backtrace_sources(&mut config);
                }
                ComponentView {
                    name: component_display_name(table),
                    config,
                    sources,
                }
            })
            .collect();
        sections.push(SectionView {
            title,
            toml_key,
            components,
        });
    }
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

struct PreparedSource {
    content: String,
    highlighted: Rc<[(Html, usize)]>,
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
    let has_opened = use_state(|| false);
    let fetched = use_state(|| None::<Rc<Result<String, String>>>);

    let ontoggle = {
        let opened = opened.clone();
        let has_opened = has_opened.clone();
        Callback::from(move |event: Event| {
            let details: web_sys::HtmlElement = event.target_unchecked_into();
            let is_open = details.has_attribute("open");
            if is_open {
                has_opened.set(true);
            }
            opened.set(is_open);
        })
    };

    // Fetch the source via RPC when first opened.
    {
        let fetched = fetched.clone();
        let source = source.clone();
        let component_id = component_id.clone();
        use_effect_with(*opened, move |opened| {
            if !*opened || fetched.is_some() {
                return;
            }
            match &source.content {
                SourceContent::Fetch { file } => {
                    let Some(component_id) = component_id else {
                        fetched.set(Some(Rc::new(Err(
                            "component not found in this deployment".to_string()
                        ))));
                        return;
                    };
                    let file = file.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let mut client =
                            grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                                crate::auth::client(),
                            );
                        let response = client
                            .get_backtrace_source(grpc_client::GetBacktraceSourceRequest {
                                component_id: Some(component_id),
                                file,
                            })
                            .await;
                        fetched.set(Some(Rc::new(
                            response
                                .map(|resp| resp.into_inner().content)
                                .map_err(|err| err.message().to_string()),
                        )));
                    });
                }
                SourceContent::FetchFile { digest } => {
                    let digest = digest.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let mut client =
                            grpc_client::deployment_repository_client::DeploymentRepositoryClient::new(
                                crate::auth::client(),
                            );
                        let response = client
                            .get_file(grpc_client::GetFileRequest { digest })
                            .await;
                        fetched.set(Some(Rc::new(
                            response
                                .map(|resp| {
                                    String::from_utf8_lossy(&resp.into_inner().content).into_owned()
                                })
                                .map_err(|err| err.message().to_string()),
                        )));
                    });
                }
                _ => {}
            }
        });
    }

    // Preparing syntax-highlighted lines is relatively expensive. Keep the result
    // after the first expansion so reopening a fetched file is immediate.
    let prepared_source = use_memo(
        (source.clone(), fetched.as_ref().cloned(), *has_opened),
        |(source, fetched, has_opened)| {
            if !has_opened {
                return None;
            }
            let content = match &source.content {
                SourceContent::Inline(content) => Some(content.clone()),
                SourceContent::Fetch { .. } | SourceContent::FetchFile { .. } => fetched
                    .as_ref()
                    .and_then(|result| result.as_ref().as_ref().ok())
                    .cloned(),
                SourceContent::Oci { .. } | SourceContent::ExternalPath { .. } => None,
            }?;
            let language = PathBuf::from(&source.file_name)
                .extension()
                .map(|extension| extension.to_string_lossy().to_string());
            Some(PreparedSource {
                highlighted: Rc::from(highlight_code_line_by_line(&content, language.as_deref())),
                content,
            })
        },
    );

    let body = if !*opened {
        html! {}
    } else {
        let source_available = match &source.content {
            SourceContent::Inline(_) => true,
            SourceContent::Oci { image } => {
                return html! {
                    <details {ontoggle} class="source-block">
                        { source_summary(source) }
                        <p>{ format!("Source is stored in the OCI image `{image}`.") }</p>
                    </details>
                };
            }
            SourceContent::ExternalPath { path } => {
                return html! {
                    <details {ontoggle} class="source-block">
                        { source_summary(source) }
                        <p>{ format!("Source is read at runtime from the external path `{path}`.") }</p>
                    </details>
                };
            }
            SourceContent::Fetch { .. } | SourceContent::FetchFile { .. } => {
                match fetched.as_ref().map(Rc::as_ref) {
                    None => false,
                    Some(Ok(_)) => true,
                    Some(Err(err)) => {
                        return html! {
                            <details {ontoggle} class="source-block">
                                { source_summary(source) }
                                <p class="error">{ format!("Cannot fetch source: {err}") }</p>
                            </details>
                        };
                    }
                }
            }
        };
        if source_available {
            match prepared_source.as_ref() {
                None => html! { <p>{"Loading..."}</p> },
                Some(prepared) => {
                    html! {
                        <div class="source-code">
                            <CopyButton text={prepared.content.clone()} />
                            <SyntectCodeBlock
                                source={prepared.highlighted.clone()}
                                focus_line={None}
                                lines_above={0}
                                lines_below={0}
                                on_expand={Callback::from(|_| {})}
                            />
                        </div>
                    }
                }
            }
        } else {
            html! { <p>{"Loading..."}</p> }
        }
    };

    html! {
        <details {ontoggle} class="source-block">
            { source_summary(source) }
            { body }
        </details>
    }
}

fn source_summary(source: &SourceView) -> Html {
    html! {
        <summary>
            <span class="source-file-name">{ &source.file_name }</span>
            if let Some(metadata) = &source.metadata {
                <span class="source-file-metadata">
                    <span>{metadata.role}</span>
                </span>
            }
        </summary>
    }
}

#[derive(Properties, PartialEq)]
pub struct DeploymentConfigViewProps {
    pub sections: Vec<SectionView>,
    /// Component name -> component metadata resolved via `ListComponents` for this deployment.
    pub components_by_name: HashMap<String, grpc_client::Component>,
    /// The deployment these components belong to; threaded into component detail links.
    pub deployment_id: grpc_client::DeploymentId,
    /// Whether execution submission is safe because this deployment is active.
    pub allow_submit: bool,
}

#[component(DeploymentConfigView)]
pub fn deployment_config_view(
    DeploymentConfigViewProps {
        sections,
        components_by_name,
        deployment_id,
        allow_submit,
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
                    <div class="deployment-component-list">
                        { for section.components.iter().map(|component| {
                            let component_metadata = components_by_name.get(&component.name);
                            let component_id = component_metadata
                                .and_then(|component| component.component_id.clone());
                            let exports = component_metadata
                                .map(|component| map_interfaces_to_fn_details(
                                    &component.exports,
                                    InterfaceFilter::WithoutExtensions,
                                ))
                                .unwrap_or_default();
                            html!{
                                <article class="deployment-component-card">
                                    <header>
                                    <span class="component-name">{ &component.name }</span>
                                    if let Some(component_id) = &component_id {
                                        <span class="component-link">
                                            <Link<Route, ComponentQuery>
                                                to={Route::Component { component_id: component_id.clone() }}
                                                query={ComponentQuery { deployment_id: Some(deployment_id.id.clone()) }}
                                            >
                                                {"Component details"}
                                            </Link<Route, ComponentQuery>>
                                        </span>
                                    }
                                    </header>
                                if !exports.is_empty() {
                                    <div class="deployment-component-exports">
                                        { for exports.iter().map(|(interface, functions)| html! {
                                            <section class="types-interface">
                                                <h4>
                                                    <Link<Route, ExecutionQuery>
                                                        to={Route::ExecutionList}
                                                        query={ExecutionQuery {
                                                            ffqn_prefix: Some(interface.to_string()),
                                                            show_derived: true,
                                                            ..Default::default()
                                                        }}
                                                    >
                                                        {interface.to_string()}
                                                    </Link<Route, ExecutionQuery>>
                                                </h4>
                                                <ul>
                                                    { for functions.iter().map(|function| {
                                                        let ffqn = FunctionFqn::from_fn_detail(function)
                                                            .expect("exported function must be parseable");
                                                        html! {
                                                            <li>
                                                                <FfqnWithLinks
                                                                    {ffqn}
                                                                    hide_submit={!*allow_submit || !function.submittable}
                                                                />
                                                                {": "}
                                                                <span>
                                                                    <FunctionSignature
                                                                        params={function.params.clone()}
                                                                        return_type={function.return_type.clone()}
                                                                    />
                                                                </span>
                                                            </li>
                                                        }
                                                    }) }
                                                </ul>
                                            </section>
                                        }) }
                                    </div>
                                } else {
                                    <p class="component-empty-state">{"No exported functions."}</p>
                                }
                                </article>
                            }
                        })}
                    </div>
                </section>
            }
        })
        .collect()
}
