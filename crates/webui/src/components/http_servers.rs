use crate::app::Route;
use crate::components::{
    component_detail::component_link, deployment_detail_page::DeploymentQuery,
};
use concepts::prefixed_ulid::DeploymentId;
use hashbrown::HashMap;
use obelisk_api_types::ServerInfoHttpServer;
use rest_types::components::ComponentConfig;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use yew::prelude::*;
use yew_router::prelude::Link;

/// Webhook endpoint serving this Web UI, added by the server configuration rather than the deployment.
pub const WEBUI_COMPONENT: &str = "obelisk_webui";

/// A route a webhook endpoint serves on an HTTP server.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerRoute {
    pub webhook: String,
    /// Empty means all methods.
    pub methods: Vec<String>,
    pub route: String,
}

impl ServerRoute {
    pub fn label(&self) -> String {
        if self.methods.is_empty() {
            self.route.clone()
        } else {
            format!("{} {}", self.methods.join(", "), self.route)
        }
    }
}

/// Routes of the manifest's webhook endpoints, grouped by the HTTP server they are bound to.
pub fn http_server_routes(manifest: &Value) -> BTreeMap<String, Vec<ServerRoute>> {
    let mut servers = BTreeMap::<String, Vec<ServerRoute>>::new();
    let webhooks = ["webhook_endpoint_wasm", "webhook_endpoint_js"]
        .into_iter()
        .filter_map(|key| manifest.get(key).and_then(Value::as_array))
        .flatten();
    for webhook in webhooks {
        if let (Some(server), Some(name)) =
            (webhook["http_server"].as_str(), webhook["name"].as_str())
        {
            let routes = webhook["routes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|route| {
                    let (methods, path) = match route {
                        Value::String(path) => (Vec::new(), path.as_str()),
                        detail => (
                            detail["methods"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect(),
                            detail["route"].as_str().unwrap_or_default(),
                        ),
                    };
                    ServerRoute {
                        webhook: name.to_string(),
                        methods,
                        // The webhook router treats an empty route as a catch-all.
                        route: if path.is_empty() { "/*" } else { path }.to_string(),
                    }
                });
            let mut routes = routes.peekable();
            if routes.peek().is_some() {
                servers
                    .entry(server.to_string())
                    .or_default()
                    .extend(routes);
            }
        }
    }
    servers
}

/// Built-in server serving this Web UI.
const WEBUI_SERVER: &str = "webui";

/// URL a browser can open for the route, if it is a plain GET path.
fn route_url(public_url: &str, route: &ServerRoute) -> Option<String> {
    let gettable = route.methods.is_empty() || route.methods.iter().any(|m| m == "GET");
    let plain = !route.route.contains(['*', ':']);
    (gettable && plain).then(|| format!("{}{}", public_url.trim_end_matches('/'), route.route))
}

#[derive(Properties, PartialEq)]
pub struct HttpServerRoutesProps {
    pub deployment_id: DeploymentId,
    pub servers: Rc<BTreeMap<String, Vec<ServerRoute>>>,
    pub components_by_name: Rc<HashMap<String, ComponentConfig>>,
    /// Servers running on this Obelisk server, from `/v1/info`.
    #[prop_or_default]
    pub running: Option<Rc<Vec<ServerInfoHttpServer>>>,
    /// Whether `deployment_id` is the active deployment, so `has_routes` applies to it.
    #[prop_or_default]
    pub active: bool,
}

#[component(HttpServerRoutes)]
pub fn http_server_routes_view(
    HttpServerRoutesProps {
        deployment_id,
        servers,
        components_by_name,
        running,
        active,
    }: &HttpServerRoutesProps,
) -> Html {
    let webhook_link = |name: &str| match components_by_name.get(name) {
        Some(component) => component_link(component, deployment_id),
        None => html! {
            <Link<Route, DeploymentQuery>
                to={Route::DeploymentDetail { deployment_id: *deployment_id }}
                query={Some(DeploymentQuery { component: Some(name.to_string()) })}
            >
                {name.to_string()}
            </Link<Route, DeploymentQuery>>
        },
    };
    // Servers older than `http_servers` in `/v1/info` report an empty list.
    let running = running.as_deref().filter(|running| !running.is_empty());
    let mut names: BTreeSet<&str> = servers.keys().map(String::as_str).collect();
    names.extend(
        running
            .into_iter()
            .flatten()
            .map(|server| server.name.as_str())
            .filter(|name| *name != WEBUI_SERVER),
    );
    let no_routes: &[ServerRoute] = &[];
    let rows = names.into_iter().map(|name| {
        let info = running.and_then(|running| running.iter().find(|server| server.name == name));
        let public_url = info.and_then(|info| info.public_url.as_deref());
        let routes = servers.get(name).map_or(no_routes, Vec::as_slice);
        let routes = if *active && info.is_some_and(|info| !info.has_routes) {
            no_routes
        } else {
            routes
        };
        let server_cell = html! {
            <td class="server" rowspan={routes.len().max(1).to_string()}>
                <div>{name.to_string()}</div>
                if let Some(url) = public_url {
                    <a class="public-url" href={url.to_string()} target="_blank" rel="noopener">{url.to_string()}</a>
                }
                if running.is_some() && info.is_none() {
                    <span class="server-tag">{"Not running"}</span>
                }
            </td>
        };
        if routes.is_empty() {
            return html! {
                <tr>
                    {server_cell}
                    <td class="secondary-text" colspan="2">{"No routes"}</td>
                </tr>
            };
        }
        html! {
            { for routes.iter().enumerate().map(|(idx, route)| html! {
                <tr>
                    if idx == 0 { {server_cell.clone()} }
                    <td class="route">
                        {match public_url.and_then(|url| route_url(url, route)) {
                            Some(href) => html! { <a {href} target="_blank" rel="noopener">{route.label()}</a> },
                            None => html! { route.label() },
                        }}
                    </td>
                    <td>{ webhook_link(&route.webhook) }</td>
                </tr>
            }) }
        }
    });
    let rows: Vec<Html> = rows.collect();
    html! {
        <section class="deployment-http-servers">
            <div class="deployment-section-heading">
                <h4>{"HTTP servers"}</h4>
            </div>
            if rows.is_empty() {
                <p class="secondary-text">{"No webhook endpoint is bound to an HTTP server."}</p>
            } else {
                if running.is_none() {
                    <p class="secondary-text">
                        {"Servers this deployment binds webhook endpoints to. A server also needs to be enabled in the server configuration to serve them."}
                    </p>
                }
                <table class="http-server-routes">
                    <thead>
                        <tr><th>{"Server"}</th><th>{"Route"}</th><th>{"Webhook endpoint"}</th></tr>
                    </thead>
                    <tbody>{ for rows }</tbody>
                </table>
            }
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_grouped_by_server() {
        let manifest: Value = toml::from_str(
            r#"
            [[webhook_endpoint_js]]
            name = "hook"
            http_server = "external"
            routes = ["/a", { methods = ["GET", "POST"], route = "/b" }]

            [[webhook_endpoint_wasm]]
            name = "admin"
            http_server = "internal"
            routes = [""]
            "#,
        )
        .unwrap();

        let servers = http_server_routes(&manifest);

        let labels = |server: &str| {
            servers[server]
                .iter()
                .map(|route| format!("{} {}", route.webhook, route.label()))
                .collect::<Vec<_>>()
        };
        assert_eq!(labels("external"), ["hook /a", "hook GET, POST /b"]);
        assert_eq!(labels("internal"), ["admin /*"]);
    }

    #[test]
    fn only_plain_get_routes_link() {
        let route = |methods: &[&str], route: &str| ServerRoute {
            webhook: "hook".to_string(),
            methods: methods.iter().map(ToString::to_string).collect(),
            route: route.to_string(),
        };
        let url = "https://app.example/";
        assert_eq!(
            route_url(url, &route(&[], "/a")).as_deref(),
            Some("https://app.example/a")
        );
        assert!(route_url(url, &route(&["POST"], "/a")).is_none());
        assert!(route_url(url, &route(&["GET"], "/*")).is_none());
        assert!(route_url(url, &route(&["GET"], "/users/:id")).is_none());
    }
}
