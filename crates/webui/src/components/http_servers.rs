use crate::app::Route;
use crate::components::{
    component_detail::component_link, deployment_detail_page::DeploymentQuery,
};
use concepts::prefixed_ulid::DeploymentId;
use hashbrown::HashMap;
use rest_types::components::ComponentConfig;
use serde_json::Value;
use std::collections::BTreeMap;
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

#[derive(Properties, PartialEq)]
pub struct HttpServerRoutesProps {
    pub deployment_id: DeploymentId,
    pub servers: Rc<BTreeMap<String, Vec<ServerRoute>>>,
    pub components_by_name: Rc<HashMap<String, ComponentConfig>>,
}

#[component(HttpServerRoutes)]
pub fn http_server_routes_view(
    HttpServerRoutesProps {
        deployment_id,
        servers,
        components_by_name,
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
    html! {
        <section class="deployment-http-servers">
            <div class="deployment-section-heading">
                <h4>{"HTTP servers"}</h4>
            </div>
            if servers.is_empty() {
                <p class="secondary-text">{"No webhook endpoint is bound to an HTTP server."}</p>
            } else {
                <p class="secondary-text">
                    {"Servers this deployment binds webhook endpoints to. A server also needs to be enabled in the server configuration to serve them."}
                </p>
                <table class="http-server-routes">
                    <thead>
                        <tr><th>{"Server"}</th><th>{"Route"}</th><th>{"Webhook endpoint"}</th></tr>
                    </thead>
                    <tbody>
                        { for servers.iter().flat_map(|(server, routes)| {
                            routes.iter().enumerate().map(move |(idx, route)| html! {
                                <tr>
                                    if idx == 0 {
                                        <td class="server" rowspan={routes.len().to_string()}>{server.clone()}</td>
                                    }
                                    <td class="route">{route.label()}</td>
                                    <td>{ webhook_link(&route.webhook) }</td>
                                </tr>
                            })
                        }) }
                    </tbody>
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
}
