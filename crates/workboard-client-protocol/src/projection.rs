use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use ts_rs::TS;

use crate::{
    AssociationId, AvailableAction, BoardViewId, CheckoutId, CheckoutPathId, DocumentId, EntityRef,
    EpicId, FeatureId, HierarchyRef, RepositoryId, RepositoryPathId, SessionId, WorkItemId,
    WorkspaceId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReference {
    pub id: WorkspaceId,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryReference {
    pub id: RepositoryId,
    pub workspace_id: WorkspaceId,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EpicReference {
    pub id: EpicId,
    pub workspace_id: WorkspaceId,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FeatureReference {
    pub id: FeatureId,
    pub epic_id: EpicId,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemReference {
    pub id: WorkItemId,
    pub feature_id: FeatureId,
    pub key: String,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHierarchy {
    pub workspace: WorkspaceReference,
    pub repositories: Vec<RepositoryReference>,
    pub epics: Vec<HierarchyEpic>,
    pub features: Vec<HierarchyFeature>,
    pub work_items: Vec<HierarchyWorkItem>,
    pub recent_entities: Vec<EntityRef>,
    pub focused_entity: Option<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyEpic {
    pub epic: EpicReference,
    pub repository_ids: Vec<RepositoryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyFeature {
    pub feature: FeatureReference,
    pub repository_ids: Vec<RepositoryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyWorkItem {
    pub work_item: WorkItemReference,
    pub repository_ids: Vec<RepositoryId>,
    pub status: WorkItemStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardViewDefinition {
    pub id: BoardViewId,
    pub workspace_id: WorkspaceId,
    pub title: String,
    pub filters: BoardViewFilters,
    pub grouping: BoardViewGrouping,
    pub sort: BoardViewSort,
    pub density: BoardViewDensity,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardQuery {
    pub cursor: Option<String>,
    pub limit: usize,
    pub query: Option<String>,
    pub repository_ids: Vec<RepositoryId>,
    pub statuses: Vec<WorkItemStatus>,
    pub lane_keys: Vec<String>,
    pub sort: BoardViewSort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttentionQuery {
    pub cursor: Option<String>,
    pub limit: usize,
    pub repository_ids: Vec<RepositoryId>,
    pub reason_codes: Vec<AttentionReasonCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardPage {
    pub lanes: Vec<BoardLaneProjection>,
    pub cards: Vec<BoardCardProjection>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttentionPage {
    pub entries: Vec<AttentionEntryProjection>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardLaneProjection {
    pub key: String,
    pub title: String,
    pub position: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardCardProjection {
    pub work_item: WorkItemReference,
    pub feature: FeatureReference,
    pub status: WorkItemStatus,
    pub lane_key: String,
    pub lane_position: usize,
    pub lane_count: usize,
    pub dependency_readiness: DependencyReadiness,
    pub blocked_by: Vec<BlockedByEvidence>,
    pub parallel_readiness: ParallelReadiness,
    pub repositories: Vec<RepositoryReference>,
    pub session_summary: SessionSummary,
    pub attention_reasons: Vec<AttentionReason>,
    pub revision: u64,
    pub available_actions: Vec<AvailableAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttentionEntryProjection {
    pub owner: EntityRef,
    pub title: String,
    pub subtitle: String,
    pub repositories: Vec<RepositoryReference>,
    pub card: Option<BoardCardProjection>,
    pub reasons: Vec<AttentionReason>,
    pub revision: u64,
    pub available_actions: Vec<AvailableAction>,
    pub position: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DependencyReadiness {
    Ready,
    Waiting,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BlockedByEvidence {
    pub work_item: WorkItemReference,
    pub status: WorkItemStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ParallelReadiness {
    pub group_key: String,
    pub ready_count: usize,
    pub waiting_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub total: usize,
    pub active: usize,
    pub idle: usize,
    pub unknown: usize,
    pub providers: Vec<Provider>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttentionReason {
    pub code: AttentionReasonCode,
    pub rank: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReasonCode {
    ApprovalRequired,
    RevisionRequested,
    ReconciliationRequired,
    Blocked,
    CheckpointDue,
    InterruptedOperation,
    RecoveryConflict,
    StaleOrUnknownSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardViewFilters {
    pub query: Option<String>,
    pub repository_ids: Vec<RepositoryId>,
    pub statuses: Vec<WorkItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardViewGrouping {
    pub kind: BoardViewGroupingKind,
    pub lanes: Vec<BoardViewLaneDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardViewLaneDefinition {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BoardViewGroupingKind {
    Hierarchy,
    Repository,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardViewSort {
    pub field: BoardViewSortField,
    pub direction: BoardViewSortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BoardViewSortField {
    Title,
    Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BoardViewSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BoardViewDensity {
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReference {
    pub id: SessionId,
    pub provider: Provider,
    pub native_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub workspace: WorkspaceReference,
    pub repository_count: usize,
    pub epic_count: usize,
    pub feature_count: usize,
    pub work_item_count: usize,
    pub session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyChildren {
    pub parent: HierarchyRef,
    pub children: Vec<HierarchyNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HierarchyNode {
    Repository(RepositoryReference),
    Epic(EpicReference),
    Feature(FeatureReference),
    WorkItem(WorkItemReference),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardSnapshot {
    pub workspace: WorkspaceProjection,
    pub repositories: Vec<RepositoryProjection>,
    pub epics: Vec<EpicProjection>,
    pub features: Vec<FeatureProjection>,
    pub work_items: Vec<WorkItemProjection>,
    pub documents: Vec<DocumentProjection>,
    pub checkouts: Vec<CheckoutProjection>,
    pub effective_checkouts: Vec<EffectiveCheckoutProjection>,
    pub sessions: Vec<SessionProjection>,
    pub associations: Vec<SessionAssociationProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceProjection {
    pub id: WorkspaceId,
    pub slug: String,
    pub title: String,
    pub planning_store_repository_id: RepositoryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryProjection {
    pub id: RepositoryId,
    pub workspace_id: WorkspaceId,
    pub slug: String,
    pub title: String,
    pub git_common_directory: String,
    pub default_branch: Option<String>,
    pub remotes: Vec<RepositoryRemoteProjection>,
    pub paths: Vec<RepositoryPathProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRemoteProjection {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryPathProjection {
    pub id: RepositoryPathId,
    pub path: String,
    #[serde(with = "time::serde::rfc3339")]
    pub observed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub superseded_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicProjection {
    pub id: EpicId,
    pub workspace_id: WorkspaceId,
    pub slug: String,
    pub title: String,
    pub document_id: DocumentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureProjection {
    pub id: FeatureId,
    pub epic_id: EpicId,
    pub slug: String,
    pub title: String,
    pub document_id: Option<DocumentId>,
    pub state: WorkflowState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemProjection {
    pub id: WorkItemId,
    pub feature_id: FeatureId,
    pub key: String,
    pub slug: String,
    pub title: String,
    pub status: WorkItemStatus,
    pub document_id: DocumentId,
    pub repository_ids: Vec<RepositoryId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum OwnerProjection {
    Epic(EpicId),
    Feature(FeatureId),
    WorkItem(WorkItemId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentProjection {
    pub id: DocumentId,
    pub owner: OwnerProjection,
    pub repository_id: RepositoryId,
    pub relative_path: String,
    pub content_hash: String,
    pub observed_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutProjection {
    pub id: CheckoutId,
    pub repository_id: RepositoryId,
    pub git_worktree_identity: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub availability: CheckoutAvailability,
    pub replaces_checkout_id: Option<CheckoutId>,
    pub paths: Vec<CheckoutPathProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutPathProjection {
    pub id: CheckoutPathId,
    pub checkout_id: CheckoutId,
    pub path: String,
    #[serde(with = "time::serde::rfc3339")]
    pub observed_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub observed_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveCheckoutProjection {
    pub feature_id: FeatureId,
    pub work_item_id: Option<WorkItemId>,
    pub repository_id: RepositoryId,
    pub checkout_id: CheckoutId,
    pub inherited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProjection {
    pub id: SessionId,
    pub native: NativeSessionReference,
    #[serde(with = "time::serde::rfc3339")]
    pub discovered_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSessionReference {
    pub tool: Provider,
    pub native_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAssociationProjection {
    pub id: AssociationId,
    pub session_id: SessionId,
    pub owner: OwnerProjection,
    pub role: ManagedSessionRole,
    #[serde(with = "time::serde::rfc3339")]
    pub associated_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub associated_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Draft,
    WorktreePending,
    PlanningLaunchPending,
    PlanningActive,
    ProposalReady,
    AwaitingApproval,
    Publishing,
    Planned,
    WorkItemLaunchPending,
    WorkItemActive,
    ReconciliationRequired,
    Blocked,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutAvailability {
    Available,
    Missing,
    Deleted,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSessionRole {
    EpicNavigation,
    FeaturePlanning,
    WorkItemExecution,
    Debugging,
    Review,
}
