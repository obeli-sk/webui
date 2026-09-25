use crate::grpc::{ffqn::FunctionFqn, grpc_client as grpc};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::{collections::HashMap, str::FromStr};

#[derive(Deserialize)]
struct EventPage {
    events: Vec<EventRecord>,
}

#[derive(Deserialize)]
struct EventRecord {
    event: serde_json::Value,
}

#[derive(Deserialize)]
struct Created {
    ffqn: String,
    params: serde_json::Value,
    parent: Option<(String, String)>,
    scheduled_at: DateTime<Utc>,
    component_id: ComponentId,
    deployment_id: String,
    metadata: HashMap<String, String>,
    scheduled_by: Option<String>,
    max_persisted_value_size_bytes: u64,
}

#[derive(Deserialize)]
struct ComponentId {
    component_type: String,
    name: String,
    component_digest: String,
}

pub async fn child_created(id: &str) -> Result<Option<grpc::execution_event::Created>, String> {
    let page: EventPage = super::get(
        &format!("/v1/executions/{id}/events"),
        &[
            ("version", "0".to_string()),
            ("length", "1".to_string()),
            ("including_cursor", "true".to_string()),
        ],
    )
    .await?;
    page.events
        .into_iter()
        .next()
        .map(|record| {
            let value = record
                .event
                .get("created")
                .ok_or_else(|| "First execution event is not Created".to_string())?;
            let created: Created =
                serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
            created.try_into()
        })
        .transpose()
}

impl TryFrom<Created> for grpc::execution_event::Created {
    type Error = String;

    fn try_from(value: Created) -> Result<Self, Self::Error> {
        let ffqn = FunctionFqn::from_str(&value.ffqn).map_err(|err| err.to_string())?;
        let component_type = match value.component_id.component_type.as_str() {
            "workflow" => grpc::ComponentType::Workflow,
            "activity" => grpc::ComponentType::Activity,
            "activity_stub" => grpc::ComponentType::ActivityStub,
            "webhook_endpoint" => grpc::ComponentType::WebhookEndpoint,
            "cron" => grpc::ComponentType::Cron,
            other => return Err(format!("Unknown component type: {other}")),
        };
        Ok(Self {
            function_name: Some(ffqn.into()),
            params: Some(prost_wkt_types::Any {
                type_url: format!("urn:obelisk:json:params:{}", value.ffqn),
                value: serde_json::to_vec(&value.params).map_err(|err| err.to_string())?,
            }),
            scheduled_at: Some(value.scheduled_at.into()),
            component_id: Some(grpc::ComponentId {
                component_type: component_type as i32,
                name: value.component_id.name,
                digest: Some(grpc::ContentDigest {
                    digest: value.component_id.component_digest,
                }),
            }),
            deployment_id: Some(grpc::DeploymentId {
                id: value.deployment_id,
            }),
            parent_execution_id: value
                .parent
                .as_ref()
                .map(|(id, _)| grpc::ExecutionId { id: id.clone() }),
            parent_join_set_id: value
                .parent
                .as_ref()
                .map(|(_, id)| parse_join_set_id(id))
                .transpose()?,
            metadata: value.metadata,
            scheduled_by: value.scheduled_by.map(|id| grpc::ExecutionId { id }),
            max_persisted_value_size_bytes: value.max_persisted_value_size_bytes,
        })
    }
}

fn parse_join_set_id(value: &str) -> Result<grpc::JoinSetId, String> {
    let (kind, name) = value
        .split_once(':')
        .ok_or_else(|| format!("Invalid join set ID: {value}"))?;
    let kind = match kind {
        "o" => grpc::join_set_id::JoinSetKind::OneOff,
        "n" => grpc::join_set_id::JoinSetKind::Named,
        "g" => grpc::join_set_id::JoinSetKind::Generated,
        _ => return Err(format!("Invalid join set ID: {value}")),
    };
    Ok(grpc::JoinSetId {
        kind: kind as i32,
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_event_preserves_parent_and_json_params() {
        let created: Created = serde_json::from_value(serde_json::json!({
            "ffqn": "example:pkg/ifc.run",
            "params": [42, {"name": "child"}],
            "parent": ["parent-id", "n:children"],
            "scheduled_at": "2026-01-01T00:00:00Z",
            "component_id": {
                "component_type": "workflow",
                "name": "worker",
                "component_digest": "sha256:abc"
            },
            "deployment_id": "deployment-id",
            "metadata": {"traceparent": "trace"},
            "scheduled_by": null,
            "max_persisted_value_size_bytes": 1024
        }))
        .unwrap();
        let mapped: grpc::execution_event::Created = created.try_into().unwrap();
        assert_eq!(mapped.parent_execution_id.unwrap().id, "parent-id");
        assert_eq!(mapped.parent_join_set_id.unwrap().name, "children");
        assert_eq!(mapped.params.unwrap().value, br#"[42,{"name":"child"}]"#);
    }
}
