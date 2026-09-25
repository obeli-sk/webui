//! Component and deployment loading utilities.

use crate::grpc::{
    function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
    grpc_client::{self, ComponentId, DeploymentId},
    ifc_fqn::IfcFqn,
};
use crate::rest;
use hashbrown::HashMap;
use log::debug;
use std::rc::Rc;

/// Loaded component data from the server.
#[derive(Clone, Default, PartialEq)]
pub struct LoadedComponents {
    pub components_by_id: HashMap<ComponentId, Rc<grpc_client::Component>>,
    pub components_by_exported_ifc: HashMap<IfcFqn, Rc<grpc_client::Component>>,
}

/// Fetches all components from the server.
pub async fn load_components() -> Result<LoadedComponents, String> {
    let mut components = rest::components::list(None, None).await?;
    debug!("Got REST components");
    components.sort_by(|a, b| {
        a.component_id
            .as_ref()
            .expect("`component_id` is sent")
            .name
            .cmp(
                &b.component_id
                    .as_ref()
                    .expect("`component_id` is sent")
                    .name,
            )
    });
    let components_by_id: HashMap<_, _> = components
        .into_iter()
        .map(|component| {
            (
                component
                    .component_id
                    .clone()
                    .expect("`component_id` is sent"),
                Rc::new(component),
            )
        })
        .collect();

    let components_by_exported_ifc: HashMap<IfcFqn, Rc<grpc_client::Component>> = components_by_id
        .values()
        .flat_map(|component| {
            map_interfaces_to_fn_details(&component.exports, InterfaceFilter::All)
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
    rest::get::<String>("/v1/deployment-id", &[])
        .await
        .map(DeploymentId::from)
}
