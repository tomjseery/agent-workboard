use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId, ConversationRef, DocumentId,
    EpicId, FeatureId, GitOperationKind, IntentStatus, LaunchIntentId, ManagedSessionRole,
    OperationIntentId, RepositoryId, RepositoryPathId, RestoreMembershipId, TerminalLayoutId,
    TerminalTabId, Tool, WorkItemId, WorkflowActor, WorkflowEventId, WorkflowRunId, WorkflowState,
    WorkspaceId,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    pub fn new(value: impl Into<String>) -> Result<Self, NameError> {
        let value = value.into();
        if value.is_empty() {
            return Err(NameError::Empty);
        }
        if value.len() > 100 {
            return Err(NameError::TooLong);
        }
        if value.starts_with('-')
            || value.ends_with('-')
            || value.chars().any(|character| {
                !character.is_ascii_lowercase() && !character.is_ascii_digit() && character != '-'
            })
        {
            return Err(NameError::InvalidSlug);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Slug {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Slug {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Slug> for String {
    fn from(value: Slug) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkItemKey(String);

impl WorkItemKey {
    pub fn new(value: impl Into<String>) -> Result<Self, NameError> {
        let value = value.into();
        if value.is_empty() {
            return Err(NameError::Empty);
        }
        if value.len() > 300 {
            return Err(NameError::TooLong);
        }
        if value.starts_with('/')
            || value.ends_with('/')
            || value.split('/').any(|segment| Slug::new(segment).is_err())
        {
            return Err(NameError::InvalidKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for WorkItemKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for WorkItemKey {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<WorkItemKey> for String {
    fn from(value: WorkItemKey) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    InvalidSlug,
    InvalidKey,
}

impl Display for NameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("name cannot be empty"),
            Self::TooLong => formatter.write_str("name is too long"),
            Self::InvalidSlug => formatter.write_str(
                "slug must contain lowercase ASCII letters, digits, and interior hyphens",
            ),
            Self::InvalidKey => {
                formatter.write_str("key must be a slash-separated sequence of valid slugs")
            }
        }
    }
}

impl Error for NameError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub slug: Slug,
    pub title: String,
    pub planning_store_repository_id: RepositoryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub workspace_id: WorkspaceId,
    pub slug: Slug,
    pub title: String,
    pub git_common_directory: PathBuf,
    pub default_branch: Option<String>,
    pub remotes: Vec<RepositoryRemote>,
    pub paths: Vec<RepositoryPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRemote {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryPath {
    pub id: RepositoryPathId,
    pub path: PathBuf,
    pub observed_at: OffsetDateTime,
    pub superseded_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epic {
    pub id: EpicId,
    pub workspace_id: WorkspaceId,
    pub slug: Slug,
    pub title: String,
    pub document_id: DocumentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub epic_id: EpicId,
    pub slug: Slug,
    pub title: String,
    pub document_id: Option<DocumentId>,
    pub state: WorkflowState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Backlog,
    Ready,
    InProgress,
    Blocked,
    Review,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: WorkItemId,
    pub feature_id: FeatureId,
    pub key: WorkItemKey,
    pub slug: Slug,
    pub title: String,
    pub status: WorkItemStatus,
    pub document_id: DocumentId,
    pub repository_ids: Vec<RepositoryId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum HierarchyOwner {
    Workspace(WorkspaceId),
    Epic(EpicId),
    Feature(FeatureId),
    WorkItem(WorkItemId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownDocument {
    pub id: DocumentId,
    pub owner: HierarchyOwner,
    pub repository_id: RepositoryId,
    pub relative_path: PathBuf,
    pub content_hash: String,
    pub observed_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRevision {
    pub document_id: DocumentId,
    pub revision: u64,
    pub content_hash: String,
    pub observed_commit: Option<String>,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutAvailability {
    Available,
    Missing,
    Deleted,
    Replaced,
}

pub const CHECKOUT_READINESS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutPurpose {
    FeatureIntegration,
    WorkItemWrite,
    WriterSession,
    ReadOnlyShared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutAccessMode {
    WriteIsolated,
    ReadOnlyShared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutEvidenceKind {
    IntentRecorded,
    Materialized,
    Restored,
    GitResolved,
    IdentityVerified,
    AvailabilityCorrected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutReconciliationEvidence {
    pub kind: CheckoutEvidenceKind,
    pub observed_at: OffsetDateTime,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutReadiness {
    pub schema_version: u32,
    pub repository_id: RepositoryId,
    pub checkout_id: CheckoutId,
    pub checkout_path_id: CheckoutPathId,
    pub purpose: CheckoutPurpose,
    pub access_mode: CheckoutAccessMode,
    pub owner: HierarchyOwner,
    pub session_id: Option<ConversationId>,
    pub parent_feature_checkout_id: Option<CheckoutId>,
    pub base_revision: String,
    pub source_revision: String,
    pub path: PathBuf,
    pub git_worktree_identity: PathBuf,
    pub branch: Option<String>,
    pub head: String,
    pub availability: CheckoutAvailability,
    pub isolation_generation: u64,
    pub reconciliation_generation: u64,
    pub evidence: Vec<CheckoutReconciliationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    pub id: CheckoutId,
    pub repository_id: RepositoryId,
    pub git_worktree_identity: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub availability: CheckoutAvailability,
    pub replaces_checkout_id: Option<CheckoutId>,
    pub paths: Vec<CheckoutPathInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutPathInterval {
    pub id: CheckoutPathId,
    pub checkout_id: CheckoutId,
    pub path: PathBuf,
    pub observed_from: OffsetDateTime,
    pub observed_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveCheckout {
    pub feature_id: FeatureId,
    pub work_item_id: Option<WorkItemId>,
    pub repository_id: RepositoryId,
    pub checkout_id: CheckoutId,
    pub inherited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSession {
    pub id: ConversationId,
    pub native: ConversationRef,
    pub discovered_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSessionAssociation {
    pub id: AssociationIntervalId,
    pub session_id: ConversationId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub associated_from: OffsetDateTime,
    pub associated_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: WorkflowRunId,
    pub owner: HierarchyOwner,
    pub state: WorkflowState,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub id: WorkflowEventId,
    pub run_id: WorkflowRunId,
    pub sequence: u64,
    pub from_state: WorkflowState,
    pub to_state: WorkflowState,
    pub actor: WorkflowActor,
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationIntent {
    pub id: OperationIntentId,
    pub owner: HierarchyOwner,
    pub idempotency_key: String,
    pub kind: GitOperationKind,
    pub status: IntentStatus,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchIntent {
    pub id: LaunchIntentId,
    pub owner: HierarchyOwner,
    pub checkout_id: CheckoutId,
    pub tool: Tool,
    pub idempotency_key: String,
    pub status: IntentStatus,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreMembership {
    pub id: RestoreMembershipId,
    pub session_id: ConversationId,
    pub feature_id: FeatureId,
    pub active_from: OffsetDateTime,
    pub active_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLayout {
    pub id: TerminalLayoutId,
    pub workspace_id: WorkspaceId,
    pub captured_at: OffsetDateTime,
    pub tabs: Vec<TerminalTab>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTab {
    pub id: TerminalTabId,
    pub layout_id: TerminalLayoutId,
    pub feature_id: FeatureId,
    pub session_id: ConversationId,
    pub position: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub workspace: Workspace,
    pub repositories: Vec<Repository>,
    pub epics: Vec<Epic>,
    pub features: Vec<Feature>,
    pub work_items: Vec<WorkItem>,
    pub documents: Vec<MarkdownDocument>,
    pub checkouts: Vec<Checkout>,
    pub effective_checkouts: Vec<EffectiveCheckout>,
    pub sessions: Vec<NativeSession>,
    pub associations: Vec<NativeSessionAssociation>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{HierarchyOwner, NameError, Slug, WorkItemKey};
    use crate::WorkItemId;

    #[test]
    fn validates_slugs_and_hierarchical_keys() {
        assert_eq!(
            Slug::new("venue-availability")
                .expect("valid slug")
                .as_str(),
            "venue-availability"
        );
        assert_eq!(Slug::new("Venue").unwrap_err(), NameError::InvalidSlug);
        assert_eq!(Slug::new("../venue").unwrap_err(), NameError::InvalidSlug);
        assert_eq!(
            WorkItemKey::new("launch/venue-availability/api")
                .expect("valid key")
                .as_str(),
            "launch/venue-availability/api"
        );
        assert_eq!(
            WorkItemKey::new("launch//api").unwrap_err(),
            NameError::InvalidKey
        );
    }

    #[test]
    fn hierarchy_owner_has_a_stable_tagged_shape() {
        let id = WorkItemId::generate();
        assert_eq!(
            serde_json::to_value(HierarchyOwner::WorkItem(id)).expect("owner serialises"),
            json!({ "kind": "work_item", "id": id })
        );
    }
}
