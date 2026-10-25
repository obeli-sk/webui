//! Component and deployment loading utilities.

use crate::model::{ComponentConfigExt as _, InterfaceFilter, map_interfaces_to_fn_details};
use crate::rest;
use concepts::{
    ComponentId, IfcFqnName,
    prefixed_ulid::{DeploymentId, NodeRunId},
};
use hashbrown::HashMap;
use log::debug;
use obelisk_api_types::ServerInfoResponse;
use rest_types::components::ComponentConfig;
use std::rc::Rc;

/// Loaded component data from the server.
#[derive(Clone, Default, PartialEq)]
pub struct LoadedComponents {
    pub components_by_id: HashMap<ComponentId, Rc<ComponentConfig>>,
    pub components_by_exported_ifc: HashMap<IfcFqnName, Rc<ComponentConfig>>,
}

/// Fetches all components from the server.
pub async fn load_components() -> Result<LoadedComponents, String> {
    let mut components = rest::components::list(None, None).await?;
    debug!("Got REST components");
    components.sort_by(|a, b| a.component_id.name.cmp(&b.component_id.name));
    let components_by_id: HashMap<_, _> = components
        .into_iter()
        .map(|component| (component.component_id.clone(), Rc::new(component)))
        .collect();

    let components_by_exported_ifc: HashMap<IfcFqnName, Rc<ComponentConfig>> = components_by_id
        .values()
        .flat_map(|component| {
            map_interfaces_to_fn_details(component.exported_fns(), InterfaceFilter::All)
                .keys()
                .map(|ifc| (ifc.clone(), component.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    Ok(LoadedComponents {
        components_by_id,
        components_by_exported_ifc,
    })
}

/// Fetches the current deployment ID from the server.
pub async fn get_current_deployment_id() -> Result<DeploymentId, String> {
    Ok(get_server_info().await?.deployment_id)
}

pub struct ServerInfo {
    response: ServerInfoResponse,
    pub deployment_id: DeploymentId,
    pub node_run_id: NodeRunId,
}

impl std::ops::Deref for ServerInfo {
    type Target = ServerInfoResponse;

    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

pub async fn get_server_info() -> Result<ServerInfo, String> {
    let response: ServerInfoResponse = rest::get("/v1/info", &[]).await?;
    Ok(ServerInfo {
        deployment_id: response
            .deployment_id
            .parse::<DeploymentId>()
            .map_err(|err| err.to_string())?,
        node_run_id: response
            .node_run_id
            .parse::<NodeRunId>()
            .map_err(|err| err.to_string())?,
        response,
    })
}
