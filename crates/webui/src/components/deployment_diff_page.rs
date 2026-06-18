use crate::{
    BASE_URL,
    app::Route,
    components::{
        deployment_config_view::{
            ComponentView, MANIFEST_SECTIONS, SectionView, SourceContent, SourceView,
            build_sections_from_manifest, render_config_value,
        },
        notification::{Notification, NotificationContext},
    },
    grpc::grpc_client::{
        self, DeploymentId, deployment_repository_client::DeploymentRepositoryClient,
        execution_repository_client::ExecutionRepositoryClient,
        function_repository_client::FunctionRepositoryClient,
    },
};
use log::error;
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use std::ops::Deref;
use tonic_web_wasm_client::Client;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentDiffPageProps {
    pub from: DeploymentId,
    pub to: DeploymentId,
}

#[derive(Clone)]
struct DeploymentInfo {
    sections: Vec<SectionView>,
    components_by_name: BTreeMap<String, grpc_client::ComponentId>,
}

#[derive(Clone)]
struct DeploymentDiffData {
    from: DeploymentInfo,
    to: DeploymentInfo,
    source_diffs: BTreeMap<(String, String), Vec<SourceDiff>>,
}

#[derive(Clone)]
struct SourceDiff {
    file_name: String,
    change: SourceChange,
}

#[derive(Clone)]
enum SourceChange {
    Added(ResolvedSource),
    Removed(ResolvedSource),
    Changed {
        from: ResolvedSource,
        to: ResolvedSource,
    },
}

#[derive(Clone, PartialEq)]
enum ResolvedSource {
    Text(String),
    Note(String),
    Error(String),
}

fn components_by_name(
    sections: &[SectionView],
    section_key: &str,
) -> BTreeMap<String, ComponentView> {
    sections
        .iter()
        .find(|section| section.toml_key == section_key)
        .map(|section| {
            section
                .components
                .iter()
                .map(|component| (component.name.clone(), component.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Flatten a JSON value into `path -> leaf` entries.
/// Nulls and empty containers are skipped so that e.g. an empty `allowed_hosts`
/// list and a missing one do not produce a noise diff entry.
fn flatten_value(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_value(&path, child, out);
            }
        }
        Value::Array(arr) => {
            for (idx, child) in arr.iter().enumerate() {
                flatten_value(&format!("{prefix}[{idx}]"), child, out);
            }
        }
        Value::Null => {}
        other => {
            out.insert(prefix.to_string(), other.clone());
        }
    }
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn is_multiline(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::String(s)) if s.contains('\n'))
}

fn render_text_diff(old: &str, new: &str) -> Html {
    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(3);
    html! {
        <pre class="text-diff">
            { for groups.iter().enumerate().map(|(group_idx, group)| html!{
                <>
                    if group_idx > 0 {
                        <span class="diff-separator">{"⋮\n"}</span>
                    }
                    { for group.iter().flat_map(|op| diff.iter_changes(op)).map(|change| {
                        let (class, sign) = match change.tag() {
                            ChangeTag::Delete => ("diff-del", "-"),
                            ChangeTag::Insert => ("diff-ins", "+"),
                            ChangeTag::Equal => ("diff-eq", " "),
                        };
                        let line = change.value();
                        let line = if line.ends_with('\n') { line.to_string() } else { format!("{line}\n") };
                        html!{ <span class={class}>{ format!("{sign}{line}") }</span> }
                    })}
                </>
            })}
        </pre>
    }
}

fn render_source_side(source: &ResolvedSource) -> Html {
    match source {
        ResolvedSource::Text(content) => {
            html! { <pre class="source-content">{ content }</pre> }
        }
        ResolvedSource::Note(note) => html! { <p class="source-note">{ note }</p> },
        ResolvedSource::Error(err) => html! { <p class="error">{ err }</p> },
    }
}

fn source_summary(source: &ResolvedSource) -> String {
    match source {
        ResolvedSource::Text(content) => {
            let line_count = content.lines().count();
            format!(
                "{line_count} source line{}",
                if line_count == 1 { "" } else { "s" }
            )
        }
        ResolvedSource::Note(note) | ResolvedSource::Error(note) => note.clone(),
    }
}

fn render_source_diff(diff: &SourceDiff) -> Html {
    match &diff.change {
        SourceChange::Changed { from, to } => {
            let body = match (from, to) {
                (ResolvedSource::Text(from), ResolvedSource::Text(to)) => {
                    render_text_diff(from, to)
                }
                _ => html! {
                    <p>
                        <span class="diff-del">{ source_summary(from) }</span>
                        {" → "}
                        <span class="diff-ins">{ source_summary(to) }</span>
                    </p>
                },
            };
            html! {
                <details open=true class="source-diff">
                    <summary><span class="badge changed">{"changed"}</span>{" "}{ &diff.file_name }</summary>
                    { body }
                </details>
            }
        }
        SourceChange::Added(source) => html! {
            <details class="source-diff">
                <summary><span class="badge added">{"added"}</span>{" "}{ &diff.file_name }</summary>
                { render_source_side(source) }
            </details>
        },
        SourceChange::Removed(source) => html! {
            <details class="source-diff">
                <summary><span class="badge removed">{"removed"}</span>{" "}{ &diff.file_name }</summary>
                { render_source_side(source) }
            </details>
        },
    }
}

fn render_changed_component(
    name: &str,
    from: &Value,
    to: &Value,
    source_diffs: &[SourceDiff],
) -> Html {
    let mut from_leaves = BTreeMap::new();
    let mut to_leaves = BTreeMap::new();
    flatten_value("", from, &mut from_leaves);
    flatten_value("", to, &mut to_leaves);

    let all_paths: Vec<&String> = {
        let mut paths: Vec<&String> = from_leaves.keys().chain(to_leaves.keys()).collect();
        paths.sort();
        paths.dedup();
        paths
    };

    let rows = all_paths
        .into_iter()
        .filter_map(|path| {
            let old = from_leaves.get(path);
            let new = to_leaves.get(path);
            if old == new {
                return None;
            }
            let value_cell = if is_multiline(old) || is_multiline(new) {
                let old = old.map(scalar_to_string).unwrap_or_default();
                let new = new.map(scalar_to_string).unwrap_or_default();
                render_text_diff(&old, &new)
            } else {
                html! {<>
                    <span class="diff-del">
                        { old.map(scalar_to_string).unwrap_or_else(|| "—".to_string()) }
                    </span>
                    {" → "}
                    <span class="diff-ins">
                        { new.map(scalar_to_string).unwrap_or_else(|| "—".to_string()) }
                    </span>
                </>}
            };
            Some(html! {
                <tr>
                    <th>{ path }</th>
                    <td>{ value_cell }</td>
                </tr>
            })
        })
        .collect::<Vec<_>>();

    html! {
        <details open=true class="component-config diff-changed">
            <summary><span class="badge changed">{"changed"}</span>{" "}{ name }</summary>
            if !rows.is_empty() {
                <table class="config-table">
                    { rows }
                </table>
            }
            if !source_diffs.is_empty() {
                <h5>{"Source diffs"}</h5>
                { for source_diffs.iter().map(render_source_diff) }
            }
        </details>
    }
}

fn render_section_diff(
    section_key: &str,
    section_title: &str,
    from_components: &BTreeMap<String, ComponentView>,
    to_components: &BTreeMap<String, ComponentView>,
    source_diffs: &BTreeMap<(String, String), Vec<SourceDiff>>,
) -> Option<Html> {
    let mut names: Vec<&String> = from_components.keys().chain(to_components.keys()).collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return None;
    }

    let mut unchanged = 0usize;
    let mut entries = Vec::new();
    for name in names {
        let source_diffs_for_component = source_diffs
            .get(&(section_key.to_string(), name.clone()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match (from_components.get(name), to_components.get(name)) {
            (Some(from), Some(to))
                if from.config == to.config && source_diffs_for_component.is_empty() =>
            {
                unchanged += 1;
            }
            (Some(from), Some(to)) => entries.push(render_changed_component(
                name,
                &from.config,
                &to.config,
                source_diffs_for_component,
            )),
            (Some(from), None) => entries.push(html! {
                <details class="component-config diff-removed">
                    <summary><span class="badge removed">{"removed"}</span>{" "}{ name }</summary>
                    { render_config_value(&from.config) }
                </details>
            }),
            (None, Some(to)) => entries.push(html! {
                <details class="component-config diff-added">
                    <summary><span class="badge added">{"added"}</span>{" "}{ name }</summary>
                    { render_config_value(&to.config) }
                </details>
            }),
            (None, None) => unreachable!("name comes from one of the maps"),
        }
    }
    if entries.is_empty() {
        return None;
    }
    Some(html! {
        <section class="deployment-section">
            <h4>
                { section_title }
                if unchanged > 0 {
                    <span class="unchanged-count">{ format!(" ({unchanged} unchanged)") }</span>
                }
            </h4>
            { entries }
        </section>
    })
}

async fn fetch_deployment_info(deployment_id: DeploymentId) -> Result<DeploymentInfo, String> {
    let mut client = DeploymentRepositoryClient::new(Client::new(BASE_URL.to_string()));
    let deployment = client
        .get_deployment(grpc_client::GetDeploymentRequest {
            deployment_id: Some(deployment_id.clone()),
        })
        .await
        .map_err(|e| {
            format!(
                "cannot get deployment {}: {}",
                deployment_id.id,
                e.message()
            )
        })?
        .into_inner()
        .deployment
        .ok_or_else(|| format!("deployment {} not found", deployment_id.id))?;
    let deployment_toml = deployment
        .deployment_toml
        .ok_or_else(|| format!("deployment {} has no manifest", deployment_id.id))?;
    let config = toml::from_str(&deployment_toml)
        .map_err(|e| format!("cannot parse manifest of {}: {e}", deployment_id.id))?;
    let sections = build_sections_from_manifest(&config);

    let mut function_client = FunctionRepositoryClient::new(Client::new(BASE_URL.to_string()));
    let components_by_name = function_client
        .list_components(grpc_client::ListComponentsRequest {
            function_name: None,
            component_digest: None,
            extensions: false,
            deployment_id: Some(deployment_id),
        })
        .await
        .map(|resp| {
            resp.into_inner()
                .components
                .into_iter()
                .filter_map(|component| component.component_id)
                .map(|component_id| (component_id.name.clone(), component_id))
                .collect()
        })
        .unwrap_or_default();

    Ok(DeploymentInfo {
        sections,
        components_by_name,
    })
}

async fn resolve_source(
    source: SourceView,
    component_id: Option<grpc_client::ComponentId>,
) -> ResolvedSource {
    match source.content {
        SourceContent::Inline(content) => ResolvedSource::Text(content),
        SourceContent::Oci { image } => {
            ResolvedSource::Note(format!("Source is stored in the OCI image `{image}`."))
        }
        SourceContent::ExternalPath { path } => ResolvedSource::Note(format!(
            "Source is read at runtime from the external path `{path}`."
        )),
        SourceContent::FetchFile { digest } => {
            let mut client = DeploymentRepositoryClient::new(Client::new(BASE_URL.to_string()));
            client
                .get_file(grpc_client::GetFileRequest { digest })
                .await
                .map(|resp| {
                    ResolvedSource::Text(
                        String::from_utf8_lossy(&resp.into_inner().content).into_owned(),
                    )
                })
                .unwrap_or_else(|err| {
                    ResolvedSource::Error(format!("Cannot fetch source: {}", err.message()))
                })
        }
        SourceContent::Fetch { file } => {
            let Some(component_id) = component_id else {
                return ResolvedSource::Error("component not found in this deployment".to_string());
            };
            let mut client = ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));
            client
                .get_backtrace_source(grpc_client::GetBacktraceSourceRequest {
                    component_id: Some(component_id),
                    file,
                })
                .await
                .map(|resp| ResolvedSource::Text(resp.into_inner().content))
                .unwrap_or_else(|err| {
                    ResolvedSource::Error(format!("Cannot fetch source: {}", err.message()))
                })
        }
    }
}

async fn diff_sources(
    from_sources: &[SourceView],
    to_sources: &[SourceView],
    from_component_id: Option<grpc_client::ComponentId>,
    to_component_id: Option<grpc_client::ComponentId>,
) -> Vec<SourceDiff> {
    let from_sources: BTreeMap<String, SourceView> = from_sources
        .iter()
        .map(|source| (source.file_name.clone(), source.clone()))
        .collect();
    let to_sources: BTreeMap<String, SourceView> = to_sources
        .iter()
        .map(|source| (source.file_name.clone(), source.clone()))
        .collect();
    let mut names: Vec<&String> = from_sources.keys().chain(to_sources.keys()).collect();
    names.sort();
    names.dedup();

    let mut diffs = Vec::new();
    for name in names {
        match (from_sources.get(name), to_sources.get(name)) {
            (Some(from), Some(to)) if from == to && from_component_id == to_component_id => {}
            (Some(from), Some(to)) => {
                let from = resolve_source(from.clone(), from_component_id.clone()).await;
                let to = resolve_source(to.clone(), to_component_id.clone()).await;
                if from != to {
                    diffs.push(SourceDiff {
                        file_name: name.clone(),
                        change: SourceChange::Changed { from, to },
                    });
                }
            }
            (Some(from), None) => {
                let from = resolve_source(from.clone(), from_component_id.clone()).await;
                diffs.push(SourceDiff {
                    file_name: name.clone(),
                    change: SourceChange::Removed(from),
                });
            }
            (None, Some(to)) => {
                let to = resolve_source(to.clone(), to_component_id.clone()).await;
                diffs.push(SourceDiff {
                    file_name: name.clone(),
                    change: SourceChange::Added(to),
                });
            }
            (None, None) => unreachable!("name comes from one of the maps"),
        }
    }
    diffs
}

async fn build_source_diffs(
    from: &DeploymentInfo,
    to: &DeploymentInfo,
) -> BTreeMap<(String, String), Vec<SourceDiff>> {
    let mut source_diffs = BTreeMap::new();

    for (section_key, _) in MANIFEST_SECTIONS {
        let from_components = components_by_name(&from.sections, section_key);
        let to_components = components_by_name(&to.sections, section_key);
        let mut names: Vec<&String> = from_components.keys().chain(to_components.keys()).collect();
        names.sort();
        names.dedup();

        for name in names {
            let (Some(from_component), Some(to_component)) =
                (from_components.get(name), to_components.get(name))
            else {
                continue;
            };
            if from_component.sources.is_empty() && to_component.sources.is_empty() {
                continue;
            }
            let diffs = diff_sources(
                &from_component.sources,
                &to_component.sources,
                from.components_by_name.get(name).cloned(),
                to.components_by_name.get(name).cloned(),
            )
            .await;
            if !diffs.is_empty() {
                source_diffs.insert(((*section_key).to_string(), name.clone()), diffs);
            }
        }
    }

    source_diffs
}

async fn fetch_diff_data(
    from: DeploymentId,
    to: DeploymentId,
) -> Result<DeploymentDiffData, String> {
    let from = fetch_deployment_info(from).await?;
    let to = fetch_deployment_info(to).await?;
    let source_diffs = build_source_diffs(&from, &to).await;
    Ok(DeploymentDiffData {
        from,
        to,
        source_diffs,
    })
}

#[component(DeploymentDiffPage)]
pub fn deployment_diff_page(
    DeploymentDiffPageProps { from, to }: &DeploymentDiffPageProps,
) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let diff_state = use_state(|| None::<Result<DeploymentDiffData, String>>);

    {
        let diff_state = diff_state.clone();
        let notifications = notifications.clone();
        use_effect_with((from.clone(), to.clone()), move |(from, to)| {
            let from = from.clone();
            let to = to.clone();
            spawn_local(async move {
                let result = fetch_diff_data(from, to).await;
                if let Err(err) = &result {
                    error!("Deployment diff failed: {err}");
                    notifications.push(Notification::error(err.clone()));
                }
                diff_state.set(Some(result));
            });
        });
    }

    let body = match diff_state.deref() {
        None => html! { <p>{"Loading..."}</p> },
        Some(Err(err)) => html! { <p class="error">{ err }</p> },
        Some(Ok(diff_data)) => {
            let sections: Vec<Html> = MANIFEST_SECTIONS
                .iter()
                .filter_map(|(key, title)| {
                    render_section_diff(
                        key,
                        title,
                        &components_by_name(&diff_data.from.sections, key),
                        &components_by_name(&diff_data.to.sections, key),
                        &diff_data.source_diffs,
                    )
                })
                .collect();
            if sections.is_empty() {
                html! { <p>{"The deployments have identical configurations."}</p> }
            } else {
                sections.into_iter().collect::<Html>()
            }
        }
    };

    html! {
        <>
            <h3>{"Deployment diff"}</h3>
            <p>
                <span class="diff-del">
                    <Link<Route> to={Route::DeploymentDetail { deployment_id: from.clone() }}>
                        { &from.id }
                    </Link<Route>>
                </span>
                {" → "}
                <span class="diff-ins">
                    <Link<Route> to={Route::DeploymentDetail { deployment_id: to.clone() }}>
                        { &to.id }
                    </Link<Route>>
                </span>
            </p>
            { body }
        </>
    }
}
