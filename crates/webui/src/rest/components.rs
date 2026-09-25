use crate::grpc::{ffqn::FunctionFqn, grpc_client as grpc};
use serde::Deserialize;

#[derive(Deserialize)]
struct Component {
    component_id: ComponentId,
    files: Vec<ComponentFile>,
    #[serde(default)]
    imports: Vec<Function>,
    #[serde(default)]
    exports: Vec<Function>,
}

#[derive(Deserialize)]
struct ComponentId {
    component_type: String,
    name: String,
    component_digest: String,
}

#[derive(Deserialize)]
struct ComponentFile {
    file: File,
    role: String,
}

#[derive(Deserialize)]
struct File {
    path: String,
    digest: String,
    size: u64,
}

#[derive(Deserialize)]
struct Function {
    ffqn: String,
    parameter_types: Vec<Parameter>,
    return_type: String,
    extension: Option<String>,
    submittable: bool,
}

#[derive(Deserialize)]
struct Parameter {
    name: String,
    wit_type: String,
}

pub async fn list(
    deployment_id: Option<&str>,
    digest: Option<&str>,
) -> Result<Vec<grpc::Component>, String> {
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
    let components: Vec<Component> = super::get("/v1/components", &query).await?;
    components.into_iter().map(TryInto::try_into).collect()
}

impl TryFrom<Component> for grpc::Component {
    type Error = String;

    fn try_from(component: Component) -> Result<Self, Self::Error> {
        let component_type = match component.component_id.component_type.as_str() {
            "workflow" => grpc::ComponentType::Workflow,
            "activity" => grpc::ComponentType::Activity,
            "activity_stub" => grpc::ComponentType::ActivityStub,
            "webhook_endpoint" => grpc::ComponentType::WebhookEndpoint,
            "cron" => grpc::ComponentType::Cron,
            other => return Err(format!("Unknown component type: {other}")),
        };
        let files = component
            .files
            .into_iter()
            .map(|file| {
                let role = match file.role.as_str() {
                    "wasm_component" => grpc::ComponentFileRole::WasmComponent,
                    "exec_program" => grpc::ComponentFileRole::ExecProgram,
                    "js_entrypoint" => grpc::ComponentFileRole::JsEntrypoint,
                    "js_module" => grpc::ComponentFileRole::JsModule,
                    "backtrace_source" => grpc::ComponentFileRole::BacktraceSource,
                    "wit_source" => grpc::ComponentFileRole::WitSource,
                    other => return Err(format!("Unknown component file role: {other}")),
                };
                Ok(grpc::ComponentFileRef {
                    file: Some(grpc::FileRef {
                        path: file.file.path,
                        digest: file.file.digest,
                        size: file.file.size,
                    }),
                    role: role as i32,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(grpc::Component {
            component_id: Some(grpc::ComponentId {
                component_type: component_type as i32,
                name: component.component_id.name,
                digest: Some(grpc::ContentDigest {
                    digest: component.component_id.component_digest,
                }),
            }),
            exports: component
                .exports
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            imports: component
                .imports
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            files,
        })
    }
}

impl TryFrom<Function> for grpc::FunctionDetail {
    type Error = String;

    fn try_from(function: Function) -> Result<Self, Self::Error> {
        let fqn: FunctionFqn = function
            .ffqn
            .parse()
            .map_err(|error| format!("Invalid function name: {error}"))?;
        let extension = match function.extension.as_deref() {
            None => None,
            Some("submit") => Some(grpc::FunctionExtension::Submit as i32),
            Some("await_next") => Some(grpc::FunctionExtension::AwaitNext as i32),
            Some("schedule") => Some(grpc::FunctionExtension::Schedule as i32),
            Some("stub") => Some(grpc::FunctionExtension::Stub as i32),
            Some("get") => Some(grpc::FunctionExtension::Get as i32),
            Some(other) => return Err(format!("Unknown function extension: {other}")),
        };
        Ok(grpc::FunctionDetail {
            function_name: Some(fqn.into()),
            params: function
                .parameter_types
                .into_iter()
                .map(|parameter| grpc::FunctionParameter {
                    name: parameter.name,
                    r#type: Some(grpc::WitType {
                        wit_type: parameter.wit_type.clone(),
                        type_wrapper: parameter.wit_type.clone(),
                        wit_type_inline: parameter.wit_type,
                    }),
                })
                .collect(),
            return_type: Some(grpc::WitType {
                wit_type: function.return_type.clone(),
                type_wrapper: function.return_type.clone(),
                wit_type_inline: function.return_type,
            }),
            extension,
            submittable: function.submittable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_component_preserves_function_types_and_files() {
        let component: Component = serde_json::from_str(r#"{
            "component_id": {"component_type":"workflow","name":"sample","component_digest":"sha256:abc"},
            "files": [{"file":{"path":"src/main.js","digest":"sha256:def","size":7},"role":"js_entrypoint"}],
            "exports": [{"ffqn":"sample:pkg/ifc.run","parameter_types":[{"name":"input","wit_type":"string"}],"return_type":"result<string, string>","extension":null,"submittable":true}],
            "imports": []
        }"#).unwrap();
        let mapped: grpc::Component = component.try_into().unwrap();
        assert_eq!(mapped.component_id.as_ref().unwrap().name, "sample");
        assert_eq!(
            mapped.files[0].role(),
            grpc::ComponentFileRole::JsEntrypoint
        );
        assert_eq!(
            mapped.exports[0].params[0]
                .r#type
                .as_ref()
                .unwrap()
                .wit_type_inline,
            "string"
        );
        assert_eq!(
            mapped.exports[0]
                .return_type
                .as_ref()
                .unwrap()
                .wit_type_inline,
            "result<string, string>"
        );
    }
}
