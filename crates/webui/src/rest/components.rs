use concepts::{component_id::ComponentDigest, prefixed_ulid::DeploymentId};
use rest_types::components::ComponentConfig;

pub async fn list(
    deployment_id: Option<&DeploymentId>,
    digest: Option<&ComponentDigest>,
) -> Result<Vec<ComponentConfig>, String> {
    let mut query = vec![
        ("exports", "true".to_string()),
        ("imports", "true".to_string()),
        ("extensions", "true".to_string()),
    ];
    if let Some(deployment_id) = deployment_id {
        query.push(("deployment_id", deployment_id.to_string()));
    }
    if let Some(digest) = digest {
        query.push(("digest", digest.to_string()));
    }
    super::get("/v1/components", &query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use concepts::{FunctionExtension, storage::ComponentFileRole};

    #[test]
    fn caller_supplied_schedule_id_extension_preserves_its_signature() {
        let function: rest_types::components::FunctionMetadataLite = serde_json::from_str(
            r#"{
            "ffqn":"sample:pkg-obelisk-schedule/ifc.run-schedule-with-id",
            "parameter_types":[{"name":"execution-id","wit_type":"execution-id"}],
            "return_type":"result<execution-id, schedule-with-id-error>",
            "extension":"schedule_with_id","submittable":false
        }"#,
        )
        .unwrap();
        assert_eq!(function.extension, Some(FunctionExtension::ScheduleWithId));
        assert_eq!(function.parameter_types[0].name, "execution-id");
        assert_eq!(
            function.return_type,
            "result<execution-id, schedule-with-id-error>"
        );
    }

    #[test]
    fn rest_component_preserves_function_types_and_files() {
        let component: ComponentConfig = serde_json::from_str(r#"{
            "component_id": {"component_type":"workflow","name":"sample","component_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
            "files": [{"file":{"path":"src/main.js","digest":"sha256:def","size":7},"role":"js_entrypoint"}],
            "exports": [{"ffqn":"sample:pkg/ifc.run","parameter_types":[{"name":"input","wit_type":"string"}],"return_type":"result<string, string>","extension":null,"submittable":true}],
            "imports": []
        }"#).unwrap();
        assert_eq!(component.component_id.name.as_ref(), "sample");
        assert_eq!(component.files[0].role, ComponentFileRole::JsEntrypoint);
        let exports = component.exports.unwrap();
        assert_eq!(exports[0].parameter_types[0].wit_type, "string");
        assert_eq!(exports[0].return_type, "result<string, string>");
    }
}
