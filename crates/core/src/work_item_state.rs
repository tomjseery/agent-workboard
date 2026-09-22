use serde::{Deserialize, Serialize};

use crate::{NextActionKind, WorkItemId, WorkItemStatus};

pub const WORK_ITEM_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemStateUpdate {
    pub schema_version: u32,
    pub work_item_id: WorkItemId,
    pub expected_revision: u64,
    pub expected_document_revision: u64,
    pub current_state: String,
    pub next_action: WorkItemNextAction,
    pub blockers: Vec<WorkItemBlocker>,
    pub decisions: Vec<WorkItemDecision>,
    pub verification: Vec<WorkItemVerification>,
    pub review: WorkItemReviewState,
    pub delivery: WorkItemDeliveryState,
    pub status: WorkItemStatus,
    pub terminal_intent: Option<WorkItemTerminalIntent>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemNextAction {
    pub kind: NextActionKind,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemBlocker {
    pub description: String,
    pub owner: String,
    pub unblock_action: String,
    pub resume_when: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemDecision {
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemVerification {
    pub check: String,
    pub result: WorkItemVerificationResult,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemVerificationResult {
    Passed,
    Failed,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemReviewState {
    pub status: WorkItemReviewStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemReviewStatus {
    NotStarted,
    InProgress,
    ChangesRequested,
    Ready,
    Accepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkItemDeliveryState {
    pub status: WorkItemDeliveryStatus,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemDeliveryStatus {
    NotStarted,
    InProgress,
    Blocked,
    Ready,
    Delivered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemTerminalIntent {
    Complete,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemState {
    pub schema_version: u32,
    pub work_item_id: WorkItemId,
    pub revision: u64,
    pub document_revision: u64,
    pub current_state: String,
    pub next_action: WorkItemNextAction,
    pub blockers: Vec<WorkItemBlocker>,
    pub decisions: Vec<WorkItemDecision>,
    pub verification: Vec<WorkItemVerification>,
    pub review: WorkItemReviewState,
    pub delivery: WorkItemDeliveryState,
    pub status: WorkItemStatus,
    pub terminal_intent: Option<WorkItemTerminalIntent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatePublicationStatus {
    Pending,
    ReconciliationRequired,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemStateOutcome {
    pub checkpoint_id: crate::WorkItemCheckpointId,
    pub state: WorkItemState,
    pub publication_status: WorkItemStatePublicationStatus,
    pub published_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemStateView {
    pub state: Option<WorkItemState>,
    pub document_revision: u64,
    pub reconciliation: Option<WorkItemStateReconciliation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemStateReconciliation {
    pub checkpoint_id: crate::WorkItemCheckpointId,
    pub idempotency_key: String,
    pub expected_document_hash: String,
    pub candidate_document_hash: String,
    pub reason: String,
}
