use crate::{
    components::execution_status::status_to_string,
    grpc::{finished_result_kind::FinishedResultKind, grpc_client, version::VersionType},
};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::Duration;
use yew::Html;

/// Links a direct child-execution/delay node to its correlated events in the detail
/// panel: the `Submit` (`JoinSetRequest`) that spawned it and, once resolved, the
/// `JoinNext` that consumed its result. `version` (the submit) drives the node's badge
/// and DOM id; `group` is every version that should highlight together on hover.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceLink {
    pub version: VersionType,
    pub group: Vec<VersionType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraceData {
    Root(TraceDataRoot),
    Child(TraceDataChild),
}
impl TraceData {
    pub fn name(&self) -> &Html {
        match self {
            TraceData::Root(root) => &root.name,
            TraceData::Child(child) => &child.name,
        }
    }

    pub fn busy(&self) -> &[BusyInterval] {
        match self {
            TraceData::Root(TraceDataRoot { busy, .. }) => busy,
            TraceData::Child(TraceDataChild { busy, .. }) => busy,
        }
    }

    pub fn busy_duration(&self, root_last_event_at: DateTime<Utc>) -> Duration {
        self.busy()
            .iter()
            .filter(|interval| interval.status != BusyIntervalStatus::ExecutionSinceScheduled)
            .map(|interval| interval.duration(root_last_event_at))
            .reduce(|acc, current| acc + current)
            .unwrap_or_default()
    }

    pub fn children(&self) -> &[TraceData] {
        match self {
            TraceData::Root(root) => &root.children,
            TraceData::Child(child) => &child.children,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            TraceData::Root(root) => &root.title,
            TraceData::Child(child) => &child.title,
        }
    }

    pub fn load_button(&self) -> Option<Html> {
        match self {
            TraceData::Root(root) => root.load_button.clone(),
            TraceData::Child(child) => child.load_button.clone(),
        }
    }

    pub fn node_key(&self) -> &str {
        match self {
            TraceData::Root(root) => &root.node_key,
            TraceData::Child(child) => &child.node_key,
        }
    }

    /// Link between a child-execution/delay node and its events on the detail side.
    pub fn link(&self) -> Option<&TraceLink> {
        match self {
            TraceData::Root(root) => root.link.as_ref(),
            TraceData::Child(child) => child.link.as_ref(),
        }
    }

    pub fn is_expanded(&self) -> bool {
        match self {
            TraceData::Root(root) => root.is_expanded,
            TraceData::Child(child) => child.is_expanded,
        }
    }

    pub fn can_expand(&self) -> bool {
        match self {
            TraceData::Root(root) => root.can_expand,
            TraceData::Child(child) => child.can_expand,
        }
    }

    pub fn current_status(&self) -> Option<Html> {
        if let TraceData::Root(TraceDataRoot {
            current_status: Some(status),
            ..
        }) = self
        {
            Some(status_to_string(status))
        } else {
            self.busy()
                .last()
                .map(|interval| Html::from(interval.status.to_string()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, derive_more::Display)]
pub enum BusyIntervalStatus {
    #[display("Finished")]
    HttpTraceFinished(u32),
    #[display("Timeout")]
    HttpTraceNotResponded,
    #[display("Error")]
    HttpTraceError,
    #[display("Temporary timeout")]
    ExecutionTimeoutTemporary,
    #[display("Temporary error")]
    ExecutionErrorTemporary,
    #[display("Locked")]
    ExecutionLocked,
    #[display("In progress")]
    DelayInProgress,
    #[display("Paused")]
    DelayPaused,
    #[display("OK")]
    DelayOk,
    #[display("Cancelled")]
    DelayCancelled,
    #[display("{_0}")]
    ExecutionFinished(FinishedResultKind),
    #[display("Unfinished")]
    ExecutionUnfinishedWithoutPendingState,
    #[display("Since scheduled")]
    ExecutionSinceScheduled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusyInterval {
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub status: BusyIntervalStatus,
}
impl BusyInterval {
    pub fn as_percentage(
        &self,
        root_scheduled_at: DateTime<Utc>,
        root_last_event_at: DateTime<Utc>,
    ) -> (f64, f64) {
        let total_duration_micros =
            root_last_event_at.timestamp_micros() - root_scheduled_at.timestamp_micros();
        let start_percentage = 100.0
            * (self.started_at.timestamp_micros() - root_scheduled_at.timestamp_micros()) as f64
            / total_duration_micros as f64;

        let end_percentage = 100.0
            * (self
                .finished_at
                .unwrap_or(root_last_event_at)
                .timestamp_micros()
                - self.started_at.timestamp_micros()) as f64
            / total_duration_micros as f64;

        (start_percentage, end_percentage)
    }

    fn duration(&self, root_last_event_at: DateTime<Utc>) -> Duration {
        TimeDelta::microseconds(
            self.finished_at
                .unwrap_or(root_last_event_at)
                .timestamp_micros()
                - self.started_at.timestamp_micros(),
        )
        .to_std()
        .expect("started_at must be <= finished_at")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceDataRoot {
    pub node_key: String,
    pub is_expanded: bool,
    pub can_expand: bool,
    pub name: Html,
    pub title: String,
    pub scheduled_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub busy: Vec<BusyInterval>,
    pub children: Vec<TraceData>,
    pub load_button: Option<Html>,
    pub current_status: Option<grpc_client::execution_status::Status>,
    pub link: Option<TraceLink>,
}
impl TraceDataRoot {
    pub fn total_duration(&self) -> Duration {
        TimeDelta::microseconds(
            self.last_event_at.timestamp_micros() - self.scheduled_at.timestamp_micros(),
        )
        .to_std()
        .unwrap_or_default() // If scheduled to the future, current duration is 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceDataChild {
    pub node_key: String,
    pub is_expanded: bool,
    pub can_expand: bool,
    pub name: Html,
    pub title: String,
    pub busy: Vec<BusyInterval>,
    pub children: Vec<TraceData>,
    pub load_button: Option<Html>,
    pub link: Option<TraceLink>,
}

mod grpc {
    use super::BusyIntervalStatus;
    use crate::grpc::finished_result_kind::FinishedResultKind;
    use crate::grpc::grpc_client::supported_function_result;

    impl From<&supported_function_result::Value> for BusyIntervalStatus {
        fn from(supported_function_result_value: &supported_function_result::Value) -> Self {
            BusyIntervalStatus::ExecutionFinished(FinishedResultKind::from(
                supported_function_result_value,
            ))
        }
    }
}

mod css {
    use super::BusyIntervalStatus;
    use crate::grpc::finished_result_kind::FinishedResultKind;

    impl BusyIntervalStatus {
        pub fn get_css_class(&self) -> &'static str {
            match self {
                BusyIntervalStatus::HttpTraceFinished(_) => "busy-http-trace-finished",
                BusyIntervalStatus::HttpTraceNotResponded => "busy-http-trace-unfinished",
                BusyIntervalStatus::HttpTraceError => "busy-http-trace-error",
                BusyIntervalStatus::ExecutionTimeoutTemporary => "busy-execution-timeout-temporary",
                BusyIntervalStatus::ExecutionErrorTemporary => "busy-execution-error-temporary",
                BusyIntervalStatus::ExecutionLocked => "busy-execution-locked",
                BusyIntervalStatus::DelayInProgress => "busy-execution-delay",
                BusyIntervalStatus::DelayPaused => "busy-execution-delay-paused",
                BusyIntervalStatus::DelayOk => "busy-execution-delay-ok",
                BusyIntervalStatus::DelayCancelled => "busy-execution-delay-cancelled",
                BusyIntervalStatus::ExecutionFinished(FinishedResultKind::Ok) => {
                    "busy-execution-finished"
                }
                BusyIntervalStatus::ExecutionFinished(FinishedResultKind::Error) => {
                    "busy-execution-returned-error-variant"
                }
                BusyIntervalStatus::ExecutionFinished(FinishedResultKind::Failed) => {
                    "busy-execution-error-permanent"
                }
                BusyIntervalStatus::ExecutionUnfinishedWithoutPendingState => {
                    "busy-execution-unfinished"
                }
                BusyIntervalStatus::ExecutionSinceScheduled => "busy-execution-since-scheduled",
            }
        }
    }
}
