// Generated from obelisk/proto/obelisk.proto and checked in so WebUI builds do not require protoc.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GenerateExecutionIdRequest {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GenerateExecutionIdResponse {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListExecutionsRequest {
    /// Do not return child executions
    #[prost(bool, tag = "2")]
    pub top_level_only: bool,
    #[prost(bool, tag = "5")]
    pub hide_finished: bool,
    #[prost(string, optional, tag = "6")]
    pub execution_id_prefix: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "7")]
    pub component_digest: ::core::option::Option<ContentDigest>,
    #[prost(message, optional, tag = "8")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(message, optional, tag = "9")]
    pub function_filter: ::core::option::Option<
        list_executions_request::ExecutionFunctionFilter,
    >,
    /// Match executions in any of the given states (logical OR). Empty list matches all.
    #[prost(
        enumeration = "list_executions_request::ExecutionStateFilter",
        repeated,
        tag = "10"
    )]
    pub state_filters: ::prost::alloc::vec::Vec<i32>,
    /// latest by `created_at` if not set
    #[prost(oneof = "list_executions_request::Pagination", tags = "3, 4")]
    pub pagination: ::core::option::Option<list_executions_request::Pagination>,
}
/// Nested message and enum types in `ListExecutionsRequest`.
pub mod list_executions_request {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct ExecutionFunctionFilter {
        #[prost(oneof = "execution_function_filter::Scope", tags = "1, 2, 3")]
        pub scope: ::core::option::Option<execution_function_filter::Scope>,
    }
    /// Nested message and enum types in `ExecutionFunctionFilter`.
    pub mod execution_function_filter {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Scope {
            #[prost(string, tag = "1")]
            PackageName(::prost::alloc::string::String),
            #[prost(string, tag = "2")]
            InterfaceName(::prost::alloc::string::String),
            #[prost(string, tag = "3")]
            FunctionName(::prost::alloc::string::String),
        }
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Cursor {
        #[prost(oneof = "cursor::Cursor", tags = "1, 2")]
        pub cursor: ::core::option::Option<cursor::Cursor>,
    }
    /// Nested message and enum types in `Cursor`.
    pub mod cursor {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Cursor {
            #[prost(message, tag = "1")]
            ExecutionId(super::super::ExecutionId),
            #[prost(message, tag = "2")]
            CreatedAt(crate::grpc::wkt_types::Timestamp),
        }
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct NewerThan {
        #[prost(uint32, tag = "1")]
        pub length: u32,
        #[prost(message, optional, tag = "2")]
        pub cursor: ::core::option::Option<Cursor>,
        #[prost(bool, tag = "3")]
        pub including_cursor: bool,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct OlderThan {
        #[prost(uint32, tag = "1")]
        pub length: u32,
        #[prost(message, optional, tag = "2")]
        pub cursor: ::core::option::Option<Cursor>,
        #[prost(bool, tag = "3")]
        pub including_cursor: bool,
    }
    /// Filter by the current execution state, using the same buckets as `DeploymentSummary`.
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum ExecutionStateFilter {
        Unspecified = 0,
        Locked = 1,
        /// `PendingAt` with the scheduled time in the past.
        Pending = 2,
        /// `PendingAt` with the scheduled time in the future.
        Scheduled = 3,
        Blocked = 4,
        /// Finished with any result.
        Finished = 5,
        /// Paused regardless of the underlying state (locked, pending or blocked).
        Paused = 9,
        /// Cancellation requested; teardown in progress.
        Cancelling = 10,
        /// Finished successfully.
        FinishedOk = 6,
        /// Finished with the `err` variant of its result type.
        FinishedError = 7,
        /// Execution failure: trap, timeout, nondeterminism, cancellation etc.
        FinishedExecutionFailure = 8,
    }
    impl ExecutionStateFilter {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "EXECUTION_STATE_FILTER_UNSPECIFIED",
                Self::Locked => "EXECUTION_STATE_FILTER_LOCKED",
                Self::Pending => "EXECUTION_STATE_FILTER_PENDING",
                Self::Scheduled => "EXECUTION_STATE_FILTER_SCHEDULED",
                Self::Blocked => "EXECUTION_STATE_FILTER_BLOCKED",
                Self::Finished => "EXECUTION_STATE_FILTER_FINISHED",
                Self::Paused => "EXECUTION_STATE_FILTER_PAUSED",
                Self::Cancelling => "EXECUTION_STATE_FILTER_CANCELLING",
                Self::FinishedOk => "EXECUTION_STATE_FILTER_FINISHED_OK",
                Self::FinishedError => "EXECUTION_STATE_FILTER_FINISHED_ERROR",
                Self::FinishedExecutionFailure => {
                    "EXECUTION_STATE_FILTER_FINISHED_EXECUTION_FAILURE"
                }
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "EXECUTION_STATE_FILTER_UNSPECIFIED" => Some(Self::Unspecified),
                "EXECUTION_STATE_FILTER_LOCKED" => Some(Self::Locked),
                "EXECUTION_STATE_FILTER_PENDING" => Some(Self::Pending),
                "EXECUTION_STATE_FILTER_SCHEDULED" => Some(Self::Scheduled),
                "EXECUTION_STATE_FILTER_BLOCKED" => Some(Self::Blocked),
                "EXECUTION_STATE_FILTER_FINISHED" => Some(Self::Finished),
                "EXECUTION_STATE_FILTER_PAUSED" => Some(Self::Paused),
                "EXECUTION_STATE_FILTER_CANCELLING" => Some(Self::Cancelling),
                "EXECUTION_STATE_FILTER_FINISHED_OK" => Some(Self::FinishedOk),
                "EXECUTION_STATE_FILTER_FINISHED_ERROR" => Some(Self::FinishedError),
                "EXECUTION_STATE_FILTER_FINISHED_EXECUTION_FAILURE" => {
                    Some(Self::FinishedExecutionFailure)
                }
                _ => None,
            }
        }
    }
    /// latest by `created_at` if not set
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Pagination {
        #[prost(message, tag = "3")]
        NewerThan(NewerThan),
        #[prost(message, tag = "4")]
        OlderThan(OlderThan),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ExecutionSummary {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "2")]
    pub function_name: ::core::option::Option<FunctionName>,
    #[prost(message, optional, tag = "3")]
    pub current_status: ::core::option::Option<ExecutionStatus>,
    #[prost(message, optional, tag = "4")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "5")]
    pub first_scheduled_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "6")]
    pub component_digest: ::core::option::Option<ContentDigest>,
    #[prost(message, optional, tag = "7")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(enumeration = "ComponentType", tag = "8")]
    pub component_type: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListExecutionsResponse {
    #[prost(message, repeated, tag = "3")]
    pub executions: ::prost::alloc::vec::Vec<ExecutionSummary>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListComponentsRequest {
    /// All filters are applied if present (logical AND)
    #[prost(message, optional, tag = "1")]
    pub function_name: ::core::option::Option<FunctionName>,
    #[prost(message, optional, tag = "2")]
    pub component_digest: ::core::option::Option<ContentDigest>,
    #[prost(bool, tag = "3")]
    pub extensions: bool,
    #[prost(message, optional, tag = "4")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListComponentsResponse {
    #[prost(message, repeated, tag = "1")]
    pub components: ::prost::alloc::vec::Vec<Component>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Component {
    #[prost(message, optional, tag = "1")]
    pub component_id: ::core::option::Option<ComponentId>,
    /// Functions are sorted by their interface name
    #[prost(message, repeated, tag = "6")]
    pub exports: ::prost::alloc::vec::Vec<FunctionDetail>,
    /// Functions are sorted by their interface name
    #[prost(message, repeated, tag = "7")]
    pub imports: ::prost::alloc::vec::Vec<FunctionDetail>,
    #[prost(message, repeated, tag = "8")]
    pub files: ::prost::alloc::vec::Vec<ComponentFileRef>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ComponentFileRef {
    #[prost(message, optional, tag = "1")]
    pub file: ::core::option::Option<FileRef>,
    #[prost(enumeration = "ComponentFileRole", tag = "2")]
    pub role: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionDetail {
    #[prost(message, optional, tag = "1")]
    pub function_name: ::core::option::Option<FunctionName>,
    #[prost(message, repeated, tag = "2")]
    pub params: ::prost::alloc::vec::Vec<FunctionParameter>,
    #[prost(message, optional, tag = "3")]
    pub return_type: ::core::option::Option<WitType>,
    #[prost(enumeration = "FunctionExtension", optional, tag = "4")]
    pub extension: ::core::option::Option<i32>,
    #[prost(bool, tag = "5")]
    pub submittable: bool,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FunctionParameter {
    #[prost(message, optional, tag = "1")]
    pub r#type: ::core::option::Option<WitType>,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct WitType {
    /// WIT string as outputed by WIT printer. May contain external types
    #[prost(string, tag = "1")]
    pub wit_type: ::prost::alloc::string::String,
    /// Internal information about the type serialized as JSON
    /// deprecated
    #[prost(string, tag = "2")]
    pub type_wrapper: ::prost::alloc::string::String,
    /// WIT type without external references, e.g. `variant { first, second(string) }`
    #[prost(string, tag = "3")]
    pub wit_type_inline: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ExecutionId {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct RunId {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct JoinSetId {
    #[prost(enumeration = "join_set_id::JoinSetKind", tag = "2")]
    pub kind: i32,
    #[prost(string, tag = "3")]
    pub name: ::prost::alloc::string::String,
}
/// Nested message and enum types in `JoinSetId`.
pub mod join_set_id {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum JoinSetKind {
        Unspecified = 0,
        OneOff = 1,
        Named = 2,
        Generated = 3,
    }
    impl JoinSetKind {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "JOIN_SET_KIND_UNSPECIFIED",
                Self::OneOff => "JOIN_SET_KIND_ONE_OFF",
                Self::Named => "JOIN_SET_KIND_NAMED",
                Self::Generated => "JOIN_SET_KIND_GENERATED",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "JOIN_SET_KIND_UNSPECIFIED" => Some(Self::Unspecified),
                "JOIN_SET_KIND_ONE_OFF" => Some(Self::OneOff),
                "JOIN_SET_KIND_NAMED" => Some(Self::Named),
                "JOIN_SET_KIND_GENERATED" => Some(Self::Generated),
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DelayId {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
}
/// sha256 digest of the component
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ContentDigest {
    /// hex encoded with `sha256:` prefix.
    #[prost(string, tag = "1")]
    pub digest: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ComponentId {
    #[prost(enumeration = "ComponentType", tag = "1")]
    pub component_type: i32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub digest: ::core::option::Option<ContentDigest>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeploymentId {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FunctionName {
    /// `namespace:pkg_name/ifc_name` or `namespace:pkg_name/ifc_name@version`
    #[prost(string, tag = "1")]
    pub interface_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub function_name: ::prost::alloc::string::String,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ComponentRetryConfig {
    #[prost(uint32, optional, tag = "1")]
    pub max_retries: ::core::option::Option<u32>,
    #[prost(message, optional, tag = "2")]
    pub retry_exp_backoff: ::core::option::Option<crate::grpc::wkt_types::Duration>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SubmitRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "2")]
    pub function_name: ::core::option::Option<FunctionName>,
    #[prost(message, optional, tag = "3")]
    pub params: ::core::option::Option<crate::grpc::wkt_types::Any>,
    /// If true, create the execution in paused state so it won't be picked up
    /// by an executor until explicitly unpaused or advanced.
    #[prost(bool, tag = "4")]
    pub paused: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SubmitResponse {
    #[prost(enumeration = "submit_response::Outcome", tag = "1")]
    pub outcome: i32,
}
/// Nested message and enum types in `SubmitResponse`.
pub mod submit_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum Outcome {
        Unspecified = 0,
        Created = 1,
        ExistsWithSameParameters = 2,
    }
    impl Outcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "OUTCOME_UNSPECIFIED",
                Self::Created => "OUTCOME_CREATED",
                Self::ExistsWithSameParameters => "OUTCOME_EXISTS_WITH_SAME_PARAMETERS",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "OUTCOME_UNSPECIFIED" => Some(Self::Unspecified),
                "OUTCOME_CREATED" => Some(Self::Created),
                "OUTCOME_EXISTS_WITH_SAME_PARAMETERS" => {
                    Some(Self::ExistsWithSameParameters)
                }
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct StubRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "4")]
    pub return_value: ::core::option::Option<crate::grpc::wkt_types::Any>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct StubResponse {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetStatusRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(bool, tag = "2")]
    pub follow: bool,
    #[prost(bool, tag = "3")]
    pub send_finished_status: bool,
}
/// Corresponds to concise Pending State
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ExecutionStatus {
    #[prost(message, optional, tag = "5")]
    pub component_digest: ::core::option::Option<ContentDigest>,
    #[prost(oneof = "execution_status::Status", tags = "1, 2, 3, 4, 6, 7")]
    pub status: ::core::option::Option<execution_status::Status>,
}
/// Nested message and enum types in `ExecutionStatus`.
pub mod execution_status {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Locked {
        #[prost(message, optional, tag = "2")]
        pub run_id: ::core::option::Option<super::RunId>,
        #[prost(message, optional, tag = "3")]
        pub lock_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct PendingAt {
        #[prost(message, optional, tag = "1")]
        pub scheduled_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct BlockedByJoinSet {
        #[prost(message, optional, tag = "1")]
        pub join_set_id: ::core::option::Option<super::JoinSetId>,
        #[prost(message, optional, tag = "2")]
        pub lock_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(bool, tag = "3")]
        pub closing: bool,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Finished {
        #[prost(message, optional, tag = "4")]
        pub finished_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(message, optional, tag = "5")]
        pub result_kind: ::core::option::Option<super::ResultKind>,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Paused {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Cancelling {}
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Status {
        #[prost(message, tag = "1")]
        Locked(Locked),
        #[prost(message, tag = "2")]
        PendingAt(PendingAt),
        #[prost(message, tag = "3")]
        BlockedByJoinSet(BlockedByJoinSet),
        #[prost(message, tag = "4")]
        Finished(Finished),
        #[prost(message, tag = "6")]
        Paused(Paused),
        #[prost(message, tag = "7")]
        Cancelling(Cancelling),
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ResultKind {
    #[prost(oneof = "result_kind::Value", tags = "1, 2, 3")]
    pub value: ::core::option::Option<result_kind::Value>,
}
/// Nested message and enum types in `ResultKind`.
pub mod result_kind {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Ok {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Error {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        Ok(Ok),
        #[prost(message, tag = "2")]
        Error(Error),
        #[prost(enumeration = "super::ExecutionFailureKind", tag = "3")]
        ExecutionFailureKind(i32),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FinishedStatus {
    #[prost(message, optional, tag = "1")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "2")]
    pub scheduled_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "3")]
    pub finished_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "4")]
    pub value: ::core::option::Option<SupportedFunctionResult>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SupportedFunctionResult {
    #[prost(string, optional, tag = "5")]
    pub wit_type_inline: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(oneof = "supported_function_result::Value", tags = "1, 2, 4")]
    pub value: ::core::option::Option<supported_function_result::Value>,
}
/// Nested message and enum types in `SupportedFunctionResult`.
pub mod supported_function_result {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct OkPayload {
        #[prost(message, optional, tag = "1")]
        pub return_value: ::core::option::Option<crate::grpc::wkt_types::Any>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct ErrorPayload {
        #[prost(message, optional, tag = "1")]
        pub return_value: ::core::option::Option<crate::grpc::wkt_types::Any>,
    }
    /// FinishedExecutionFailure
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct ExecutionFailure {
        #[prost(enumeration = "super::ExecutionFailureKind", tag = "1")]
        pub kind: i32,
        #[prost(string, optional, tag = "2")]
        pub reason: ::core::option::Option<::prost::alloc::string::String>,
        #[prost(string, optional, tag = "3")]
        pub detail: ::core::option::Option<::prost::alloc::string::String>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        Ok(OkPayload),
        #[prost(message, tag = "2")]
        Error(ErrorPayload),
        #[prost(message, tag = "4")]
        ExecutionFailure(ExecutionFailure),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetStatusResponse {
    #[prost(oneof = "get_status_response::Message", tags = "1, 2, 3")]
    pub message: ::core::option::Option<get_status_response::Message>,
}
/// Nested message and enum types in `GetStatusResponse`.
pub mod get_status_response {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Message {
        /// First message in the stream
        #[prost(message, tag = "1")]
        Summary(super::ExecutionSummary),
        /// Remaining messages
        #[prost(message, tag = "2")]
        CurrentStatus(super::ExecutionStatus),
        /// Finished status, sent only if `send_finished_status` is set.
        #[prost(message, tag = "3")]
        FinishedStatus(super::FinishedStatus),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecutionEvent {
    #[prost(message, optional, tag = "1")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(uint32, tag = "2")]
    pub version: u32,
    #[prost(uint32, optional, tag = "10")]
    pub backtrace_id: ::core::option::Option<u32>,
    #[prost(
        oneof = "execution_event::Event",
        tags = "3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14"
    )]
    pub event: ::core::option::Option<execution_event::Event>,
}
/// Nested message and enum types in `ExecutionEvent`.
pub mod execution_event {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Created {
        #[prost(message, optional, tag = "1")]
        pub function_name: ::core::option::Option<super::FunctionName>,
        #[prost(message, optional, tag = "2")]
        pub params: ::core::option::Option<crate::grpc::wkt_types::Any>,
        #[prost(message, optional, tag = "3")]
        pub scheduled_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(message, optional, tag = "4")]
        pub component_id: ::core::option::Option<super::ComponentId>,
        #[prost(message, optional, tag = "5")]
        pub scheduled_by: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "6")]
        pub deployment_id: ::core::option::Option<super::DeploymentId>,
        #[prost(message, optional, tag = "7")]
        pub parent_execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "8")]
        pub parent_join_set_id: ::core::option::Option<super::JoinSetId>,
        #[prost(map = "string, string", tag = "9")]
        pub metadata: ::std::collections::HashMap<
            ::prost::alloc::string::String,
            ::prost::alloc::string::String,
        >,
        #[prost(uint64, tag = "10")]
        pub max_persisted_value_size_bytes: u64,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Locked {
        #[prost(message, optional, tag = "1")]
        pub component_id: ::core::option::Option<super::ComponentId>,
        #[prost(message, optional, tag = "2")]
        pub deployment_id: ::core::option::Option<super::DeploymentId>,
        #[prost(message, optional, tag = "3")]
        pub lock_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(string, tag = "4")]
        pub run_id: ::prost::alloc::string::String,
        #[prost(string, tag = "5")]
        pub executor_id: ::prost::alloc::string::String,
        #[prost(message, optional, tag = "6")]
        pub retry_config: ::core::option::Option<super::ComponentRetryConfig>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Unlocked {
        #[prost(message, optional, tag = "1")]
        pub backoff_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(string, tag = "2")]
        pub reason: ::prost::alloc::string::String,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct TemporarilyFailed {
        #[prost(string, tag = "1")]
        pub reason: ::prost::alloc::string::String,
        #[prost(string, optional, tag = "2")]
        pub detail: ::core::option::Option<::prost::alloc::string::String>,
        #[prost(message, optional, tag = "3")]
        pub backoff_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(message, repeated, tag = "4")]
        pub http_client_traces: ::prost::alloc::vec::Vec<super::HttpClientTrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct TemporarilyTimedOut {
        #[prost(message, optional, tag = "1")]
        pub backoff_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(message, repeated, tag = "2")]
        pub http_client_traces: ::prost::alloc::vec::Vec<super::HttpClientTrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Finished {
        #[prost(message, optional, tag = "1")]
        pub value: ::core::option::Option<super::SupportedFunctionResult>,
        #[prost(message, repeated, tag = "2")]
        pub http_client_traces: ::prost::alloc::vec::Vec<super::HttpClientTrace>,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Paused {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Unpaused {}
    /// Cancellation requested for a cancellable workflow. Sets the execution's
    /// lifecycle to cancelling without changing the underlying state.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct CancellationRequested {}
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct ComponentUpgradeFinished {
        #[prost(message, optional, tag = "1")]
        pub component_digest: ::core::option::Option<super::ContentDigest>,
        #[prost(message, optional, tag = "4")]
        pub deployment_id: ::core::option::Option<super::DeploymentId>,
        #[prost(oneof = "component_upgrade_finished::Outcome", tags = "2, 3")]
        pub outcome: ::core::option::Option<component_upgrade_finished::Outcome>,
    }
    /// Nested message and enum types in `ComponentUpgradeFinished`.
    pub mod component_upgrade_finished {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Auto {}
        #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Manual {
            #[prost(bool, tag = "1")]
            pub force: bool,
        }
        #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Success {
            #[prost(oneof = "success::Reason", tags = "1, 2")]
            pub reason: ::core::option::Option<success::Reason>,
        }
        /// Nested message and enum types in `Success`.
        pub mod success {
            #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
            pub enum Reason {
                #[prost(message, tag = "1")]
                Auto(super::Auto),
                #[prost(message, tag = "2")]
                Manual(super::Manual),
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Failed {
            #[prost(string, tag = "1")]
            pub reason: ::prost::alloc::string::String,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Outcome {
            #[prost(message, tag = "2")]
            Success(Success),
            #[prost(message, tag = "3")]
            Failed(Failed),
        }
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct HistoryEvent {
        #[prost(oneof = "history_event::Event", tags = "1, 2, 3, 4, 7, 8, 5, 6")]
        pub event: ::core::option::Option<history_event::Event>,
    }
    /// Nested message and enum types in `HistoryEvent`.
    pub mod history_event {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Persist {
            #[prost(message, optional, tag = "1")]
            pub data: ::core::option::Option<crate::grpc::wkt_types::Any>,
            #[prost(message, optional, tag = "2")]
            pub kind: ::core::option::Option<persist::PersistKind>,
            #[prost(string, tag = "3")]
            pub value_hash: ::prost::alloc::string::String,
        }
        /// Nested message and enum types in `Persist`.
        pub mod persist {
            #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct PersistKind {
                #[prost(oneof = "persist_kind::Variant", tags = "1, 2, 3")]
                pub variant: ::core::option::Option<persist_kind::Variant>,
            }
            /// Nested message and enum types in `PersistKind`.
            pub mod persist_kind {
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct RandomString {
                    #[prost(uint64, tag = "1")]
                    pub min_length: u64,
                    #[prost(uint64, tag = "2")]
                    pub max_length_exclusive: u64,
                }
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct RandomU64 {
                    #[prost(uint64, tag = "1")]
                    pub min: u64,
                    #[prost(uint64, tag = "2")]
                    pub max_inclusive: u64,
                }
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct ExecutionId {}
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
                pub enum Variant {
                    #[prost(message, tag = "1")]
                    RandomString(RandomString),
                    #[prost(message, tag = "2")]
                    RandomU64(RandomU64),
                    #[prost(message, tag = "3")]
                    ExecutionId(ExecutionId),
                }
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct JoinSetCreated {
            #[prost(message, optional, tag = "1")]
            pub join_set_id: ::core::option::Option<super::super::JoinSetId>,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct JoinSetRequest {
            #[prost(message, optional, tag = "1")]
            pub join_set_id: ::core::option::Option<super::super::JoinSetId>,
            #[prost(oneof = "join_set_request::JoinSetRequest", tags = "2, 3")]
            pub join_set_request: ::core::option::Option<
                join_set_request::JoinSetRequest,
            >,
        }
        /// Nested message and enum types in `JoinSetRequest`.
        pub mod join_set_request {
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct DelayRequest {
                #[prost(message, optional, tag = "1")]
                pub delay_id: ::core::option::Option<super::super::super::DelayId>,
                #[prost(message, optional, tag = "2")]
                pub expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
                #[prost(message, optional, tag = "3")]
                pub scheduled_at: ::core::option::Option<delay_request::ScheduledAt>,
                #[prost(bool, tag = "4")]
                pub paused: bool,
            }
            /// Nested message and enum types in `DelayRequest`.
            pub mod delay_request {
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct ScheduledAt {
                    #[prost(oneof = "scheduled_at::Variant", tags = "1, 2, 3")]
                    pub variant: ::core::option::Option<scheduled_at::Variant>,
                }
                /// Nested message and enum types in `ScheduledAt`.
                pub mod scheduled_at {
                    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                    pub struct Now {}
                    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                    pub struct At {
                        #[prost(message, optional, tag = "1")]
                        pub at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
                    }
                    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                    pub struct In {
                        #[prost(message, optional, tag = "1")]
                        pub r#in: ::core::option::Option<crate::grpc::wkt_types::Duration>,
                    }
                    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
                    pub enum Variant {
                        #[prost(message, tag = "1")]
                        Now(Now),
                        #[prost(message, tag = "2")]
                        At(At),
                        #[prost(message, tag = "3")]
                        In(In),
                    }
                }
            }
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct ChildExecutionRequest {
                #[prost(message, optional, tag = "1")]
                pub child_execution_id: ::core::option::Option<
                    super::super::super::ExecutionId,
                >,
                #[prost(message, optional, tag = "2")]
                pub function_name: ::core::option::Option<
                    super::super::super::FunctionName,
                >,
                #[prost(message, optional, tag = "3")]
                pub params: ::core::option::Option<crate::grpc::wkt_types::Any>,
                #[prost(message, optional, tag = "6")]
                pub rejected_params: ::core::option::Option<
                    child_execution_request::RejectedParams,
                >,
                /// backcompat: Empty for history written by version 0.41.
                #[prost(string, tag = "7")]
                pub params_hash: ::prost::alloc::string::String,
                #[prost(oneof = "child_execution_request::Result", tags = "4, 5")]
                pub result: ::core::option::Option<child_execution_request::Result>,
            }
            /// Nested message and enum types in `ChildExecutionRequest`.
            pub mod child_execution_request {
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct RejectedParams {
                    #[prost(uint64, tag = "2")]
                    pub encoded_size_at_least: u64,
                }
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct Ok {}
                #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct Error {
                    #[prost(enumeration = "error::Kind", tag = "1")]
                    pub kind: i32,
                    #[prost(string, optional, tag = "2")]
                    pub detail: ::core::option::Option<::prost::alloc::string::String>,
                }
                /// Nested message and enum types in `Error`.
                pub mod error {
                    #[derive(
                        Clone,
                        Copy,
                        Debug,
                        PartialEq,
                        Eq,
                        Hash,
                        PartialOrd,
                        Ord,
                        ::prost::Enumeration
                    )]
                    #[repr(i32)]
                    pub enum Kind {
                        Unspecified = 0,
                        FunctionNotFound = 1,
                        TypeCheckError = 2,
                        ValueTooLarge = 3,
                    }
                    impl Kind {
                        /// String value of the enum field names used in the ProtoBuf definition.
                        ///
                        /// The values are not transformed in any way and thus are considered stable
                        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
                        pub fn as_str_name(&self) -> &'static str {
                            match self {
                                Self::Unspecified => "KIND_UNSPECIFIED",
                                Self::FunctionNotFound => "KIND_FUNCTION_NOT_FOUND",
                                Self::TypeCheckError => "KIND_TYPE_CHECK_ERROR",
                                Self::ValueTooLarge => "KIND_VALUE_TOO_LARGE",
                            }
                        }
                        /// Creates an enum from field names used in the ProtoBuf definition.
                        pub fn from_str_name(
                            value: &str,
                        ) -> ::core::option::Option<Self> {
                            match value {
                                "KIND_UNSPECIFIED" => Some(Self::Unspecified),
                                "KIND_FUNCTION_NOT_FOUND" => Some(Self::FunctionNotFound),
                                "KIND_TYPE_CHECK_ERROR" => Some(Self::TypeCheckError),
                                "KIND_VALUE_TOO_LARGE" => Some(Self::ValueTooLarge),
                                _ => None,
                            }
                        }
                    }
                }
                #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
                pub enum Result {
                    #[prost(message, tag = "4")]
                    Ok(Ok),
                    #[prost(message, tag = "5")]
                    Error(Error),
                }
            }
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
            pub enum JoinSetRequest {
                #[prost(message, tag = "2")]
                DelayRequest(DelayRequest),
                #[prost(message, tag = "3")]
                ChildExecutionRequest(ChildExecutionRequest),
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct JoinNext {
            #[prost(message, optional, tag = "1")]
            pub join_set_id: ::core::option::Option<super::super::JoinSetId>,
            #[prost(message, optional, tag = "2")]
            pub run_expires_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
            #[prost(bool, tag = "3")]
            pub closing: bool,
            #[prost(message, optional, tag = "4")]
            pub function: ::core::option::Option<super::super::FunctionName>,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct JoinNextTooMany {
            #[prost(message, optional, tag = "1")]
            pub join_set_id: ::core::option::Option<super::super::JoinSetId>,
            #[prost(message, optional, tag = "2")]
            pub function: ::core::option::Option<super::super::FunctionName>,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct JoinNextTry {
            #[prost(message, optional, tag = "1")]
            pub join_set_id: ::core::option::Option<super::super::JoinSetId>,
            #[prost(enumeration = "join_next_try::Outcome", tag = "3")]
            pub outcome: i32,
        }
        /// Nested message and enum types in `JoinNextTry`.
        pub mod join_next_try {
            #[derive(
                Clone,
                Copy,
                Debug,
                PartialEq,
                Eq,
                Hash,
                PartialOrd,
                Ord,
                ::prost::Enumeration
            )]
            #[repr(i32)]
            pub enum Outcome {
                Found = 0,
                Pending = 1,
                AllProcessed = 2,
            }
            impl Outcome {
                /// String value of the enum field names used in the ProtoBuf definition.
                ///
                /// The values are not transformed in any way and thus are considered stable
                /// (if the ProtoBuf definition does not change) and safe for programmatic use.
                pub fn as_str_name(&self) -> &'static str {
                    match self {
                        Self::Found => "FOUND",
                        Self::Pending => "PENDING",
                        Self::AllProcessed => "ALL_PROCESSED",
                    }
                }
                /// Creates an enum from field names used in the ProtoBuf definition.
                pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                    match value {
                        "FOUND" => Some(Self::Found),
                        "PENDING" => Some(Self::Pending),
                        "ALL_PROCESSED" => Some(Self::AllProcessed),
                        _ => None,
                    }
                }
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Schedule {
            #[prost(message, optional, tag = "1")]
            pub execution_id: ::core::option::Option<super::super::ExecutionId>,
            #[prost(message, optional, tag = "2")]
            pub scheduled_at: ::core::option::Option<schedule::ScheduledAt>,
            #[prost(string, tag = "5")]
            pub params_hash: ::prost::alloc::string::String,
            #[prost(oneof = "schedule::Result", tags = "3, 4")]
            pub result: ::core::option::Option<schedule::Result>,
        }
        /// Nested message and enum types in `Schedule`.
        pub mod schedule {
            #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct Ok {}
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct Error {
                #[prost(enumeration = "error::Kind", tag = "1")]
                pub kind: i32,
                #[prost(string, optional, tag = "2")]
                pub detail: ::core::option::Option<::prost::alloc::string::String>,
            }
            /// Nested message and enum types in `Error`.
            pub mod error {
                #[derive(
                    Clone,
                    Copy,
                    Debug,
                    PartialEq,
                    Eq,
                    Hash,
                    PartialOrd,
                    Ord,
                    ::prost::Enumeration
                )]
                #[repr(i32)]
                pub enum Kind {
                    Unspecified = 0,
                    FunctionNotFound = 1,
                    TypeCheckError = 2,
                    ValueTooLarge = 3,
                }
                impl Kind {
                    /// String value of the enum field names used in the ProtoBuf definition.
                    ///
                    /// The values are not transformed in any way and thus are considered stable
                    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
                    pub fn as_str_name(&self) -> &'static str {
                        match self {
                            Self::Unspecified => "KIND_UNSPECIFIED",
                            Self::FunctionNotFound => "KIND_FUNCTION_NOT_FOUND",
                            Self::TypeCheckError => "KIND_TYPE_CHECK_ERROR",
                            Self::ValueTooLarge => "KIND_VALUE_TOO_LARGE",
                        }
                    }
                    /// Creates an enum from field names used in the ProtoBuf definition.
                    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                        match value {
                            "KIND_UNSPECIFIED" => Some(Self::Unspecified),
                            "KIND_FUNCTION_NOT_FOUND" => Some(Self::FunctionNotFound),
                            "KIND_TYPE_CHECK_ERROR" => Some(Self::TypeCheckError),
                            "KIND_VALUE_TOO_LARGE" => Some(Self::ValueTooLarge),
                            _ => None,
                        }
                    }
                }
            }
            #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct ScheduledAt {
                #[prost(oneof = "scheduled_at::Variant", tags = "1, 2, 3")]
                pub variant: ::core::option::Option<scheduled_at::Variant>,
            }
            /// Nested message and enum types in `ScheduledAt`.
            pub mod scheduled_at {
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct Now {}
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct At {
                    #[prost(message, optional, tag = "1")]
                    pub at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
                }
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
                pub struct In {
                    #[prost(message, optional, tag = "1")]
                    pub r#in: ::core::option::Option<crate::grpc::wkt_types::Duration>,
                }
                #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
                pub enum Variant {
                    #[prost(message, tag = "1")]
                    Now(Now),
                    #[prost(message, tag = "2")]
                    At(At),
                    #[prost(message, tag = "3")]
                    In(In),
                }
            }
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
            pub enum Result {
                #[prost(message, tag = "3")]
                Ok(Ok),
                #[prost(message, tag = "4")]
                Error(Error),
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct Stub {
            #[prost(message, optional, tag = "1")]
            pub execution_id: ::core::option::Option<super::super::ExecutionId>,
            #[prost(string, tag = "2")]
            pub retval_hash: ::prost::alloc::string::String,
            #[prost(oneof = "stub::Result", tags = "3, 4")]
            pub result: ::core::option::Option<stub::Result>,
        }
        /// Nested message and enum types in `Stub`.
        pub mod stub {
            #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct Ok {}
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
            pub struct Error {
                #[prost(enumeration = "error::Kind", tag = "1")]
                pub kind: i32,
                #[prost(string, optional, tag = "2")]
                pub detail: ::core::option::Option<::prost::alloc::string::String>,
            }
            /// Nested message and enum types in `Error`.
            pub mod error {
                #[derive(
                    Clone,
                    Copy,
                    Debug,
                    PartialEq,
                    Eq,
                    Hash,
                    PartialOrd,
                    Ord,
                    ::prost::Enumeration
                )]
                #[repr(i32)]
                pub enum Kind {
                    Unspecified = 0,
                    ExecutionNotFound = 1,
                    TypeCheckError = 2,
                    Conflict = 3,
                    ValueTooLarge = 4,
                }
                impl Kind {
                    /// String value of the enum field names used in the ProtoBuf definition.
                    ///
                    /// The values are not transformed in any way and thus are considered stable
                    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
                    pub fn as_str_name(&self) -> &'static str {
                        match self {
                            Self::Unspecified => "KIND_UNSPECIFIED",
                            Self::ExecutionNotFound => "KIND_EXECUTION_NOT_FOUND",
                            Self::TypeCheckError => "KIND_TYPE_CHECK_ERROR",
                            Self::Conflict => "KIND_CONFLICT",
                            Self::ValueTooLarge => "KIND_VALUE_TOO_LARGE",
                        }
                    }
                    /// Creates an enum from field names used in the ProtoBuf definition.
                    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                        match value {
                            "KIND_UNSPECIFIED" => Some(Self::Unspecified),
                            "KIND_EXECUTION_NOT_FOUND" => Some(Self::ExecutionNotFound),
                            "KIND_TYPE_CHECK_ERROR" => Some(Self::TypeCheckError),
                            "KIND_CONFLICT" => Some(Self::Conflict),
                            "KIND_VALUE_TOO_LARGE" => Some(Self::ValueTooLarge),
                            _ => None,
                        }
                    }
                }
            }
            #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
            pub enum Result {
                #[prost(message, tag = "3")]
                Ok(Ok),
                #[prost(message, tag = "4")]
                Error(Error),
            }
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Event {
            #[prost(message, tag = "1")]
            Persist(Persist),
            #[prost(message, tag = "2")]
            JoinSetCreated(JoinSetCreated),
            #[prost(message, tag = "3")]
            JoinSetRequest(JoinSetRequest),
            #[prost(message, tag = "4")]
            JoinNext(JoinNext),
            #[prost(message, tag = "7")]
            JoinNextTooMany(JoinNextTooMany),
            #[prost(message, tag = "8")]
            JoinNextTry(JoinNextTry),
            #[prost(message, tag = "5")]
            Schedule(Schedule),
            #[prost(message, tag = "6")]
            Stub(Stub),
        }
    }
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Event {
        #[prost(message, tag = "3")]
        Created(Created),
        #[prost(message, tag = "4")]
        Locked(Locked),
        #[prost(message, tag = "5")]
        Unlocked(Unlocked),
        #[prost(message, tag = "6")]
        TemporarilyFailed(TemporarilyFailed),
        #[prost(message, tag = "7")]
        TemporarilyTimedOut(TemporarilyTimedOut),
        #[prost(message, tag = "8")]
        Finished(Finished),
        #[prost(message, tag = "9")]
        HistoryVariant(HistoryEvent),
        #[prost(message, tag = "11")]
        Paused(Paused),
        #[prost(message, tag = "12")]
        Unpaused(Unpaused),
        #[prost(message, tag = "13")]
        ComponentUpgradeFinished(ComponentUpgradeFinished),
        #[prost(message, tag = "14")]
        CancellationRequested(CancellationRequested),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct HttpClientTrace {
    #[prost(message, optional, tag = "1")]
    pub sent_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(string, tag = "2")]
    pub uri: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub method: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub finished_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    /// present iif finished_at is sent.
    #[prost(oneof = "http_client_trace::Result", tags = "5, 6")]
    pub result: ::core::option::Option<http_client_trace::Result>,
}
/// Nested message and enum types in `HttpClientTrace`.
pub mod http_client_trace {
    /// present iif finished_at is sent.
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Result {
        #[prost(uint32, tag = "5")]
        Status(u32),
        #[prost(string, tag = "6")]
        Error(::prost::alloc::string::String),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListExecutionEventsRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    /// 0 to request the first page
    #[prost(uint32, tag = "2")]
    pub version_from: u32,
    #[prost(uint32, tag = "3")]
    pub length: u32,
    /// return version_min_including if a matching backtrace is found
    #[prost(bool, tag = "4")]
    pub include_backtrace_id: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListExecutionEventsResponse {
    #[prost(message, repeated, tag = "2")]
    pub events: ::prost::alloc::vec::Vec<ExecutionEvent>,
    #[prost(uint32, tag = "3")]
    pub max_version: u32,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct JoinSetResponseEvent {
    #[prost(message, optional, tag = "1")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "2")]
    pub join_set_id: ::core::option::Option<JoinSetId>,
    #[prost(oneof = "join_set_response_event::Response", tags = "3, 4")]
    pub response: ::core::option::Option<join_set_response_event::Response>,
}
/// Nested message and enum types in `JoinSetResponseEvent`.
pub mod join_set_response_event {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct DelayFinished {
        #[prost(message, optional, tag = "1")]
        pub delay_id: ::core::option::Option<super::DelayId>,
        /// Set to `false` if delay was cancelled.
        #[prost(bool, tag = "2")]
        pub success: bool,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct ChildExecutionFinished {
        #[prost(message, optional, tag = "1")]
        pub child_execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "2")]
        pub value: ::core::option::Option<super::SupportedFunctionResult>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Response {
        #[prost(message, tag = "3")]
        DelayFinished(DelayFinished),
        #[prost(message, tag = "4")]
        ChildExecutionFinished(ChildExecutionFinished),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ResponseWithCursor {
    #[prost(message, optional, tag = "1")]
    pub event: ::core::option::Option<JoinSetResponseEvent>,
    #[prost(uint32, tag = "2")]
    pub cursor: u32,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListResponsesRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    /// 0 to request the first page
    #[prost(uint32, tag = "2")]
    pub cursor_from: u32,
    #[prost(uint32, tag = "3")]
    pub length: u32,
    /// true to request the first page
    #[prost(bool, tag = "4")]
    pub including_cursor: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListResponsesResponse {
    #[prost(message, repeated, tag = "1")]
    pub responses: ::prost::alloc::vec::Vec<ResponseWithCursor>,
    #[prost(uint32, tag = "2")]
    pub max_cursor: u32,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListExecutionEventsAndResponsesRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    /// 0 to request the first page of execution events
    #[prost(uint32, tag = "2")]
    pub version_from: u32,
    #[prost(uint32, tag = "3")]
    pub events_length: u32,
    /// 0 to request the first page of responses
    #[prost(uint32, tag = "4")]
    pub responses_cursor_from: u32,
    #[prost(uint32, tag = "5")]
    pub responses_length: u32,
    /// true to request the first page of responses
    #[prost(bool, tag = "6")]
    pub responses_including_cursor: bool,
    /// return version_min_including if a matching backtrace is found
    #[prost(bool, tag = "7")]
    pub include_backtrace_id: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListExecutionEventsAndResponsesResponse {
    #[prost(message, repeated, tag = "1")]
    pub events: ::prost::alloc::vec::Vec<ExecutionEvent>,
    #[prost(message, repeated, tag = "2")]
    pub responses: ::prost::alloc::vec::Vec<ResponseWithCursor>,
    #[prost(message, optional, tag = "3")]
    pub current_status: ::core::option::Option<ExecutionStatus>,
    #[prost(uint32, tag = "4")]
    pub max_version: u32,
    #[prost(uint32, tag = "5")]
    pub max_cursor: u32,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetWitRequest {
    #[prost(message, optional, tag = "1")]
    pub component_digest: ::core::option::Option<ContentDigest>,
    #[prost(message, optional, tag = "2")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetWitResponse {
    #[prost(string, optional, tag = "1")]
    pub content: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetBacktraceRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(oneof = "get_backtrace_request::Filter", tags = "2, 3, 4")]
    pub filter: ::core::option::Option<get_backtrace_request::Filter>,
}
/// Nested message and enum types in `GetBacktraceRequest`.
pub mod get_backtrace_request {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct First {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Last {}
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Specific {
        #[prost(uint32, tag = "1")]
        pub version: u32,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Filter {
        #[prost(message, tag = "2")]
        First(First),
        #[prost(message, tag = "3")]
        Last(Last),
        #[prost(message, tag = "4")]
        Specific(Specific),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetBacktraceResponse {
    #[prost(message, optional, tag = "1")]
    pub component_id: ::core::option::Option<ComponentId>,
    #[prost(message, optional, tag = "2")]
    pub wasm_backtrace: ::core::option::Option<WasmBacktrace>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WasmBacktrace {
    #[prost(message, repeated, tag = "1")]
    pub frames: ::prost::alloc::vec::Vec<FrameInfo>,
    #[prost(uint32, tag = "2")]
    pub version_min_including: u32,
    #[prost(uint32, tag = "3")]
    pub version_max_excluding: u32,
}
/// Display-only call-site backtrace attached to a captured write.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CapturedBacktrace {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "2")]
    pub component_id: ::core::option::Option<ComponentId>,
    #[prost(message, optional, tag = "3")]
    pub wasm_backtrace: ::core::option::Option<WasmBacktrace>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FrameInfo {
    #[prost(string, tag = "1")]
    pub module: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub func_name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub symbols: ::prost::alloc::vec::Vec<FrameSymbol>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FrameSymbol {
    #[prost(string, optional, tag = "1")]
    pub func_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "2")]
    pub file: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "3")]
    pub line: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "4")]
    pub col: ::core::option::Option<u32>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetBacktraceSourceRequest {
    #[prost(message, optional, tag = "1")]
    pub component_id: ::core::option::Option<ComponentId>,
    /// As appears in FrameSymbol
    #[prost(string, tag = "2")]
    pub file: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetBacktraceSourceResponse {
    #[prost(string, tag = "1")]
    pub content: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CancelExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CancelExecutionResponse {
    #[prost(
        enumeration = "cancel_execution_response::CancelExecutionOutcome",
        tag = "1"
    )]
    pub outcome: i32,
}
/// Nested message and enum types in `CancelExecutionResponse`.
pub mod cancel_execution_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum CancelExecutionOutcome {
        Unspecified = 0,
        /// Cancellation successfully requested. Activity termination is best effort; a
        /// cancellable workflow moves to cancelling and is completed by the
        /// cancellation driver.
        CancellationRequested = 1,
        AlreadyFinished = 2,
        AlreadyCancelling = 3,
    }
    impl CancelExecutionOutcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "CANCEL_EXECUTION_OUTCOME_UNSPECIFIED",
                Self::CancellationRequested => {
                    "CANCEL_EXECUTION_OUTCOME_CANCELLATION_REQUESTED"
                }
                Self::AlreadyFinished => "CANCEL_EXECUTION_OUTCOME_ALREADY_FINISHED",
                Self::AlreadyCancelling => "CANCEL_EXECUTION_OUTCOME_ALREADY_CANCELLING",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "CANCEL_EXECUTION_OUTCOME_UNSPECIFIED" => Some(Self::Unspecified),
                "CANCEL_EXECUTION_OUTCOME_CANCELLATION_REQUESTED" => {
                    Some(Self::CancellationRequested)
                }
                "CANCEL_EXECUTION_OUTCOME_ALREADY_FINISHED" => {
                    Some(Self::AlreadyFinished)
                }
                "CANCEL_EXECUTION_OUTCOME_ALREADY_CANCELLING" => {
                    Some(Self::AlreadyCancelling)
                }
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CancelDelayRequest {
    #[prost(message, optional, tag = "1")]
    pub delay_id: ::core::option::Option<DelayId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CancelDelayResponse {
    #[prost(enumeration = "cancel_delay_response::CancelDelayOutcome", tag = "1")]
    pub outcome: i32,
}
/// Nested message and enum types in `CancelDelayResponse`.
pub mod cancel_delay_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum CancelDelayOutcome {
        Unspecified = 0,
        Cancelled = 1,
        AlreadyFinished = 2,
    }
    impl CancelDelayOutcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "CANCEL_DELAY_OUTCOME_UNSPECIFIED",
                Self::Cancelled => "CANCEL_DELAY_OUTCOME_CANCELLED",
                Self::AlreadyFinished => "CANCEL_DELAY_OUTCOME_ALREADY_FINISHED",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "CANCEL_DELAY_OUTCOME_UNSPECIFIED" => Some(Self::Unspecified),
                "CANCEL_DELAY_OUTCOME_CANCELLED" => Some(Self::Cancelled),
                "CANCEL_DELAY_OUTCOME_ALREADY_FINISHED" => Some(Self::AlreadyFinished),
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ReplayExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
/// A write captured during replay, produced by ReplayExecution and applied by AdvanceExecution.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CapturedWrite {
    #[prost(oneof = "captured_write::Write", tags = "1, 2, 3, 4, 5, 6")]
    pub write: ::core::option::Option<captured_write::Write>,
}
/// Nested message and enum types in `CapturedWrite`.
pub mod captured_write {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Append {
        #[prost(message, optional, tag = "1")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "2")]
        pub version: u32,
        #[prost(message, optional, tag = "3")]
        pub event: ::core::option::Option<super::ExecutionEvent>,
        #[prost(message, repeated, tag = "4")]
        pub backtraces: ::prost::alloc::vec::Vec<super::CapturedBacktrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AppendBatch {
        #[prost(message, repeated, tag = "2")]
        pub events: ::prost::alloc::vec::Vec<super::ExecutionEvent>,
        #[prost(message, optional, tag = "3")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "4")]
        pub version: u32,
        #[prost(message, repeated, tag = "5")]
        pub backtraces: ::prost::alloc::vec::Vec<super::CapturedBacktrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AppendBatchCreateNewExecution {
        #[prost(message, repeated, tag = "2")]
        pub events: ::prost::alloc::vec::Vec<super::ExecutionEvent>,
        #[prost(message, optional, tag = "3")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "4")]
        pub version: u32,
        #[prost(message, repeated, tag = "5")]
        pub child_requests: ::prost::alloc::vec::Vec<super::CreateExecutionRequest>,
        #[prost(message, repeated, tag = "6")]
        pub backtraces: ::prost::alloc::vec::Vec<super::CapturedBacktrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AppendBatchWithDelayResponse {
        #[prost(message, repeated, tag = "1")]
        pub events: ::prost::alloc::vec::Vec<super::ExecutionEvent>,
        #[prost(message, optional, tag = "2")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "3")]
        pub version: u32,
        #[prost(message, optional, tag = "4")]
        pub join_set_id: ::core::option::Option<super::JoinSetId>,
        #[prost(string, tag = "5")]
        pub delay_id: ::prost::alloc::string::String,
        #[prost(message, repeated, tag = "6")]
        pub backtraces: ::prost::alloc::vec::Vec<super::CapturedBacktrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AppendStubResponse {
        #[prost(message, optional, tag = "1")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "2")]
        pub version: u32,
        #[prost(message, repeated, tag = "3")]
        pub events: ::prost::alloc::vec::Vec<super::ExecutionEvent>,
        #[prost(message, optional, tag = "4")]
        pub parent_execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "5")]
        pub join_set_id: ::core::option::Option<super::JoinSetId>,
        #[prost(message, optional, tag = "6")]
        pub child_execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "7")]
        pub result: ::core::option::Option<super::SupportedFunctionResult>,
        #[prost(uint32, tag = "10")]
        pub finished_version: u32,
        #[prost(message, repeated, tag = "11")]
        pub backtraces: ::prost::alloc::vec::Vec<super::CapturedBacktrace>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AppendFinished {
        #[prost(message, optional, tag = "1")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(uint32, tag = "2")]
        pub version: u32,
        #[prost(message, optional, tag = "3")]
        pub event: ::core::option::Option<super::execution_event::Finished>,
        #[prost(message, optional, tag = "4")]
        pub parent_execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(message, optional, tag = "5")]
        pub parent_join_set_id: ::core::option::Option<super::JoinSetId>,
    }
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Write {
        #[prost(message, tag = "1")]
        Append(Append),
        #[prost(message, tag = "2")]
        AppendBatch(AppendBatch),
        #[prost(message, tag = "3")]
        AppendBatchCreateNewExecution(AppendBatchCreateNewExecution),
        #[prost(message, tag = "4")]
        AppendStubResponse(AppendStubResponse),
        #[prost(message, tag = "5")]
        AppendFinished(AppendFinished),
        #[prost(message, tag = "6")]
        AppendBatchWithDelayResponse(AppendBatchWithDelayResponse),
    }
}
/// Child execution creation request (part of CapturedWrite).
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "2")]
    pub function_name: ::core::option::Option<FunctionName>,
    #[prost(message, optional, tag = "3")]
    pub params: ::core::option::Option<crate::grpc::wkt_types::Any>,
    #[prost(message, optional, tag = "4")]
    pub scheduled_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "5")]
    pub component_id: ::core::option::Option<ComponentId>,
    #[prost(message, optional, tag = "6")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(message, optional, tag = "7")]
    pub parent_execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "8")]
    pub parent_join_set_id: ::core::option::Option<JoinSetId>,
    #[prost(message, optional, tag = "9")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(map = "string, string", tag = "10")]
    pub metadata: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::string::String,
    >,
    #[prost(bool, tag = "11")]
    pub paused: bool,
    #[prost(message, optional, tag = "12")]
    pub scheduled_by: ::core::option::Option<ExecutionId>,
    #[prost(uint64, tag = "13")]
    pub max_persisted_value_size_bytes: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReplayExecutionResponse {
    /// Number of persisted history events supplied to the workflow replay.
    #[prost(uint64, tag = "5")]
    pub replayed_event_count: u64,
    /// Time spent executing the workflow replay after loading its execution log.
    #[prost(message, optional, tag = "6")]
    pub replay_duration: ::core::option::Option<crate::grpc::wkt_types::Duration>,
    /// Highest persisted execution-event version included in the replay log. Response records use a
    /// separate cursor and are not represented by this version.
    #[prost(uint32, tag = "7")]
    pub replay_version: u32,
    #[prost(oneof = "replay_execution_response::Outcome", tags = "1, 2, 3, 4")]
    pub outcome: ::core::option::Option<replay_execution_response::Outcome>,
}
/// Nested message and enum types in `ReplayExecutionResponse`.
pub mod replay_execution_response {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Advanceable {
        #[prost(message, repeated, tag = "1")]
        pub captured_writes: ::prost::alloc::vec::Vec<super::CapturedWrite>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Finished {
        #[prost(message, optional, tag = "1")]
        pub result: ::core::option::Option<super::SupportedFunctionResult>,
    }
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Blocked {}
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ReplayFailed {
        #[prost(string, tag = "1")]
        pub error: ::prost::alloc::string::String,
        #[prost(message, repeated, tag = "2")]
        pub captured_writes: ::prost::alloc::vec::Vec<super::CapturedWrite>,
        /// Structured, sanitized reason for the replay failure.
        #[prost(message, optional, tag = "3")]
        pub failure: ::core::option::Option<
            super::supported_function_result::ExecutionFailure,
        >,
    }
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Outcome {
        #[prost(message, tag = "1")]
        Advanceable(Advanceable),
        #[prost(message, tag = "2")]
        Finished(Finished),
        #[prost(message, tag = "3")]
        Blocked(Blocked),
        #[prost(message, tag = "4")]
        ReplayFailed(ReplayFailed),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PersistExecutionBacktracesRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PersistExecutionBacktracesResponse {
    #[prost(uint32, tag = "1")]
    pub persisted_backtrace_count: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AdvanceExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, repeated, tag = "2")]
    pub captured_writes: ::prost::alloc::vec::Vec<CapturedWrite>,
    /// Capture and persist fresh call-site backtraces while verifying and applying the writes.
    #[prost(bool, tag = "3")]
    pub persist_backtrace: bool,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct AdvanceExecutionResponse {
    #[prost(oneof = "advance_execution_response::Result", tags = "1, 2")]
    pub result: ::core::option::Option<advance_execution_response::Result>,
}
/// Nested message and enum types in `AdvanceExecutionResponse`.
pub mod advance_execution_response {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Success {
        #[prost(message, optional, tag = "1")]
        pub finished: ::core::option::Option<super::SupportedFunctionResult>,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct Error {
        #[prost(oneof = "error::Error", tags = "3, 4, 5")]
        pub error: ::core::option::Option<error::Error>,
    }
    /// Nested message and enum types in `Error`.
    pub mod error {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct VersionMismatch {
            #[prost(uint32, tag = "1")]
            pub expected: u32,
        }
        #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct ReplayMismatch {}
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct TransientError {
            #[prost(string, tag = "1")]
            pub message: ::prost::alloc::string::String,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Error {
            #[prost(message, tag = "3")]
            VersionMismatch(VersionMismatch),
            #[prost(message, tag = "4")]
            ReplayMismatch(ReplayMismatch),
            #[prost(message, tag = "5")]
            TransientError(TransientError),
        }
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Success(Success),
        #[prost(message, tag = "2")]
        Error(Error),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UpgradeExecutionComponentRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    /// old
    #[prost(message, optional, tag = "2")]
    pub expected_component_digest: ::core::option::Option<ContentDigest>,
    /// new
    #[prost(message, optional, tag = "3")]
    pub new_component_digest: ::core::option::Option<ContentDigest>,
    #[prost(bool, tag = "4")]
    pub skip_determinism_check: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UpgradeExecutionComponentResponse {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListLogsRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(int32, tag = "2")]
    pub page_size: i32,
    #[prost(string, tag = "3")]
    pub page_token: ::prost::alloc::string::String,
    #[prost(bool, tag = "4")]
    pub show_logs: bool,
    #[prost(bool, tag = "5")]
    pub show_streams: bool,
    /// Only applied if show_logs = true, empty means return all levels.
    #[prost(enumeration = "LogLevel", repeated, tag = "6")]
    pub levels: ::prost::alloc::vec::Vec<i32>,
    /// Only applied if show_streams = true, empty means return all stream types.
    #[prost(enumeration = "LogStreamType", repeated, tag = "7")]
    pub stream_types: ::prost::alloc::vec::Vec<i32>,
    #[prost(bool, tag = "8")]
    pub show_derived: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListLogsResponse {
    #[prost(message, repeated, tag = "1")]
    pub logs: ::prost::alloc::vec::Vec<list_logs_response::LogEntry>,
    #[prost(string, tag = "2")]
    pub next_page_token: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "3")]
    pub prev_page_token: ::core::option::Option<::prost::alloc::string::String>,
}
/// Nested message and enum types in `ListLogsResponse`.
pub mod list_logs_response {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct LogEntry {
        #[prost(message, optional, tag = "1")]
        pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
        #[prost(message, optional, tag = "4")]
        pub run_id: ::core::option::Option<super::RunId>,
        #[prost(message, optional, tag = "5")]
        pub execution_id: ::core::option::Option<super::ExecutionId>,
        #[prost(oneof = "log_entry::Entry", tags = "2, 3")]
        pub entry: ::core::option::Option<log_entry::Entry>,
    }
    /// Nested message and enum types in `LogEntry`.
    pub mod log_entry {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct LogVariant {
            #[prost(enumeration = "super::super::LogLevel", tag = "1")]
            pub level: i32,
            #[prost(string, tag = "2")]
            pub message: ::prost::alloc::string::String,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub struct StreamVariant {
            #[prost(bytes = "vec", tag = "1")]
            pub payload: ::prost::alloc::vec::Vec<u8>,
            #[prost(enumeration = "super::super::LogStreamType", tag = "2")]
            pub stream_type: i32,
        }
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
        pub enum Entry {
            #[prost(message, tag = "2")]
            Log(LogVariant),
            #[prost(message, tag = "3")]
            Stream(StreamVariant),
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListDeploymentsRequest {
    #[prost(bool, tag = "3")]
    pub include_deployment_toml: bool,
    /// Count child executions in the buckets too; only applies when `include_execution_counts` is set.
    #[prost(bool, tag = "4")]
    pub include_derived: bool,
    /// Populate `DeploymentSummary.execution_summary`.
    #[prost(bool, tag = "5")]
    pub include_execution_counts: bool,
    /// Populate `DeploymentSummary.component_summary`.
    ///
    /// TODO bool include_file refs
    #[prost(bool, tag = "6")]
    pub include_component_summary: bool,
    /// latest if not set
    #[prost(oneof = "list_deployments_request::Pagination", tags = "1, 2")]
    pub pagination: ::core::option::Option<list_deployments_request::Pagination>,
}
/// Nested message and enum types in `ListDeploymentsRequest`.
pub mod list_deployments_request {
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct NewerThan {
        #[prost(uint32, tag = "1")]
        pub length: u32,
        #[prost(message, optional, tag = "2")]
        pub cursor: ::core::option::Option<super::DeploymentId>,
        #[prost(bool, tag = "3")]
        pub including_cursor: bool,
    }
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
    pub struct OlderThan {
        #[prost(uint32, tag = "1")]
        pub length: u32,
        #[prost(message, optional, tag = "2")]
        pub cursor: ::core::option::Option<super::DeploymentId>,
        #[prost(bool, tag = "3")]
        pub including_cursor: bool,
    }
    /// latest if not set
    #[derive(Clone, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Pagination {
        #[prost(message, tag = "1")]
        NewerThan(NewerThan),
        #[prost(message, tag = "2")]
        OlderThan(OlderThan),
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeploymentComponentCount {
    #[prost(enumeration = "DeploymentComponentType", tag = "1")]
    pub component_type: i32,
    #[prost(uint32, tag = "2")]
    pub count: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeploymentComponentSummary {
    #[prost(message, repeated, tag = "1")]
    pub components: ::prost::alloc::vec::Vec<DeploymentComponentCount>,
}
/// Execution counts use disjoint buckets: a paused execution is counted only in
/// `paused`, not in `locked`/`pending`/`scheduled`/`blocked`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeploymentExecutionSummary {
    #[prost(uint32, tag = "1")]
    pub locked: u32,
    #[prost(uint32, tag = "2")]
    pub pending: u32,
    #[prost(uint32, tag = "3")]
    pub scheduled: u32,
    #[prost(uint32, tag = "4")]
    pub blocked: u32,
    /// Paused regardless of the underlying state (locked, pending or blocked).
    #[prost(uint32, tag = "5")]
    pub paused: u32,
    /// Finished successfully.
    #[prost(uint32, tag = "6")]
    pub finished_ok: u32,
    /// Finished with the `err` variant of the result type.
    #[prost(uint32, tag = "7")]
    pub finished_error: u32,
    /// Execution failure: trap, timeout, nondeterminism, cancellation etc.
    #[prost(uint32, tag = "8")]
    pub finished_execution_failure: u32,
    /// Cancellation requested; teardown in progress.
    #[prost(uint32, tag = "9")]
    pub cancelling: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Deployment {
    #[prost(message, optional, tag = "1")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(enumeration = "DeploymentStatus", tag = "2")]
    pub status: i32,
    #[prost(message, optional, tag = "3")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(message, optional, tag = "4")]
    pub last_active_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    /// deployment manifest (`deployment.toml`). Present when requested.
    #[prost(string, optional, tag = "5")]
    pub deployment_toml: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "6")]
    pub description: ::core::option::Option<::prost::alloc::string::String>,
    /// Content digest = sha256(deployment_toml).
    #[prost(string, tag = "7")]
    pub digest: ::prost::alloc::string::String,
    /// Files the manifest references: (relative path, content digest).
    #[prost(message, repeated, tag = "8")]
    pub files: ::prost::alloc::vec::Vec<FileRef>,
}
/// A deployment-owned file the manifest references, named by both its
/// deployment-relative path and its content digest ("sha256:...").
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FileRef {
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub digest: ::prost::alloc::string::String,
    /// Byte length of the referenced blob.
    #[prost(uint64, tag = "3")]
    pub size: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeploymentSummary {
    #[prost(message, optional, tag = "1")]
    pub deployment: ::core::option::Option<Deployment>,
    #[prost(message, optional, tag = "11")]
    pub execution_summary: ::core::option::Option<DeploymentExecutionSummary>,
    #[prost(message, optional, tag = "12")]
    pub component_summary: ::core::option::Option<DeploymentComponentSummary>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListDeploymentsResponse {
    #[prost(message, repeated, tag = "1")]
    pub deployments: ::prost::alloc::vec::Vec<DeploymentSummary>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetCurrentDeploymentIdRequest {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetCurrentDeploymentIdResponse {
    #[prost(message, optional, tag = "1")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SwitchDeploymentRequest {
    #[prost(message, optional, tag = "1")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    /// Runtime config availability policy applied while verifying the deployment.
    /// Setting `apply = true` rejects RUNTIME_CONFIG_CHECK_ALLOW_UNAVAILABLE.
    #[prost(enumeration = "RuntimeConfigCheck", tag = "2")]
    pub runtime_config_check: i32,
    /// Apply the deployment immediately without restart.
    #[prost(bool, tag = "3")]
    pub apply: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SwitchDeploymentResponse {
    #[prost(enumeration = "switch_deployment_response::Outcome", tag = "1")]
    pub outcome: i32,
}
/// Nested message and enum types in `SwitchDeploymentResponse`.
pub mod switch_deployment_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum Outcome {
        SwitchOutcomeUnspecified = 0,
        /// The server performed a hot redeploy; the new deployment is now live.
        SwitchOutcomeSwitched = 1,
        /// The database was updated; the server must be restarted to apply the new deployment.
        SwitchOutcomeRestartRequired = 2,
    }
    impl Outcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::SwitchOutcomeUnspecified => "SWITCH_OUTCOME_UNSPECIFIED",
                Self::SwitchOutcomeSwitched => "SWITCH_OUTCOME_SWITCHED",
                Self::SwitchOutcomeRestartRequired => "SWITCH_OUTCOME_RESTART_REQUIRED",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "SWITCH_OUTCOME_UNSPECIFIED" => Some(Self::SwitchOutcomeUnspecified),
                "SWITCH_OUTCOME_SWITCHED" => Some(Self::SwitchOutcomeSwitched),
                "SWITCH_OUTCOME_RESTART_REQUIRED" => {
                    Some(Self::SwitchOutcomeRestartRequired)
                }
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubmitDeploymentRequest {
    /// Processed deployment manifest (`deployment.toml`), stored byte-for-byte.
    /// Every deployment-owned relative `location =` must carry a `content_digest`.
    #[prost(string, tag = "1")]
    pub deployment_toml: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "2")]
    pub created_by: ::core::option::Option<::prost::alloc::string::String>,
    /// Runtime config availability policy applied while verifying the deployment
    /// before it is persisted. Defaults to RUNTIME_CONFIG_CHECK_STRICT.
    #[prost(enumeration = "RuntimeConfigCheck", tag = "3")]
    pub runtime_config_check: i32,
    #[prost(string, optional, tag = "4")]
    pub description: ::core::option::Option<::prost::alloc::string::String>,
    /// Client-supplied deployment ID for idempotent submission.
    /// If a deployment with this ID already exists and its content digest matches
    /// the submitted manifest, the submission is a no-op and returns this ID.
    /// A digest mismatch is rejected.
    #[prost(message, optional, tag = "5")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    /// Deployment-owned file blobs referenced by the manifest. A blob may be
    /// omitted when its digest already exists in the CAS; if a referenced digest
    /// is neither attached here nor present in the CAS, submit fails with
    /// `SubmitDeploymentErrorDetail.missing_files` and no deployment is stored.
    #[prost(message, repeated, tag = "6")]
    pub files: ::prost::alloc::vec::Vec<DeploymentFileContent>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeploymentFileContent {
    /// Deployment-relative path as it appears after normalizing the manifest
    /// reference. Used to attach bytes to a specific manifest reference and to
    /// make validation errors actionable.
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    /// Optional client-computed digest for clearer errors and deduplication; the
    /// server always recomputes it from `content` and uses the recomputed value.
    #[prost(string, optional, tag = "2")]
    pub digest: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bytes = "vec", tag = "3")]
    pub content: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SubmitDeploymentResponse {}
/// Structured detail attached (prost-encoded) to a FAILED_PRECONDITION /
/// INVALID_ARGUMENT submit status. Each list points at the offending manifest
/// reference so callers can act without guessing.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubmitDeploymentErrorDetail {
    /// Deployment-owned references whose `content_digest` field is absent.
    #[prost(message, repeated, tag = "1")]
    pub missing_digest_fields: ::prost::alloc::vec::Vec<FileIssue>,
    /// Referenced digests neither attached in the request nor present in CAS.
    #[prost(message, repeated, tag = "2")]
    pub missing_files: ::prost::alloc::vec::Vec<FileIssue>,
    /// Attached blobs whose digest is not referenced by the manifest.
    #[prost(message, repeated, tag = "3")]
    pub unexpected_files: ::prost::alloc::vec::Vec<FileIssue>,
    /// Attached blobs whose recomputed digest disagrees with the supplied one.
    #[prost(message, repeated, tag = "4")]
    pub digest_mismatches: ::prost::alloc::vec::Vec<DigestMismatch>,
    /// Attached blobs exceeding the per-file size limit.
    #[prost(message, repeated, tag = "5")]
    pub oversized_files: ::prost::alloc::vec::Vec<FileIssue>,
    /// Secret names referenced by the deployment but absent from the server registry.
    #[prost(string, repeated, tag = "6")]
    pub unregistered_secrets: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// Environment variables referenced by the deployment but absent from public_env.allowed.
    #[prost(string, repeated, tag = "7")]
    pub undeclared_public_env: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct FileIssue {
    /// Example: "activity_js", "workflow_wasm.backtrace.sources".
    #[prost(string, tag = "1")]
    pub section: ::prost::alloc::string::String,
    /// Component name after name derivation, when available.
    #[prost(string, optional, tag = "2")]
    pub component_name: ::core::option::Option<::prost::alloc::string::String>,
    /// Stable field path, e.g. "activity_js\[name=send-email\].location".
    #[prost(string, tag = "3")]
    pub field_path: ::prost::alloc::string::String,
    /// Deployment-relative file path, when the issue is tied to a path.
    #[prost(string, optional, tag = "4")]
    pub path: ::core::option::Option<::prost::alloc::string::String>,
    /// Expected digest, when known. Absent for `missing_digest_fields`.
    #[prost(string, optional, tag = "5")]
    pub digest: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, tag = "6")]
    pub message: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DigestMismatch {
    #[prost(message, optional, tag = "1")]
    pub file: ::core::option::Option<FileIssue>,
    #[prost(string, tag = "2")]
    pub supplied_digest: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub actual_digest: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetFileRequest {
    #[prost(string, tag = "1")]
    pub digest: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetFileResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub content: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeleteExecutionTreeRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(bool, tag = "2")]
    pub force_non_terminal: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeleteExecutionTreeResponse {
    #[prost(bool, tag = "1")]
    pub deleted: bool,
    #[prost(bool, tag = "2")]
    pub already_deleted: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct RetainExecutionsRequest {
    #[prost(uint32, tag = "2")]
    pub batch_size: u32,
    #[prost(bool, tag = "3")]
    pub dry_run: bool,
    #[prost(bool, tag = "5")]
    pub force_non_terminal: bool,
    #[prost(oneof = "retain_executions_request::Retention", tags = "1, 4")]
    pub retention: ::core::option::Option<retain_executions_request::Retention>,
}
/// Nested message and enum types in `RetainExecutionsRequest`.
pub mod retain_executions_request {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Retention {
        #[prost(uint32, tag = "1")]
        RetainCount(u32),
        #[prost(message, tag = "4")]
        MaxAge(crate::grpc::wkt_types::Duration),
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeleteDeploymentRequest {
    #[prost(message, optional, tag = "1")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(bool, tag = "2")]
    pub delete_executions: bool,
    #[prost(bool, tag = "3")]
    pub force_non_terminal: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DeleteDeploymentResponse {
    #[prost(bool, tag = "1")]
    pub deleted: bool,
    #[prost(bool, tag = "2")]
    pub already_deleted: bool,
    #[prost(uint64, tag = "3")]
    pub deleted_execution_trees: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct RetainDeploymentsRequest {
    #[prost(uint32, tag = "2")]
    pub batch_size: u32,
    #[prost(bool, tag = "3")]
    pub delete_executions: bool,
    #[prost(bool, tag = "4")]
    pub dry_run: bool,
    #[prost(bool, tag = "5")]
    pub force_non_terminal: bool,
    #[prost(oneof = "retain_deployments_request::Retention", tags = "1, 6")]
    pub retention: ::core::option::Option<retain_deployments_request::Retention>,
}
/// Nested message and enum types in `RetainDeploymentsRequest`.
pub mod retain_deployments_request {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Oneof)]
    pub enum Retention {
        #[prost(uint32, tag = "1")]
        RetainCount(u32),
        #[prost(message, tag = "6")]
        MaxAge(crate::grpc::wkt_types::Duration),
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct CleanupResponse {
    #[prost(uint64, tag = "1")]
    pub deleted_execution_trees: u64,
    #[prost(uint64, tag = "2")]
    pub deleted_deployments: u64,
    #[prost(uint64, tag = "3")]
    pub retained: u64,
    #[prost(uint64, tag = "4")]
    pub blocked_non_terminal: u64,
    #[prost(uint64, tag = "5")]
    pub blocked_by_execution_reference: u64,
    #[prost(bool, tag = "6")]
    pub has_more: bool,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SystemEvent {
    #[prost(string, tag = "1")]
    pub event_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub created_at: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    #[prost(enumeration = "SystemEventLevel", tag = "3")]
    pub level: i32,
    #[prost(string, tag = "4")]
    pub code: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub message: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "6")]
    pub execution_id: ::core::option::Option<ExecutionId>,
    #[prost(message, optional, tag = "7")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(string, tag = "8")]
    pub details_json: ::prost::alloc::string::String,
    #[prost(string, tag = "9")]
    pub node_run_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ListSystemEventsRequest {
    #[prost(enumeration = "SystemEventLevel", optional, tag = "1")]
    pub level: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub code: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    #[prost(string, optional, tag = "4")]
    pub before_event_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, tag = "5")]
    pub limit: u32,
    #[prost(string, optional, tag = "6")]
    pub node_run_id: ::core::option::Option<::prost::alloc::string::String>,
    /// Inclusive lower bound of `created_at`, millisecond precision.
    #[prost(message, optional, tag = "7")]
    pub created_from: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
    /// Exclusive upper bound of `created_at`, millisecond precision.
    #[prost(message, optional, tag = "8")]
    pub created_to: ::core::option::Option<crate::grpc::wkt_types::Timestamp>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListSystemEventsResponse {
    #[prost(message, repeated, tag = "1")]
    pub events: ::prost::alloc::vec::Vec<SystemEvent>,
    #[prost(string, optional, tag = "2")]
    pub next_cursor: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetSystemEventRequest {
    #[prost(string, tag = "1")]
    pub event_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetSystemEventResponse {
    #[prost(message, optional, tag = "1")]
    pub event: ::core::option::Option<SystemEvent>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetStorageStatusRequest {}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetStorageStatusResponse {
    #[prost(uint64, optional, tag = "1")]
    pub database_bytes: ::core::option::Option<u64>,
    #[prost(uint64, tag = "2")]
    pub execution_count: u64,
    #[prost(uint64, tag = "3")]
    pub deployment_count: u64,
    #[prost(uint64, tag = "4")]
    pub system_event_count: u64,
    #[prost(uint64, optional, tag = "5")]
    pub cas_blob_count: ::core::option::Option<u64>,
    #[prost(uint64, optional, tag = "6")]
    pub cas_bytes: ::core::option::Option<u64>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetNodeRunIdRequest {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetNodeRunIdResponse {
    #[prost(string, tag = "1")]
    pub node_run_id: ::prost::alloc::string::String,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct RetainSystemEventsRequest {
    #[prost(message, optional, tag = "1")]
    pub max_age: ::core::option::Option<crate::grpc::wkt_types::Duration>,
    #[prost(uint32, tag = "2")]
    pub batch_size: u32,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct RetainSystemEventsResponse {
    #[prost(uint64, tag = "1")]
    pub deleted: u64,
    #[prost(bool, tag = "2")]
    pub has_more: bool,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct GetDeploymentRequest {
    #[prost(message, optional, tag = "1")]
    pub deployment_id: ::core::option::Option<DeploymentId>,
    /// Include server-generated metadata in deployment_toml. Omission preserves the stored server view.
    #[prost(bool, optional, tag = "2")]
    pub include_generated_metadata: ::core::option::Option<bool>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetDeploymentResponse {
    #[prost(message, optional, tag = "1")]
    pub deployment: ::core::option::Option<Deployment>,
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PauseExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PauseExecutionResponse {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UnpauseExecutionRequest {
    #[prost(message, optional, tag = "1")]
    pub execution_id: ::core::option::Option<ExecutionId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UnpauseExecutionResponse {}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PauseDelayRequest {
    #[prost(message, optional, tag = "1")]
    pub delay_id: ::core::option::Option<DelayId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct PauseDelayResponse {
    #[prost(enumeration = "pause_delay_response::PauseDelayOutcome", tag = "1")]
    pub outcome: i32,
}
/// Nested message and enum types in `PauseDelayResponse`.
pub mod pause_delay_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum PauseDelayOutcome {
        Unspecified = 0,
        Paused = 1,
        AlreadyFinished = 2,
    }
    impl PauseDelayOutcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "PAUSE_DELAY_OUTCOME_UNSPECIFIED",
                Self::Paused => "PAUSE_DELAY_OUTCOME_PAUSED",
                Self::AlreadyFinished => "PAUSE_DELAY_OUTCOME_ALREADY_FINISHED",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "PAUSE_DELAY_OUTCOME_UNSPECIFIED" => Some(Self::Unspecified),
                "PAUSE_DELAY_OUTCOME_PAUSED" => Some(Self::Paused),
                "PAUSE_DELAY_OUTCOME_ALREADY_FINISHED" => Some(Self::AlreadyFinished),
                _ => None,
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UnpauseDelayRequest {
    #[prost(message, optional, tag = "1")]
    pub delay_id: ::core::option::Option<DelayId>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct UnpauseDelayResponse {
    #[prost(enumeration = "unpause_delay_response::UnpauseDelayOutcome", tag = "1")]
    pub outcome: i32,
}
/// Nested message and enum types in `UnpauseDelayResponse`.
pub mod unpause_delay_response {
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        PartialOrd,
        Ord,
        ::prost::Enumeration
    )]
    #[repr(i32)]
    pub enum UnpauseDelayOutcome {
        Unspecified = 0,
        Unpaused = 1,
        AlreadyFinished = 2,
    }
    impl UnpauseDelayOutcome {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Unspecified => "UNPAUSE_DELAY_OUTCOME_UNSPECIFIED",
                Self::Unpaused => "UNPAUSE_DELAY_OUTCOME_UNPAUSED",
                Self::AlreadyFinished => "UNPAUSE_DELAY_OUTCOME_ALREADY_FINISHED",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "UNPAUSE_DELAY_OUTCOME_UNSPECIFIED" => Some(Self::Unspecified),
                "UNPAUSE_DELAY_OUTCOME_UNPAUSED" => Some(Self::Unpaused),
                "UNPAUSE_DELAY_OUTCOME_ALREADY_FINISHED" => Some(Self::AlreadyFinished),
                _ => None,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ComponentFileRole {
    Unspecified = 0,
    WasmComponent = 1,
    ExecProgram = 2,
    JsEntrypoint = 3,
    JsModule = 4,
    BacktraceSource = 5,
    WitSource = 6,
}
impl ComponentFileRole {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "COMPONENT_FILE_ROLE_UNSPECIFIED",
            Self::WasmComponent => "COMPONENT_FILE_ROLE_WASM_COMPONENT",
            Self::ExecProgram => "COMPONENT_FILE_ROLE_EXEC_PROGRAM",
            Self::JsEntrypoint => "COMPONENT_FILE_ROLE_JS_ENTRYPOINT",
            Self::JsModule => "COMPONENT_FILE_ROLE_JS_MODULE",
            Self::BacktraceSource => "COMPONENT_FILE_ROLE_BACKTRACE_SOURCE",
            Self::WitSource => "COMPONENT_FILE_ROLE_WIT_SOURCE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COMPONENT_FILE_ROLE_UNSPECIFIED" => Some(Self::Unspecified),
            "COMPONENT_FILE_ROLE_WASM_COMPONENT" => Some(Self::WasmComponent),
            "COMPONENT_FILE_ROLE_EXEC_PROGRAM" => Some(Self::ExecProgram),
            "COMPONENT_FILE_ROLE_JS_ENTRYPOINT" => Some(Self::JsEntrypoint),
            "COMPONENT_FILE_ROLE_JS_MODULE" => Some(Self::JsModule),
            "COMPONENT_FILE_ROLE_BACKTRACE_SOURCE" => Some(Self::BacktraceSource),
            "COMPONENT_FILE_ROLE_WIT_SOURCE" => Some(Self::WitSource),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ComponentType {
    Unspecified = 0,
    Workflow = 1,
    Activity = 2,
    WebhookEndpoint = 3,
    ActivityStub = 4,
    Cron = 5,
}
impl ComponentType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "COMPONENT_TYPE_UNSPECIFIED",
            Self::Workflow => "COMPONENT_TYPE_WORKFLOW",
            Self::Activity => "COMPONENT_TYPE_ACTIVITY",
            Self::WebhookEndpoint => "COMPONENT_TYPE_WEBHOOK_ENDPOINT",
            Self::ActivityStub => "COMPONENT_TYPE_ACTIVITY_STUB",
            Self::Cron => "COMPONENT_TYPE_CRON",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COMPONENT_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "COMPONENT_TYPE_WORKFLOW" => Some(Self::Workflow),
            "COMPONENT_TYPE_ACTIVITY" => Some(Self::Activity),
            "COMPONENT_TYPE_WEBHOOK_ENDPOINT" => Some(Self::WebhookEndpoint),
            "COMPONENT_TYPE_ACTIVITY_STUB" => Some(Self::ActivityStub),
            "COMPONENT_TYPE_CRON" => Some(Self::Cron),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum FunctionExtension {
    Unspecified = 0,
    Submit = 1,
    AwaitNext = 2,
    Schedule = 3,
    Stub = 4,
    Get = 5,
}
impl FunctionExtension {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "FUNCTION_EXTENSION_UNSPECIFIED",
            Self::Submit => "FUNCTION_EXTENSION_SUBMIT",
            Self::AwaitNext => "FUNCTION_EXTENSION_AWAIT_NEXT",
            Self::Schedule => "FUNCTION_EXTENSION_SCHEDULE",
            Self::Stub => "FUNCTION_EXTENSION_STUB",
            Self::Get => "FUNCTION_EXTENSION_GET",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "FUNCTION_EXTENSION_UNSPECIFIED" => Some(Self::Unspecified),
            "FUNCTION_EXTENSION_SUBMIT" => Some(Self::Submit),
            "FUNCTION_EXTENSION_AWAIT_NEXT" => Some(Self::AwaitNext),
            "FUNCTION_EXTENSION_SCHEDULE" => Some(Self::Schedule),
            "FUNCTION_EXTENSION_STUB" => Some(Self::Stub),
            "FUNCTION_EXTENSION_GET" => Some(Self::Get),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ExecutionFailureKind {
    Unspecified = 0,
    /// Applicable to activities only, because workflows will be retried forever
    TimedOut = 1,
    /// Applicable to workflows
    NondeterminismDetected = 2,
    /// Applicable to WASM components
    OutOfFuel = 3,
    /// Applicable to activities
    Cancelled = 4,
    Uncategorized = 5,
    ValueTooLarge = 6,
}
impl ExecutionFailureKind {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "EXECUTION_FAILURE_KIND_UNSPECIFIED",
            Self::TimedOut => "EXECUTION_FAILURE_KIND_TIMED_OUT",
            Self::NondeterminismDetected => {
                "EXECUTION_FAILURE_KIND_NONDETERMINISM_DETECTED"
            }
            Self::OutOfFuel => "EXECUTION_FAILURE_KIND_OUT_OF_FUEL",
            Self::Cancelled => "EXECUTION_FAILURE_KIND_CANCELLED",
            Self::Uncategorized => "EXECUTION_FAILURE_KIND_UNCATEGORIZED",
            Self::ValueTooLarge => "EXECUTION_FAILURE_KIND_VALUE_TOO_LARGE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "EXECUTION_FAILURE_KIND_UNSPECIFIED" => Some(Self::Unspecified),
            "EXECUTION_FAILURE_KIND_TIMED_OUT" => Some(Self::TimedOut),
            "EXECUTION_FAILURE_KIND_NONDETERMINISM_DETECTED" => {
                Some(Self::NondeterminismDetected)
            }
            "EXECUTION_FAILURE_KIND_OUT_OF_FUEL" => Some(Self::OutOfFuel),
            "EXECUTION_FAILURE_KIND_CANCELLED" => Some(Self::Cancelled),
            "EXECUTION_FAILURE_KIND_UNCATEGORIZED" => Some(Self::Uncategorized),
            "EXECUTION_FAILURE_KIND_VALUE_TOO_LARGE" => Some(Self::ValueTooLarge),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum LogLevel {
    Unspecified = 0,
    Trace = 1,
    Debug = 2,
    Info = 3,
    Warn = 4,
    Error = 5,
}
impl LogLevel {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "LOG_LEVEL_UNSPECIFIED",
            Self::Trace => "LOG_LEVEL_TRACE",
            Self::Debug => "LOG_LEVEL_DEBUG",
            Self::Info => "LOG_LEVEL_INFO",
            Self::Warn => "LOG_LEVEL_WARN",
            Self::Error => "LOG_LEVEL_ERROR",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "LOG_LEVEL_UNSPECIFIED" => Some(Self::Unspecified),
            "LOG_LEVEL_TRACE" => Some(Self::Trace),
            "LOG_LEVEL_DEBUG" => Some(Self::Debug),
            "LOG_LEVEL_INFO" => Some(Self::Info),
            "LOG_LEVEL_WARN" => Some(Self::Warn),
            "LOG_LEVEL_ERROR" => Some(Self::Error),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum LogStreamType {
    Unspecified = 0,
    Stdout = 1,
    Stderr = 2,
}
impl LogStreamType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "LOG_STREAM_TYPE_UNSPECIFIED",
            Self::Stdout => "LOG_STREAM_TYPE_STDOUT",
            Self::Stderr => "LOG_STREAM_TYPE_STDERR",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "LOG_STREAM_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "LOG_STREAM_TYPE_STDOUT" => Some(Self::Stdout),
            "LOG_STREAM_TYPE_STDERR" => Some(Self::Stderr),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DeploymentStatus {
    Unspecified = 0,
    Inactive = 1,
    Enqueued = 2,
    Active = 3,
}
impl DeploymentStatus {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "DEPLOYMENT_STATUS_UNSPECIFIED",
            Self::Inactive => "DEPLOYMENT_STATUS_INACTIVE",
            Self::Enqueued => "DEPLOYMENT_STATUS_ENQUEUED",
            Self::Active => "DEPLOYMENT_STATUS_ACTIVE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DEPLOYMENT_STATUS_UNSPECIFIED" => Some(Self::Unspecified),
            "DEPLOYMENT_STATUS_INACTIVE" => Some(Self::Inactive),
            "DEPLOYMENT_STATUS_ENQUEUED" => Some(Self::Enqueued),
            "DEPLOYMENT_STATUS_ACTIVE" => Some(Self::Active),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DeploymentComponentType {
    Unspecified = 0,
    WorkflowWasm = 1,
    WorkflowJs = 2,
    ActivityWasm = 3,
    ActivityJs = 4,
    ActivityExec = 5,
    ActivityStub = 6,
    ActivityExternal = 7,
    WebhookEndpointWasm = 8,
    WebhookEndpointJs = 9,
    Cron = 10,
    ActivityVm = 11,
}
impl DeploymentComponentType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "DEPLOYMENT_COMPONENT_TYPE_UNSPECIFIED",
            Self::WorkflowWasm => "DEPLOYMENT_COMPONENT_TYPE_WORKFLOW_WASM",
            Self::WorkflowJs => "DEPLOYMENT_COMPONENT_TYPE_WORKFLOW_JS",
            Self::ActivityWasm => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_WASM",
            Self::ActivityJs => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_JS",
            Self::ActivityExec => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_EXEC",
            Self::ActivityStub => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_STUB",
            Self::ActivityExternal => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_EXTERNAL",
            Self::WebhookEndpointWasm => {
                "DEPLOYMENT_COMPONENT_TYPE_WEBHOOK_ENDPOINT_WASM"
            }
            Self::WebhookEndpointJs => "DEPLOYMENT_COMPONENT_TYPE_WEBHOOK_ENDPOINT_JS",
            Self::Cron => "DEPLOYMENT_COMPONENT_TYPE_CRON",
            Self::ActivityVm => "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_VM",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DEPLOYMENT_COMPONENT_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "DEPLOYMENT_COMPONENT_TYPE_WORKFLOW_WASM" => Some(Self::WorkflowWasm),
            "DEPLOYMENT_COMPONENT_TYPE_WORKFLOW_JS" => Some(Self::WorkflowJs),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_WASM" => Some(Self::ActivityWasm),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_JS" => Some(Self::ActivityJs),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_EXEC" => Some(Self::ActivityExec),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_STUB" => Some(Self::ActivityStub),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_EXTERNAL" => Some(Self::ActivityExternal),
            "DEPLOYMENT_COMPONENT_TYPE_WEBHOOK_ENDPOINT_WASM" => {
                Some(Self::WebhookEndpointWasm)
            }
            "DEPLOYMENT_COMPONENT_TYPE_WEBHOOK_ENDPOINT_JS" => {
                Some(Self::WebhookEndpointJs)
            }
            "DEPLOYMENT_COMPONENT_TYPE_CRON" => Some(Self::Cron),
            "DEPLOYMENT_COMPONENT_TYPE_ACTIVITY_VM" => Some(Self::ActivityVm),
            _ => None,
        }
    }
}
/// Policy for runtime requirements that may be unavailable on the current server,
/// including environment variables, secrets, and server capabilities.
/// Structural, file, compile, and link validation are always performed regardless
/// of this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum RuntimeConfigCheck {
    /// Unset; treated as RUNTIME_CONFIG_CHECK_STRICT.
    Unspecified = 0,
    /// Missing environment variables or secrets fail the operation.
    Strict = 1,
    /// Unavailable environment variables, secrets, or server capabilities are tolerated.
    AllowUnavailable = 2,
}
impl RuntimeConfigCheck {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "RUNTIME_CONFIG_CHECK_UNSPECIFIED",
            Self::Strict => "RUNTIME_CONFIG_CHECK_STRICT",
            Self::AllowUnavailable => "RUNTIME_CONFIG_CHECK_ALLOW_UNAVAILABLE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "RUNTIME_CONFIG_CHECK_UNSPECIFIED" => Some(Self::Unspecified),
            "RUNTIME_CONFIG_CHECK_STRICT" => Some(Self::Strict),
            "RUNTIME_CONFIG_CHECK_ALLOW_UNAVAILABLE" => Some(Self::AllowUnavailable),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SystemEventLevel {
    Unspecified = 0,
    Debug = 1,
    Info = 2,
    Warning = 3,
    Error = 4,
}
impl SystemEventLevel {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "SYSTEM_EVENT_LEVEL_UNSPECIFIED",
            Self::Debug => "SYSTEM_EVENT_LEVEL_DEBUG",
            Self::Info => "SYSTEM_EVENT_LEVEL_INFO",
            Self::Warning => "SYSTEM_EVENT_LEVEL_WARNING",
            Self::Error => "SYSTEM_EVENT_LEVEL_ERROR",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SYSTEM_EVENT_LEVEL_UNSPECIFIED" => Some(Self::Unspecified),
            "SYSTEM_EVENT_LEVEL_DEBUG" => Some(Self::Debug),
            "SYSTEM_EVENT_LEVEL_INFO" => Some(Self::Info),
            "SYSTEM_EVENT_LEVEL_WARNING" => Some(Self::Warning),
            "SYSTEM_EVENT_LEVEL_ERROR" => Some(Self::Error),
            _ => None,
        }
    }
}
