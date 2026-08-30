use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    BoardSnapshot, DaemonInstanceId, EntityRef, EventId, HierarchyChildren, HierarchyRef,
    RequestId, WorkspaceId, WorkspaceReference, WorkspaceSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    pub supported_read_versions: Vec<u32>,
    pub supported_command_versions: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    pub daemon_instance_id: DaemonInstanceId,
    pub negotiated_read_version: u32,
    pub compatible_command_versions: Vec<u32>,
    pub workspaces: Vec<WorkspaceReference>,
    pub command_capabilities: Vec<CommandCapability>,
    pub event_version: u32,
    pub heartbeat_interval_ms: u64,
    pub max_frame_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub workspace_id: Option<WorkspaceId>,
    pub expected_revision: Option<u64>,
    pub idempotency_key: Option<String>,
    pub operation: Operation,
}

impl RequestEnvelope {
    pub fn validate(&self) -> Result<(), Box<ProtocolError>> {
        if self.protocol_version == 0 {
            return Err(Box::new(ProtocolError::validation(
                "protocol_version",
                "invalid_version",
            )));
        }
        if let Some(key) = &self.idempotency_key
            && (key.is_empty() || key.len() > 200 || key.chars().any(char::is_control))
        {
            return Err(Box::new(ProtocolError::validation(
                "idempotency_key",
                "invalid_idempotency_key",
            )));
        }
        match &self.operation {
            Operation::Handshake(handshake) => {
                if self.workspace_id.is_some()
                    || self.expected_revision.is_some()
                    || self.idempotency_key.is_some()
                {
                    return Err(Box::new(ProtocolError::validation(
                        "operation",
                        "invalid_handshake_scope",
                    )));
                }
                validate_versions(&handshake.supported_read_versions)?;
                if handshake.supported_command_versions.len() > 8 {
                    return Err(Box::new(ProtocolError::validation(
                        "supported_command_versions",
                        "collection_too_large",
                    )));
                }
            }
            Operation::Query(_) | Operation::Subscribe(_) => {
                if self.workspace_id.is_none() {
                    return Err(Box::new(ProtocolError::validation(
                        "workspace_id",
                        "workspace_required",
                    )));
                }
                if self.expected_revision.is_some() || self.idempotency_key.is_some() {
                    return Err(Box::new(ProtocolError::validation(
                        "operation",
                        "invalid_read_fields",
                    )));
                }
            }
            Operation::Command(_) => {
                if self.workspace_id.is_none()
                    || self.expected_revision.is_none()
                    || self.idempotency_key.is_none()
                {
                    return Err(Box::new(ProtocolError::validation(
                        "operation",
                        "mutation_fields_required",
                    )));
                }
            }
        }
        Ok(())
    }
}

fn validate_versions(versions: &[u32]) -> Result<(), Box<ProtocolError>> {
    if versions.is_empty() || versions.len() > 8 || versions.contains(&0) {
        return Err(Box::new(ProtocolError::validation(
            "supported_read_versions",
            "invalid_versions",
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Operation {
    Handshake(HandshakeRequest),
    Query(ReadQuery),
    Command(CommandOperation),
    Subscribe(SubscriptionRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ReadQuery {
    WorkspaceSummary,
    HierarchyChildren { parent: HierarchyRef },
    BoardSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCode {
    SaveBoardView,
    ApproveFeature,
    RequestFeatureRevision,
    RejectFeature,
    CheckpointWorkItem,
    StartSession,
    ResumeSession,
    FocusSession,
    FollowUpSession,
    RecoverSession,
}

impl CommandCode {
    pub const ALL: [Self; 10] = [
        Self::SaveBoardView,
        Self::ApproveFeature,
        Self::RequestFeatureRevision,
        Self::RejectFeature,
        Self::CheckpointWorkItem,
        Self::StartSession,
        Self::ResumeSession,
        Self::FocusSession,
        Self::FollowUpSession,
        Self::RecoverSession,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CommandOperation {
    SaveBoardView,
    ApproveFeature { feature_id: crate::FeatureId },
    RequestFeatureRevision { feature_id: crate::FeatureId },
    RejectFeature { feature_id: crate::FeatureId },
    CheckpointWorkItem { work_item_id: crate::WorkItemId },
    StartSession { work_item_id: crate::WorkItemId },
    ResumeSession { session_id: crate::SessionId },
    FocusSession { session_id: crate::SessionId },
    FollowUpSession { session_id: crate::SessionId },
    RecoverSession { session_id: crate::SessionId },
}

impl CommandOperation {
    pub const fn code(&self) -> CommandCode {
        match self {
            Self::SaveBoardView => CommandCode::SaveBoardView,
            Self::ApproveFeature { .. } => CommandCode::ApproveFeature,
            Self::RequestFeatureRevision { .. } => CommandCode::RequestFeatureRevision,
            Self::RejectFeature { .. } => CommandCode::RejectFeature,
            Self::CheckpointWorkItem { .. } => CommandCode::CheckpointWorkItem,
            Self::StartSession { .. } => CommandCode::StartSession,
            Self::ResumeSession { .. } => CommandCode::ResumeSession,
            Self::FocusSession { .. } => CommandCode::FocusSession,
            Self::FollowUpSession { .. } => CommandCode::FollowUpSession,
            Self::RecoverSession { .. } => CommandCode::RecoverSession,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRequest {
    pub cursor: Option<EventCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventCursor {
    pub daemon_instance_id: DaemonInstanceId,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub correlation_id: RequestId,
    pub workspace_id: Option<WorkspaceId>,
    pub authoritative_revision: Option<u64>,
    #[serde(with = "time::serde::rfc3339")]
    pub server_timestamp: OffsetDateTime,
    pub result: Option<ResponseResult>,
    pub error: Option<ProtocolError>,
    pub diagnostics: Vec<Diagnostic>,
    pub available_actions: Vec<AvailableAction>,
    pub partial_outcomes: Vec<PartialOutcome>,
}

impl ResponseEnvelope {
    pub fn success(
        version: u32,
        request: &RequestEnvelope,
        revision: Option<u64>,
        result: ResponseResult,
        actions: Vec<AvailableAction>,
    ) -> Self {
        Self {
            protocol_version: version,
            request_id: request.request_id,
            correlation_id: request.request_id,
            workspace_id: request.workspace_id,
            authoritative_revision: revision,
            server_timestamp: OffsetDateTime::now_utc(),
            result: Some(result),
            error: None,
            diagnostics: Vec::new(),
            available_actions: actions,
            partial_outcomes: Vec::new(),
        }
    }

    pub fn failure(version: u32, request: &RequestEnvelope, error: ProtocolError) -> Self {
        Self {
            protocol_version: version,
            request_id: request.request_id,
            correlation_id: request.request_id,
            workspace_id: request.workspace_id,
            authoritative_revision: error.current_revision,
            server_timestamp: OffsetDateTime::now_utc(),
            result: None,
            error: Some(error),
            diagnostics: Vec::new(),
            available_actions: Vec::new(),
            partial_outcomes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseResult {
    Handshake(HandshakeResponse),
    WorkspaceSummary(WorkspaceSummary),
    HierarchyChildren(HierarchyChildren),
    BoardSnapshot(BoardSnapshot),
    SubscriptionAccepted { cursor: EventCursor },
    CommandAccepted { code: CommandCode },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub severity: ErrorSeverity,
    pub retryable: bool,
    pub validation_fields: Vec<ValidationField>,
    pub stale_revision: Option<u64>,
    pub current_revision: Option<u64>,
    pub reconciliation_owner: Option<EntityRef>,
    pub correlation_id: Option<RequestId>,
    pub resync: Option<ResyncRequirement>,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: ErrorSeverity::Error,
            retryable: false,
            validation_fields: Vec::new(),
            stale_revision: None,
            current_revision: None,
            reconciliation_owner: None,
            correlation_id: None,
            resync: None,
        }
    }

    pub fn validation(field: impl Into<String>, code: impl Into<String>) -> Self {
        let mut error = Self::new("invalid_request", "the request is invalid");
        error.validation_fields.push(ValidationField {
            field: field.into(),
            code: code.into(),
            message: "the field is invalid".to_owned(),
        });
        error
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationField {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub severity: ErrorSeverity,
    pub message: String,
    pub owner: Option<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableAction {
    pub code: CommandCode,
    pub available: bool,
    pub unavailable_reason: Option<UnavailableReason>,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCapability {
    pub code: CommandCode,
    pub available: bool,
    pub compatible_versions: Vec<u32>,
    pub unavailable_reason: Option<UnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnavailableReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialOutcome {
    pub owner: Option<EntityRef>,
    pub code: String,
    pub succeeded: bool,
    pub message: String,
    pub reconciliation_required: bool,
    pub evidence: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub protocol_version: u32,
    pub event_version: u32,
    pub workspace_id: WorkspaceId,
    pub sequence: u64,
    pub event_id: EventId,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub owner: EntityRef,
    pub entity_revision: u64,
    pub kind: EventKind,
    pub payload: Option<EventPayload>,
    pub invalidation_scope: Option<InvalidationScope>,
    pub operation_correlation_id: RequestId,
    pub partial_outcomes: Vec<PartialOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ProjectionChanged,
    NativeSessionsRefreshed,
    PartialOutcomeRecorded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EventPayload {
    ProjectionChanged { entity: EntityRef },
    NativeSessionsRefreshed { session_count: usize },
    PartialOutcome { outcome: PartialOutcome },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvalidationScope {
    pub queries: Vec<ReadQueryCode>,
    pub owners: Vec<EntityRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadQueryCode {
    WorkspaceSummary,
    HierarchyChildren,
    BoardSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ServerMessage {
    Response(Box<ResponseEnvelope>),
    Event(Box<EventEnvelope>),
    Heartbeat(Heartbeat),
    ResyncRequired(ResyncRequirement),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    pub daemon_instance_id: DaemonInstanceId,
    pub workspace_id: WorkspaceId,
    pub revision: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub sent_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResyncRequirement {
    pub reason: ResyncReason,
    pub workspace_id: WorkspaceId,
    pub authoritative_revision: u64,
    pub oldest_replayable_sequence: u64,
    pub required_queries: Vec<ReadQueryCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    Gap,
    CursorExpired,
    DaemonRestarted,
    IncompatibleEvent,
    HeartbeatLost,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION};

    fn handshake(version: u32) -> RequestEnvelope {
        RequestEnvelope {
            protocol_version: version,
            request_id: RequestId::from_uuid(
                uuid::Uuid::parse_str("10000000-0000-0000-0000-000000000001").expect("UUID"),
            ),
            workspace_id: None,
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Handshake(HandshakeRequest {
                supported_read_versions: vec![CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION],
                supported_command_versions: vec![CURRENT_PROTOCOL_VERSION],
            }),
        }
    }

    #[test]
    fn current_and_previous_handshakes_have_stable_golden_shapes() {
        for version in [CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION] {
            let value = serde_json::to_value(handshake(version)).expect("serialise handshake");
            assert_eq!(value["protocolVersion"], json!(version));
            assert_eq!(value["operation"]["type"], json!("handshake"));
            assert_eq!(
                value["requestId"],
                json!("10000000-0000-0000-0000-000000000001")
            );
            assert!(!value.to_string().contains("token"));
        }
    }

    #[test]
    fn every_closed_envelope_discriminant_serialises() {
        let workspace_id = WorkspaceId::generate();
        let request = RequestEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::generate(),
            workspace_id: Some(workspace_id),
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Query(ReadQuery::WorkspaceSummary),
        };
        let messages = [
            ServerMessage::Response(Box::new(ResponseEnvelope::failure(
                CURRENT_PROTOCOL_VERSION,
                &request,
                ProtocolError::new("unavailable", "unavailable"),
            ))),
            ServerMessage::Heartbeat(Heartbeat {
                daemon_instance_id: DaemonInstanceId::generate(),
                workspace_id,
                revision: 0,
                sent_at: OffsetDateTime::UNIX_EPOCH,
            }),
            ServerMessage::ResyncRequired(ResyncRequirement {
                reason: ResyncReason::Gap,
                workspace_id,
                authoritative_revision: 2,
                oldest_replayable_sequence: 1,
                required_queries: vec![ReadQueryCode::BoardSnapshot],
            }),
        ];
        let values = messages
            .into_iter()
            .map(|message| serde_json::to_value(message).expect("serialise message"))
            .collect::<Vec<Value>>();
        assert_eq!(values[0]["type"], json!("response"));
        assert_eq!(values[1]["type"], json!("heartbeat"));
        assert_eq!(values[2]["type"], json!("resync_required"));
    }

    #[test]
    fn request_validation_rejects_mutation_without_concurrency_fields() {
        let request = RequestEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::generate(),
            workspace_id: Some(WorkspaceId::generate()),
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Command(CommandOperation::SaveBoardView),
        };
        assert_eq!(
            request.validate().expect_err("invalid mutation").code,
            "invalid_request"
        );
    }

    #[test]
    fn malformed_ids_unknown_operations_and_control_keys_fail_closed() {
        let invalid_id = json!({
            "protocolVersion": CURRENT_PROTOCOL_VERSION,
            "requestId": "not-a-uuid",
            "workspaceId": null,
            "expectedRevision": null,
            "idempotencyKey": null,
            "operation": {
                "type": "handshake",
                "value": {
                    "supportedReadVersions": [CURRENT_PROTOCOL_VERSION],
                    "supportedCommandVersions": []
                }
            }
        });
        assert!(serde_json::from_value::<RequestEnvelope>(invalid_id).is_err());
        let mut unknown =
            serde_json::to_value(handshake(CURRENT_PROTOCOL_VERSION)).expect("handshake JSON");
        unknown["operation"]["type"] = json!("open_shell");
        assert!(serde_json::from_value::<RequestEnvelope>(unknown).is_err());
        let request = RequestEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::generate(),
            workspace_id: Some(WorkspaceId::generate()),
            expected_revision: Some(0),
            idempotency_key: Some("key\n".to_owned()),
            operation: Operation::Command(CommandOperation::SaveBoardView),
        };
        assert_eq!(
            request.validate().expect_err("control key").code,
            "invalid_request"
        );
    }

    #[test]
    fn published_catalogue_discriminants_have_stable_names() {
        let command_codes = CommandCode::ALL
            .into_iter()
            .map(|value| serde_json::to_value(value).expect("command code"))
            .collect::<Vec<_>>();
        assert_eq!(
            command_codes,
            vec![
                json!("save_board_view"),
                json!("approve_feature"),
                json!("request_feature_revision"),
                json!("reject_feature"),
                json!("checkpoint_work_item"),
                json!("start_session"),
                json!("resume_session"),
                json!("focus_session"),
                json!("follow_up_session"),
                json!("recover_session"),
            ]
        );
        let read_queries = [
            ReadQuery::WorkspaceSummary,
            ReadQuery::HierarchyChildren {
                parent: HierarchyRef::Workspace(WorkspaceId::generate()),
            },
            ReadQuery::BoardSnapshot,
        ];
        assert_eq!(
            read_queries
                .into_iter()
                .map(|value| serde_json::to_value(value).expect("read query")["type"].clone())
                .collect::<Vec<_>>(),
            vec![
                json!("workspace_summary"),
                json!("hierarchy_children"),
                json!("board_snapshot"),
            ]
        );
        let resync_reasons = [
            ResyncReason::Gap,
            ResyncReason::CursorExpired,
            ResyncReason::DaemonRestarted,
            ResyncReason::IncompatibleEvent,
            ResyncReason::HeartbeatLost,
        ];
        assert_eq!(
            resync_reasons
                .into_iter()
                .map(|value| serde_json::to_value(value).expect("resync reason"))
                .collect::<Vec<_>>(),
            vec![
                json!("gap"),
                json!("cursor_expired"),
                json!("daemon_restarted"),
                json!("incompatible_event"),
                json!("heartbeat_lost"),
            ]
        );
        let event_kinds = [
            EventKind::ProjectionChanged,
            EventKind::NativeSessionsRefreshed,
            EventKind::PartialOutcomeRecorded,
        ];
        assert_eq!(
            event_kinds
                .into_iter()
                .map(|value| serde_json::to_value(value).expect("event kind"))
                .collect::<Vec<_>>(),
            vec![
                json!("projection_changed"),
                json!("native_sessions_refreshed"),
                json!("partial_outcome_recorded"),
            ]
        );
    }
}
