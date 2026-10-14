//! UI helpers on top of the obelisk REST types.

use crate::{
    components::execution_header::ExecutionLink, tree::Icon, util::color::generate_color_from_hash,
};
use chrono::{DateTime, Utc};
use concepts::{
    ComponentType, ExecutionId, FunctionFqn, IfcFqnName, JoinSetId, PkgFqn,
    SupportedFunctionReturnValue,
    prefixed_ulid::DelayId,
    storage::{PendingState, PendingStateFinishedResultKind},
};
use indexmap::IndexMap;
use rest_types::{
    components::{ComponentConfig, FunctionMetadataLite},
    executions::{ExecutionWithStateSer, RetVal},
};
use std::hash::{DefaultHasher, Hash, Hasher as _};
use yew::{Html, html};

pub const EXECUTION_ID_INFIX: &str = ".";

fn color_of(value: impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    generate_color_from_hash(hasher.finish())
}

pub trait ComponentTypeExt {
    fn as_label(&self) -> &'static str;
    fn as_icon(&self) -> Icon;
    fn as_icon_html(&self) -> Html {
        Html::from(self.as_icon())
    }
    fn as_root_label(&self) -> Html;
}

impl ComponentTypeExt for ComponentType {
    fn as_label(&self) -> &'static str {
        match self {
            ComponentType::Workflow => "Workflow",
            ComponentType::Activity => "Activity",
            ComponentType::ActivityStub => "Activity Stub",
            ComponentType::WebhookEndpoint => "Webhook Endpoint",
            ComponentType::Cron => "Cron",
        }
    }

    fn as_icon(&self) -> Icon {
        match self {
            ComponentType::Workflow => Icon::GanttChart,
            ComponentType::Activity => Icon::CodeBlock,
            ComponentType::ActivityStub => Icon::Import,
            ComponentType::WebhookEndpoint => Icon::GlobeNetwork,
            ComponentType::Cron => Icon::Calendar,
        }
    }

    fn as_root_label(&self) -> Html {
        Html::from(match self {
            ComponentType::Workflow => "Workflows",
            ComponentType::Activity => "Activities",
            ComponentType::WebhookEndpoint => "Webhooks",
            ComponentType::ActivityStub => "Stub Activities",
            ComponentType::Cron => "Crons",
        })
    }
}

pub trait ExecutionIdExt {
    /// Return (part, execution_id) chain from the topmost parent to `self`.
    fn as_hierarchy(&self) -> Vec<(String, ExecutionId)>;
    fn parent_id(&self) -> Option<ExecutionId>;
    fn render_execution_parts(&self, hide_parents: bool, link: ExecutionLink) -> Html;
    fn color(&self) -> String;
}

impl ExecutionIdExt for ExecutionId {
    fn as_hierarchy(&self) -> Vec<(String, ExecutionId)> {
        let id = self.to_string();
        let mut prefix = String::new();
        let mut vec = Vec::new();
        for part in id.split(EXECUTION_ID_INFIX) {
            prefix = if prefix.is_empty() {
                part.to_string()
            } else {
                format!("{prefix}{EXECUTION_ID_INFIX}{part}")
            };
            let execution_id = prefix
                .parse()
                .expect("every prefix of an execution ID is an execution ID");
            vec.push((part.to_string(), execution_id));
        }
        vec
    }

    fn parent_id(&self) -> Option<ExecutionId> {
        let id = self.to_string();
        id.rsplit_once(EXECUTION_ID_INFIX).map(|(left, _)| {
            left.parse()
                .expect("every prefix of an execution ID is an execution ID")
        })
    }

    fn render_execution_parts(&self, hide_parents: bool, link: ExecutionLink) -> Html {
        let mut execution_id_vec = self.as_hierarchy();
        if hide_parents {
            execution_id_vec.drain(..execution_id_vec.len() - 1);
        }
        execution_id_vec
            .into_iter()
            .enumerate()
            .map(|(idx, (part, execution_id))| {
                html! {<>
                    if idx > 0 {
                        {EXECUTION_ID_INFIX}
                    }
                    {link.link(execution_id, &part)}
                </>}
            })
            .collect::<Html>()
    }

    fn color(&self) -> String {
        color_of(self.to_string())
    }
}

pub trait ColorExt {
    fn color(&self) -> String;
}

impl ColorExt for JoinSetId {
    fn color(&self) -> String {
        color_of(self.to_string())
    }
}

impl ColorExt for DelayId {
    fn color(&self) -> String {
        color_of(self.to_string())
    }
}

pub trait FunctionFqnExt {
    fn short(&self) -> String;
}

impl FunctionFqnExt for FunctionFqn {
    fn short(&self) -> String {
        format!(".../{}.{}", self.ifc_fqn.ifc_name(), self.function_name)
    }
}

pub fn pkg_fqn_from_wit(value: &wit_parser::PackageName) -> PkgFqn {
    PkgFqn {
        namespace: value.namespace.clone(),
        package_name: value.name.clone(),
        version: value.version.as_ref().map(ToString::to_string),
    }
}

pub fn ifc_fqn_from_wit(
    pkg_fqn: &PkgFqn,
    interface: &wit_parser::Interface,
) -> Result<IfcFqnName, anyhow::Error> {
    let ifc_name = interface
        .name
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("inline interfaces are not supported"))?;
    Ok(IfcFqnName::from_pkg_fqn(pkg_fqn, ifc_name))
}

/// The outcome of a finished execution, rendered identically wherever it is
/// shown (execution status badge, trace timeline). Execution failures of any
/// kind collapse to `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum FinishedResultKind {
    #[display("OK")]
    Ok,
    #[display("Error")]
    Error,
    #[display("Failed")]
    Failed,
}

impl From<&PendingStateFinishedResultKind> for FinishedResultKind {
    fn from(value: &PendingStateFinishedResultKind) -> Self {
        use concepts::storage::PendingStateFinishedError;
        match value {
            PendingStateFinishedResultKind::Ok => FinishedResultKind::Ok,
            PendingStateFinishedResultKind::Err(PendingStateFinishedError::Error) => {
                FinishedResultKind::Error
            }
            PendingStateFinishedResultKind::Err(PendingStateFinishedError::ExecutionFailure(_)) => {
                FinishedResultKind::Failed
            }
        }
    }
}

impl From<&RetVal> for FinishedResultKind {
    fn from(value: &RetVal) -> Self {
        match value {
            RetVal::Ok(_) => FinishedResultKind::Ok,
            RetVal::Err(_) => FinishedResultKind::Error,
            RetVal::ExecutionFailed(_) => FinishedResultKind::Failed,
        }
    }
}

impl From<&SupportedFunctionReturnValue> for FinishedResultKind {
    fn from(value: &SupportedFunctionReturnValue) -> Self {
        match value {
            SupportedFunctionReturnValue::Ok(_) => FinishedResultKind::Ok,
            SupportedFunctionReturnValue::Err(_) => FinishedResultKind::Error,
            SupportedFunctionReturnValue::ExecutionFailure(_) => FinishedResultKind::Failed,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InterfaceFilter {
    All,
    // without -ext, -stub interfaces
    WithoutExtensions,
}

pub fn map_interfaces_to_fn_details(
    functions: &[FunctionMetadataLite],
    filter: InterfaceFilter,
) -> IndexMap<IfcFqnName, Vec<FunctionMetadataLite>> {
    let mut interfaces_to_fn_details: IndexMap<IfcFqnName, Vec<FunctionMetadataLite>> =
        IndexMap::new();
    for function_detail in functions {
        let ifc_fqn = &function_detail.ffqn.ifc_fqn;
        if filter == InterfaceFilter::All || !ifc_fqn.is_extension() {
            interfaces_to_fn_details
                .entry(ifc_fqn.clone())
                .or_default()
                .push(function_detail.clone());
        }
    }
    interfaces_to_fn_details.sort_keys();
    for (_, fns) in interfaces_to_fn_details.iter_mut() {
        fns.sort_by(|a, b| a.ffqn.function_name.cmp(&b.ffqn.function_name));
    }
    interfaces_to_fn_details
}

/// Latest known status of an execution, as tracked by the status subscription.
#[derive(Debug, Clone, PartialEq)]
pub enum StatusMessage {
    Summary(ExecutionWithStateSer),
    CurrentStatus(PendingState),
    Finished(FinishedStatus),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinishedStatus {
    pub scheduled_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub retval: RetVal,
}

pub trait ComponentConfigExt {
    fn component_type(&self) -> ComponentType;
    fn exported_fns(&self) -> &[FunctionMetadataLite];
    fn imported_fns(&self) -> &[FunctionMetadataLite];
}

impl ComponentConfigExt for ComponentConfig {
    fn component_type(&self) -> ComponentType {
        self.component_id.component_type
    }
    fn exported_fns(&self) -> &[FunctionMetadataLite] {
        self.exports.as_deref().unwrap_or_default()
    }
    fn imported_fns(&self) -> &[FunctionMetadataLite] {
        self.imports.as_deref().unwrap_or_default()
    }
}
