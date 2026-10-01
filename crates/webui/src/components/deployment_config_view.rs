use crate::{
    components::{
        code::syntect_code_block::{SyntectCodeBlock, highlight_code_line_by_line},
        component_detail::ComponentDetail,
        copy_button::CopyButton,
    },
    grpc::grpc_client,
};
use hashbrown::HashMap;
use serde_json::Value;
use std::{path::PathBuf, rc::Rc};
use web_sys::HtmlInputElement;
use yew::prelude::*;

/// Manifest (`deployment.toml`) section keys, in display order, paired with a human title.
/// These are the `[[section]]` table-array keys the server stores verbatim.
pub const MANIFEST_SECTIONS: &[(&str, &str)] = &[
    ("activity_js", "Activities (JS)"),
    ("activity_wasm", "Activities (WASM)"),
    ("activity_exec", "Activities (Exec)"),
    ("activity_vm", "Activities (VM)"),
    ("activity_stub", "Activity Stubs"),
    ("activity_external", "External Activities"),
    ("workflow_js", "Workflows (JS)"),
    ("workflow_wasm", "Workflows (WASM)"),
    ("webhook_endpoint_js", "Webhooks (JS)"),
    ("webhook_endpoint_wasm", "Webhooks (WASM)"),
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
/// resolved to an OCI image, a deployment-owned file, or an external path.
fn script_source(
    table: &Value,
    inline_extension: Option<&str>,
    files: &[grpc_client::FileRef],
) -> Option<SourceView> {
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
    let path = deployment_relative_path(location);
    match files.iter().find(|file| file.path == path) {
        Some(file) => Some(SourceView {
            file_name,
            content: SourceContent::FetchFile {
                digest: file.digest.clone(),
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

/// Normalize a `location` the way the server keys `files[].path` (drops `.` and empty segments).
fn deployment_relative_path(location: &str) -> String {
    location
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Build the per-section view model from a parsed `deployment.toml` manifest and the
/// deployment-owned `files` it references.
pub fn build_sections_from_manifest(
    manifest: &Value,
    files: &[grpc_client::FileRef],
) -> Vec<SectionView> {
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
                    if let Some(source) = script_source(table, inline_extension, files) {
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
                        let response =
                            crate::rest::deployments::component_source(&component_id, &file).await;
                        fetched.set(Some(Rc::new(response)));
                    });
                }
                SourceContent::FetchFile { digest } => {
                    let digest = digest.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let response = crate::rest::deployments::file(&digest).await;
                        fetched.set(Some(Rc::new(response)));
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
    pub components_by_name: Rc<HashMap<String, grpc_client::Component>>,
    /// The deployment these components belong to.
    pub deployment_id: grpc_client::DeploymentId,
    /// Name of the component to open and scroll to.
    #[prop_or_default]
    pub focused_component: Option<String>,
}

#[component(DeploymentConfigView)]
pub fn deployment_config_view(
    DeploymentConfigViewProps {
        sections,
        components_by_name,
        deployment_id,
        focused_component,
    }: &DeploymentConfigViewProps,
) -> Html {
    let search = use_state(String::new);
    // The filter must not hide the component being navigated to.
    {
        let search = search.clone();
        use_effect_with(focused_component.clone(), move |focused_component| {
            if focused_component.is_some() {
                search.set(String::new());
            }
        });
    }
    let on_search = {
        let search = search.clone();
        Callback::from(move |event: InputEvent| {
            let input: HtmlInputElement = event.target_unchecked_into();
            search.set(input.value());
        })
    };

    if sections.is_empty() {
        return html! { <p>{"This deployment contains no components."}</p> };
    }
    let query = search.trim().to_lowercase();
    let total = sections
        .iter()
        .map(|section| section.components.len())
        .sum::<usize>();
    let visible = sections
        .iter()
        .flat_map(|section| &section.components)
        .filter(|component| component.name.to_lowercase().contains(&query))
        .count();

    html! {
        <>
            <div class="deployment-component-search">
                <input type="search" value={(*search).clone()} oninput={on_search}
                    placeholder="Filter components by name" aria-label="Filter components by name" />
                <span>{format!("{visible} of {total} components")}</span>
            </div>
            if visible == 0 {
                <p class="component-empty-state">{"No components match this name."}</p>
            }
            // A nested fragment keeps the keyed sections out of the unkeyed siblings, otherwise
            // filtering recreates the search input and it loses focus.
            <>{for sections.iter().filter_map(|section| {
                let matches = section.components.iter()
                    .filter(|component| component.name.to_lowercase().contains(&query))
                    .collect::<Vec<_>>();
                if matches.is_empty() {
                    None
                } else {
                    Some(html! {
                        <section class="deployment-section" key={section.toml_key}>
                            <h5>{section.title}<span>{format!("{}", matches.len())}</span></h5>
                            <div class="deployment-component-list">
                                {for matches.into_iter().map(|component| html! {
                                    <DeploymentComponentCard
                                        key={component.name.clone()}
                                        component={component.clone()}
                                        metadata={components_by_name.get(&component.name).cloned().map(Rc::new)}
                                        deployment_components={components_by_name.clone()}
                                        deployment_id={deployment_id.clone()}
                                        focused={focused_component.as_ref() == Some(&component.name)}
                                    />
                                })}
                            </div>
                        </section>
                    })
                }
            })}</>
        </>
    }
}

#[derive(Properties, PartialEq)]
struct DeploymentComponentCardProps {
    component: ComponentView,
    metadata: Option<Rc<grpc_client::Component>>,
    deployment_components: Rc<HashMap<String, grpc_client::Component>>,
    deployment_id: grpc_client::DeploymentId,
    focused: bool,
}

#[component(DeploymentComponentCard)]
fn deployment_component_card(props: &DeploymentComponentCardProps) -> Html {
    let open = use_state(|| props.focused);
    let card_ref = use_node_ref();
    {
        let open = open.clone();
        let card_ref = card_ref.clone();
        use_effect_with(props.focused, move |focused| {
            if *focused {
                open.set(true);
                if let Some(card) = card_ref.cast::<web_sys::Element>() {
                    card.scroll_into_view();
                }
            }
        });
    }
    let ontoggle = {
        let open = open.clone();
        Callback::from(move |event: Event| {
            let details: web_sys::HtmlElement = event.target_unchecked_into();
            open.set(details.has_attribute("open"));
        })
    };

    html! {
        <details
            ref={card_ref}
            class={classes!("deployment-component-card", props.focused.then_some("focused"))}
            open={*open}
            {ontoggle}
        >
            <summary>
                <span class="component-name">{&props.component.name}</span>
                if let Some(metadata) = &props.metadata {
                    <span class="deployment-component-type">{metadata.as_type().as_label()}</span>
                }
            </summary>
            if *open {
                <div class="deployment-component-detail">
                    if let Some(metadata) = &props.metadata {
                        <ComponentDetail
                            component={metadata.clone()}
                            deployment_id={props.deployment_id.clone()}
                            deployment_components={props.deployment_components.clone()}
                        />
                    } else {
                        <h5>{"Configuration"}</h5>
                        {render_config_value(&props.component.config)}
                        if !props.component.sources.is_empty() {
                            <h5>{"Sources"}</h5>
                            <div class="component-sources">
                                {for props.component.sources.iter().map(|source| html! {
                                    <CollapsibleSource source={source.clone()} component_id={None} />
                                })}
                            </div>
                        }
                    }
                </div>
            }
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::build_sections_from_manifest;
    use serde_json::json;

    #[test]
    fn activity_vm_is_rendered_as_a_component_section() {
        let sections = build_sections_from_manifest(
            &json!({
                "activity_vm": [{
                    "name": "sandboxed-task",
                    "nix": { "packages": ["hello"] }
                }]
            }),
            &[],
        );

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].toml_key, "activity_vm");
        assert_eq!(sections[0].title, "Activities (VM)");
        assert_eq!(sections[0].components.len(), 1);
        assert_eq!(sections[0].components[0].name, "sandboxed-task");
    }
}
