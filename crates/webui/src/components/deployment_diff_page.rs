use crate::{
    BASE_URL,
    app::Route,
    components::{
        deployment_config_view::render_config_value,
        notification::{Notification, NotificationContext},
    },
    grpc::grpc_client::{
        self, DeploymentId, deployment_repository_client::DeploymentRepositoryClient,
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

/// Top-level keys of the canonical deployment config in display order.
const SECTIONS: &[(&str, &str)] = &[
    ("workflows_wasm", "Workflows (WASM)"),
    ("workflows_js", "Workflows (JS)"),
    ("activities_wasm", "Activities (WASM)"),
    ("activities_js", "Activities (JS)"),
    ("activities_exec", "Activities (Exec)"),
    ("activities_stub", "Activity Stubs"),
    ("activities_external", "External Activities"),
    ("webhooks_wasm", "Webhooks (WASM)"),
    ("webhooks_js", "Webhooks (JS)"),
    ("crons", "Crons"),
];

fn components_by_name(config: &Value, section_key: &str) -> BTreeMap<String, Value> {
    config
        .get(section_key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|component| {
                    let name = component
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("<unnamed>")
                        .to_string();
                    (name, component.clone())
                })
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

fn render_changed_component(name: &str, from: &Value, to: &Value) -> Html {
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
            <table class="config-table">
                { rows }
            </table>
        </details>
    }
}

fn render_section_diff(
    section_title: &str,
    from_components: &BTreeMap<String, Value>,
    to_components: &BTreeMap<String, Value>,
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
        match (from_components.get(name), to_components.get(name)) {
            (Some(from), Some(to)) if from == to => unchanged += 1,
            (Some(from), Some(to)) => entries.push(render_changed_component(name, from, to)),
            (Some(from), None) => entries.push(html! {
                <details class="component-config diff-removed">
                    <summary><span class="badge removed">{"removed"}</span>{" "}{ name }</summary>
                    { render_config_value(from) }
                </details>
            }),
            (None, Some(to)) => entries.push(html! {
                <details class="component-config diff-added">
                    <summary><span class="badge added">{"added"}</span>{" "}{ name }</summary>
                    { render_config_value(to) }
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

async fn fetch_config(deployment_id: DeploymentId) -> Result<Value, String> {
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
    let config_json = deployment
        .config_json
        .ok_or_else(|| format!("deployment {} has no configuration", deployment_id.id))?;
    serde_json::from_str(&config_json)
        .map_err(|e| format!("cannot parse configuration of {}: {e}", deployment_id.id))
}

#[component(DeploymentDiffPage)]
pub fn deployment_diff_page(
    DeploymentDiffPageProps { from, to }: &DeploymentDiffPageProps,
) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let configs_state = use_state(|| None::<Result<(Value, Value), String>>);

    {
        let configs_state = configs_state.clone();
        let notifications = notifications.clone();
        use_effect_with((from.clone(), to.clone()), move |(from, to)| {
            let from = from.clone();
            let to = to.clone();
            spawn_local(async move {
                let result: Result<(Value, Value), String> = async {
                    let from_config = fetch_config(from).await?;
                    let to_config = fetch_config(to).await?;
                    Ok((from_config, to_config))
                }
                .await;
                if let Err(err) = &result {
                    error!("Deployment diff failed: {err}");
                    notifications.push(Notification::error(err.clone()));
                }
                configs_state.set(Some(result));
            });
        });
    }

    let body = match configs_state.deref() {
        None => html! { <p>{"Loading..."}</p> },
        Some(Err(err)) => html! { <p class="error">{ err }</p> },
        Some(Ok((from_config, to_config))) => {
            let sections: Vec<Html> = SECTIONS
                .iter()
                .filter_map(|(key, title)| {
                    render_section_diff(
                        title,
                        &components_by_name(from_config, key),
                        &components_by_name(to_config, key),
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
