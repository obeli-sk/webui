//! Component and deployment loading utilities.

use crate::{
    BASE_URL,
    grpc::{
        function_detail::{InterfaceFilter, map_interfaces_to_fn_details},
        grpc_client::{self, ComponentId, DeploymentId},
        ifc_fqn::IfcFqn,
    },
};
use hashbrown::HashMap;
use log::debug;
use std::rc::Rc;

/// Loaded component data from the server.
#[derive(Clone, PartialEq)]
pub struct LoadedComponents {
    pub components_by_id: HashMap<ComponentId, Rc<grpc_client::Component>>,
    pub components_by_exported_ifc: HashMap<IfcFqn, Rc<grpc_client::Component>>,
}

/// Fetches all components from the server.
pub async fn load_components() -> Result<LoadedComponents, tonic::Status> {
    let mut fn_repo_client = grpc_client::function_repository_client::FunctionRepositoryClient::new(
        tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
    );
    let mut response = fn_repo_client
        .list_components(grpc_client::ListComponentsRequest {
            extensions: true,
            ..Default::default()
        })
        .await?
        .into_inner();
    debug!("Got gRPC ListComponentsResponse");
    response.components.sort_by(|a, b| {
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
    let components_by_id: HashMap<_, _> = response
        .components
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
pub async fn get_current_deployment_id() -> Result<DeploymentId, tonic::Status> {
    let mut deployment_client =
        grpc_client::deployment_repository_client::DeploymentRepositoryClient::new(
            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
        );
    let response = deployment_client
        .get_current_deployment_id(grpc_client::GetCurrentDeploymentIdRequest {})
        .await?
        .into_inner();
    Ok(response
        .deployment_id
        .expect("`deployment_id` is sent by server"))
}
