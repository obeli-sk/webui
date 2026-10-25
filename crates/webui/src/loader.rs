//! Component and deployment loading utilities.

use crate::model::{ComponentConfigExt as _, InterfaceFilter, map_interfaces_to_fn_details};
use crate::rest;
use concepts::{ComponentId, IfcFqnName, prefixed_ulid::DeploymentId};
use hashbrown::HashMap;
use log::debug;
use rest_types::{components::ComponentConfig, server::ServerInfoResponse};
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

pub async fn get_server_info() -> Result<ServerInfoResponse, String> {
    rest::get("/v1/info", &[]).await
}
