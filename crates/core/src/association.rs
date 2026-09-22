use std::error::Error;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    AssociationEventId, ConversationId, ConversationRef, RepositoryId, WorkItemId, WorktreeId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct AssociationEventVersion(u32);

impl AssociationEventVersion {
    pub const V1: Self = Self(1);
}

impl TryFrom<u32> for AssociationEventVersion {
    type Error = UnsupportedAssociationEventVersion;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::V1),
            unsupported => Err(UnsupportedAssociationEventVersion(unsupported)),
        }
    }
}

impl From<AssociationEventVersion> for u32 {
    fn from(value: AssociationEventVersion) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedAssociationEventVersion(u32);

impl Display for UnsupportedAssociationEventVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "association event schema version {} is not supported",
            self.0
        )
    }
}

impl Error for UnsupportedAssociationEventVersion {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationAction {
    Transition,
    Assign,
    Correct,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationSource {
    ExplicitIntegration,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationAuthority {
    Conflict,
    WeakHint,
    CorrelatedInference,
    ExactObservation,
    ExplicitIntegration,
    Manual,
}

impl AssociationAuthority {
    pub const fn tier(self) -> u8 {
        match self {
            Self::Conflict => 0,
            Self::WeakHint => 1,
            Self::CorrelatedInference => 2,
            Self::ExactObservation => 3,
            Self::ExplicitIntegration => 4,
            Self::Manual => 5,
        }
    }

    pub const fn confidence_label(self) -> &'static str {
        match self {
            Self::Manual => "confirmed",
            Self::ExplicitIntegration => "explicit",
            Self::ExactObservation | Self::CorrelatedInference => "strong_inference",
            Self::WeakHint | Self::Conflict => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AssociationTargetData")]
pub struct AssociationTarget {
    repository_id: RepositoryId,
    work_item_id: WorkItemId,
    worktree_id: WorktreeId,
    branch: Option<String>,
}

impl AssociationTarget {
    pub fn new(
        repository_id: RepositoryId,
        work_item_id: WorkItemId,
        worktree_id: WorktreeId,
        branch: Option<String>,
    ) -> Result<Self, AssociationTargetError> {
        if branch.as_ref().is_some_and(|value| value.trim().is_empty()) {
            return Err(AssociationTargetError::EmptyBranch);
        }

        Ok(Self {
            repository_id,
            work_item_id,
            worktree_id,
            branch,
        })
    }

    pub fn repository_id(&self) -> RepositoryId {
        self.repository_id
    }

    pub fn work_item_id(&self) -> WorkItemId {
        self.work_item_id
    }

    pub fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
}

#[derive(Deserialize)]
struct AssociationTargetData {
    repository_id: RepositoryId,
    work_item_id: WorkItemId,
    worktree_id: WorktreeId,
    branch: Option<String>,
}

impl TryFrom<AssociationTargetData> for AssociationTarget {
    type Error = AssociationTargetError;

    fn try_from(value: AssociationTargetData) -> Result<Self, Self::Error> {
        Self::new(
            value.repository_id,
            value.work_item_id,
            value.worktree_id,
            value.branch,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationTargetError {
    EmptyBranch,
}

impl Display for AssociationTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBranch => formatter.write_str("branch cannot be empty when supplied"),
        }
    }
}

impl Error for AssociationTargetError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationEvidenceKind {
    CallerIdentity,
    GitCommonDirectory,
    GitWorktree,
    GitBranch,
    ManualDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AssociationEvidenceData")]
pub struct AssociationEvidence {
    kind: AssociationEvidenceKind,
    #[serde(with = "time::serde::rfc3339")]
    observed_at: OffsetDateTime,
    value: String,
    source_locator: Option<String>,
}

impl AssociationEvidence {
    pub fn new(
        kind: AssociationEvidenceKind,
        observed_at: OffsetDateTime,
        value: impl Into<String>,
        source_locator: Option<String>,
    ) -> Result<Self, AssociationEvidenceError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AssociationEvidenceError::EmptyValue);
        }
        if source_locator
            .as_ref()
            .is_some_and(|locator| locator.trim().is_empty())
        {
            return Err(AssociationEvidenceError::EmptySourceLocator);
        }

        Ok(Self {
            kind,
            observed_at,
            value,
            source_locator,
        })
    }

    pub fn kind(&self) -> AssociationEvidenceKind {
        self.kind
    }

    pub fn observed_at(&self) -> OffsetDateTime {
        self.observed_at
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn source_locator(&self) -> Option<&str> {
        self.source_locator.as_deref()
    }
}

#[derive(Deserialize)]
struct AssociationEvidenceData {
    kind: AssociationEvidenceKind,
    #[serde(with = "time::serde::rfc3339")]
    observed_at: OffsetDateTime,
    value: String,
    source_locator: Option<String>,
}

impl TryFrom<AssociationEvidenceData> for AssociationEvidence {
    type Error = AssociationEvidenceError;

    fn try_from(value: AssociationEvidenceData) -> Result<Self, Self::Error> {
        Self::new(
            value.kind,
            value.observed_at,
            value.value,
            value.source_locator,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationEvidenceError {
    EmptyValue,
    EmptySourceLocator,
}

impl Display for AssociationEvidenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyValue => formatter.write_str("association evidence value cannot be empty"),
            Self::EmptySourceLocator => {
                formatter.write_str("association evidence source locator cannot be empty")
            }
        }
    }
}

impl Error for AssociationEvidenceError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AssociationEventData")]
pub struct AssociationEvent {
    schema_version: AssociationEventVersion,
    id: AssociationEventId,
    conversation_id: ConversationId,
    conversation: ConversationRef,
    #[serde(with = "time::serde::rfc3339")]
    effective_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    action: AssociationAction,
    source: AssociationSource,
    authority: AssociationAuthority,
    target: AssociationTarget,
    reason: String,
    idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersedes_event_id: Option<AssociationEventId>,
    evidence: Vec<AssociationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAssociationEvent {
    pub id: AssociationEventId,
    pub conversation_id: ConversationId,
    pub conversation: ConversationRef,
    pub effective_at: OffsetDateTime,
    pub recorded_at: OffsetDateTime,
    pub target: AssociationTarget,
    pub reason: String,
    pub idempotency_key: String,
    pub evidence: Vec<AssociationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewManualAssociationEvent {
    pub id: AssociationEventId,
    pub conversation_id: ConversationId,
    pub conversation: ConversationRef,
    pub effective_at: OffsetDateTime,
    pub recorded_at: OffsetDateTime,
    pub action: AssociationAction,
    pub target: AssociationTarget,
    pub reason: String,
    pub idempotency_key: String,
    pub supersedes_event_id: Option<AssociationEventId>,
    pub evidence: Vec<AssociationEvidence>,
}

impl AssociationEvent {
    pub fn new(input: NewAssociationEvent) -> Result<Self, AssociationEventError> {
        Self::build(Self {
            schema_version: AssociationEventVersion::V1,
            id: input.id,
            conversation_id: input.conversation_id,
            conversation: input.conversation,
            effective_at: input.effective_at,
            recorded_at: input.recorded_at,
            action: AssociationAction::Transition,
            source: AssociationSource::ExplicitIntegration,
            authority: AssociationAuthority::ExplicitIntegration,
            target: input.target,
            reason: input.reason,
            idempotency_key: input.idempotency_key,
            supersedes_event_id: None,
            evidence: input.evidence,
        })
    }

    pub fn new_manual(input: NewManualAssociationEvent) -> Result<Self, AssociationEventError> {
        if !matches!(
            input.action,
            AssociationAction::Assign | AssociationAction::Correct | AssociationAction::Confirm
        ) {
            return Err(AssociationEventError::UnsupportedAction);
        }
        Self::build(Self {
            schema_version: AssociationEventVersion::V1,
            id: input.id,
            conversation_id: input.conversation_id,
            conversation: input.conversation,
            effective_at: input.effective_at,
            recorded_at: input.recorded_at,
            action: input.action,
            source: AssociationSource::Manual,
            authority: AssociationAuthority::Manual,
            target: input.target,
            reason: input.reason,
            idempotency_key: input.idempotency_key,
            supersedes_event_id: input.supersedes_event_id,
            evidence: input.evidence,
        })
    }

    fn build(event: Self) -> Result<Self, AssociationEventError> {
        if event.reason.trim().is_empty() {
            return Err(AssociationEventError::EmptyReason);
        }
        if event.idempotency_key.trim().is_empty() {
            return Err(AssociationEventError::EmptyIdempotencyKey);
        }
        if event.evidence.is_empty() {
            return Err(AssociationEventError::MissingEvidence);
        }
        Ok(event)
    }

    pub fn schema_version(&self) -> AssociationEventVersion {
        self.schema_version
    }

    pub fn id(&self) -> AssociationEventId {
        self.id
    }

    pub fn conversation_id(&self) -> ConversationId {
        self.conversation_id
    }

    pub fn conversation(&self) -> &ConversationRef {
        &self.conversation
    }

    pub fn effective_at(&self) -> OffsetDateTime {
        self.effective_at
    }

    pub fn recorded_at(&self) -> OffsetDateTime {
        self.recorded_at
    }

    pub fn action(&self) -> AssociationAction {
        self.action
    }

    pub fn source(&self) -> AssociationSource {
        self.source
    }

    pub fn authority(&self) -> AssociationAuthority {
        self.authority
    }

    pub fn target(&self) -> &AssociationTarget {
        &self.target
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn supersedes_event_id(&self) -> Option<AssociationEventId> {
        self.supersedes_event_id
    }

    pub fn evidence(&self) -> &[AssociationEvidence] {
        &self.evidence
    }
}

#[derive(Deserialize)]
struct AssociationEventData {
    schema_version: AssociationEventVersion,
    id: AssociationEventId,
    conversation_id: ConversationId,
    conversation: ConversationRef,
    #[serde(with = "time::serde::rfc3339")]
    effective_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    recorded_at: OffsetDateTime,
    action: AssociationAction,
    source: AssociationSource,
    authority: AssociationAuthority,
    target: AssociationTarget,
    reason: String,
    idempotency_key: String,
    #[serde(default)]
    supersedes_event_id: Option<AssociationEventId>,
    evidence: Vec<AssociationEvidence>,
}

impl TryFrom<AssociationEventData> for AssociationEvent {
    type Error = AssociationEventError;

    fn try_from(value: AssociationEventData) -> Result<Self, Self::Error> {
        if value.schema_version != AssociationEventVersion::V1 {
            return Err(AssociationEventError::UnsupportedVersion);
        }
        match (value.action, value.source, value.authority) {
            (
                AssociationAction::Transition,
                AssociationSource::ExplicitIntegration,
                AssociationAuthority::ExplicitIntegration,
            )
            | (
                AssociationAction::Assign | AssociationAction::Correct | AssociationAction::Confirm,
                AssociationSource::Manual,
                AssociationAuthority::Manual,
            ) => {}
            (_, AssociationSource::ExplicitIntegration, _) => {
                return Err(AssociationEventError::UnsupportedAuthority);
            }
            (_, AssociationSource::Manual, _) => {
                return Err(AssociationEventError::UnsupportedAction);
            }
        }
        Self::build(Self {
            schema_version: value.schema_version,
            id: value.id,
            conversation_id: value.conversation_id,
            conversation: value.conversation,
            effective_at: value.effective_at,
            recorded_at: value.recorded_at,
            action: value.action,
            source: value.source,
            authority: value.authority,
            target: value.target,
            reason: value.reason,
            idempotency_key: value.idempotency_key,
            supersedes_event_id: value.supersedes_event_id,
            evidence: value.evidence,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationEventError {
    EmptyReason,
    EmptyIdempotencyKey,
    MissingEvidence,
    UnsupportedVersion,
    UnsupportedAction,
    UnsupportedSource,
    UnsupportedAuthority,
}

impl Display for AssociationEventError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyReason => formatter.write_str("association reason cannot be empty"),
            Self::EmptyIdempotencyKey => {
                formatter.write_str("association idempotency key cannot be empty")
            }
            Self::MissingEvidence => formatter.write_str("association event must contain evidence"),
            Self::UnsupportedVersion => {
                formatter.write_str("association event schema version is not supported")
            }
            Self::UnsupportedAction => {
                formatter.write_str("association event action is not supported")
            }
            Self::UnsupportedSource => {
                formatter.write_str("association event source is not supported")
            }
            Self::UnsupportedAuthority => {
                formatter.write_str("association event authority is not supported")
            }
        }
    }
}

impl Error for AssociationEventError {}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde_json::json;
    use time::OffsetDateTime;

    use super::{
        AssociationAction, AssociationAuthority, AssociationEvent, AssociationEventError,
        AssociationEvidence, AssociationEvidenceKind, AssociationSource, AssociationTarget,
        NewAssociationEvent, NewManualAssociationEvent,
    };
    use crate::{
        AssociationEventId, ConversationId, ConversationRef, RepositoryId, Tool, WorkItemId,
        WorktreeId,
    };

    fn event() -> AssociationEvent {
        let observed_at = OffsetDateTime::parse(
            "2026-08-20T09:30:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("the fixture timestamp should be valid");
        let target = AssociationTarget::new(
            RepositoryId::from_str("31ff38a2-7614-4f82-8955-4df64670f68d")
                .expect("the repository ID should be valid"),
            WorkItemId::from_str("690f0bf4-8c4f-43c9-a55f-4a025f746c90")
                .expect("the work_item ID should be valid"),
            WorktreeId::from_str("89da50f1-873f-42fe-9124-7c8ae86a79d6")
                .expect("the worktree ID should be valid"),
            Some("refs/heads/feature/workboard".to_owned()),
        )
        .expect("the target should be valid");
        let evidence = AssociationEvidence::new(
            AssociationEvidenceKind::CallerIdentity,
            observed_at,
            "thread-123",
            Some("CODEX_THREAD_ID".to_owned()),
        )
        .expect("the evidence should be valid");

        AssociationEvent::new(NewAssociationEvent {
            id: AssociationEventId::from_str("85510b8a-5fe1-4c11-9025-6ebd28692d4d")
                .expect("the event ID should be valid"),
            conversation_id: ConversationId::from_str("68112bd1-1307-45ef-8719-d0019520b396")
                .expect("the conversation ID should be valid"),
            conversation: ConversationRef::new(Tool::Codex, "thread-123")
                .expect("the conversation should be valid"),
            effective_at: observed_at,
            recorded_at: observed_at,
            target,
            reason: "worktree-created".to_owned(),
            idempotency_key: "transition-123".to_owned(),
            evidence: vec![evidence],
        })
        .expect("the event should be valid")
    }

    #[test]
    fn serialises_the_versioned_event_contract() {
        let value = serde_json::to_value(event()).expect("the event should serialise");

        assert_eq!(
            value,
            json!({
                "schema_version": 1,
                "id": "85510b8a-5fe1-4c11-9025-6ebd28692d4d",
                "conversation_id": "68112bd1-1307-45ef-8719-d0019520b396",
                "conversation": {
                    "tool": "codex",
                    "native_id": "thread-123"
                },
                "effective_at": "2026-08-20T09:30:00Z",
                "recorded_at": "2026-08-20T09:30:00Z",
                "action": "transition",
                "source": "explicit_integration",
                "authority": "explicit_integration",
                "target": {
                    "repository_id": "31ff38a2-7614-4f82-8955-4df64670f68d",
                    "work_item_id": "690f0bf4-8c4f-43c9-a55f-4a025f746c90",
                    "worktree_id": "89da50f1-873f-42fe-9124-7c8ae86a79d6",
                    "branch": "refs/heads/feature/workboard"
                },
                "reason": "worktree-created",
                "idempotency_key": "transition-123",
                "evidence": [{
                    "kind": "caller_identity",
                    "observed_at": "2026-08-20T09:30:00Z",
                    "value": "thread-123",
                    "source_locator": "CODEX_THREAD_ID"
                }]
            })
        );
    }

    #[test]
    fn round_trips_the_event_contract() {
        let event = event();
        let encoded = serde_json::to_string(&event).expect("the event should serialise");
        let decoded: AssociationEvent =
            serde_json::from_str(&encoded).expect("the event should deserialise");

        assert_eq!(decoded, event);
    }

    #[test]
    fn rejects_an_unknown_schema_version() {
        let mut value = serde_json::to_value(event()).expect("the event should serialise");
        value["schema_version"] = json!(2);

        let error = serde_json::from_value::<AssociationEvent>(value)
            .expect_err("an unknown version should be rejected");

        assert!(error.to_string().contains("version 2 is not supported"));
    }

    #[test]
    fn rejects_an_event_without_evidence() {
        let mut value = serde_json::to_value(event()).expect("the event should serialise");
        value["evidence"] = json!([]);

        let error = serde_json::from_value::<AssociationEvent>(value)
            .expect_err("an event without evidence should be rejected");

        assert!(error.to_string().contains("must contain evidence"));
    }

    #[test]
    fn rejects_a_blank_idempotency_key() {
        let event = event();

        let result = AssociationEvent::new(NewAssociationEvent {
            id: event.id(),
            conversation_id: event.conversation_id(),
            conversation: event.conversation().clone(),
            effective_at: event.effective_at(),
            recorded_at: event.recorded_at(),
            target: event.target().clone(),
            reason: event.reason().to_owned(),
            idempotency_key: " ".to_owned(),
            evidence: event.evidence().to_vec(),
        });

        assert_eq!(result, Err(AssociationEventError::EmptyIdempotencyKey));
    }

    #[test]
    fn round_trips_a_superseding_manual_correction() {
        let explicit = event();
        let evidence = AssociationEvidence::new(
            AssociationEvidenceKind::ManualDecision,
            explicit.recorded_at(),
            "codex:thread-123",
            Some("workboard:manual".to_owned()),
        )
        .expect("the manual evidence should be valid");
        let correction = AssociationEvent::new_manual(NewManualAssociationEvent {
            id: AssociationEventId::generate(),
            conversation_id: explicit.conversation_id(),
            conversation: explicit.conversation().clone(),
            effective_at: explicit.effective_at(),
            recorded_at: explicit.recorded_at(),
            action: AssociationAction::Correct,
            target: explicit.target().clone(),
            reason: "manual-correction".to_owned(),
            idempotency_key: "manual-correction-1".to_owned(),
            supersedes_event_id: Some(explicit.id()),
            evidence: vec![evidence],
        })
        .expect("the correction should be valid");

        let encoded = serde_json::to_string(&correction).expect("the event should serialise");
        let decoded: AssociationEvent =
            serde_json::from_str(&encoded).expect("the event should deserialise");

        assert_eq!(decoded, correction);
        assert_eq!(decoded.source(), AssociationSource::Manual);
        assert_eq!(decoded.authority(), AssociationAuthority::Manual);
        assert_eq!(decoded.supersedes_event_id(), Some(explicit.id()));
    }

    #[test]
    fn authority_tiers_are_strictly_ordered() {
        assert!(
            AssociationAuthority::Manual.tier() > AssociationAuthority::ExplicitIntegration.tier()
        );
        assert!(
            AssociationAuthority::ExplicitIntegration.tier()
                > AssociationAuthority::ExactObservation.tier()
        );
        assert!(
            AssociationAuthority::ExactObservation.tier()
                > AssociationAuthority::CorrelatedInference.tier()
        );
    }
}
