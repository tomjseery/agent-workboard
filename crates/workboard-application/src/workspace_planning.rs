use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;
use workboard_core::{
    EpicId, FeatureId, HierarchyOwner, ManagedSessionRole, RepositoryId, Slug, WorkspaceId,
};

use crate::AppError;
use crate::storage::SqliteStore;
use crate::workflow_operations::{WorkflowOperationService, WorkflowPrincipal};

/// A workspace-planning proposal body is Markdown the planner authored or imported. It is data,
/// never a command, and it is bounded so one session cannot flood the planning store.
pub const MAX_PROPOSAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceProposalKind {
    CreateEpic,
    ImportEpicResearch,
    CreateFeature,
}

impl WorkspaceProposalKind {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::CreateEpic => "create_epic",
            Self::ImportEpicResearch => "import_epic_research",
            Self::CreateFeature => "create_feature",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "create_epic" => Ok(Self::CreateEpic),
            "import_epic_research" => Ok(Self::ImportEpicResearch),
            "create_feature" => Ok(Self::CreateFeature),
            _ => Err(AppError::Domain(
                "workspace planning proposal kind is invalid".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceProposalStatus {
    AwaitingApproval,
    Approved,
    Rejected,
}

impl WorkspaceProposalStatus {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::AwaitingApproval => "awaiting_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            _ => Err(AppError::Domain(
                "workspace planning proposal status is invalid".to_owned(),
            )),
        }
    }
}

/// Every source the planner actually read, so an approved proposal records what it was derived
/// from rather than asking a reader to trust the summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSource {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeEpic {
    pub title: String,
    pub slug: Option<Slug>,
    pub body: String,
    pub idempotency_key: String,
    pub proposed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeEpicResearch {
    pub title: String,
    pub slug: Option<Slug>,
    pub body: String,
    #[serde(default)]
    pub sources: Vec<ResearchSource>,
    pub idempotency_key: String,
    pub proposed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeFeature {
    pub epic_id: EpicId,
    pub title: String,
    pub slug: Option<Slug>,
    pub outcome: String,
    pub idempotency_key: String,
    pub proposed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProposal {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub repository_id: RepositoryId,
    pub kind: WorkspaceProposalKind,
    pub status: WorkspaceProposalStatus,
    pub title: String,
    pub observed_revision: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProposalOutcome {
    pub proposal_id: Uuid,
    pub kind: WorkspaceProposalKind,
    pub status: WorkspaceProposalStatus,
    pub title: String,
}

/// Recorded when an approved proposal has been turned into real hierarchy, so replaying an
/// approval returns the original identities instead of creating a second Epic or Feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProposalDecision {
    pub proposal_id: Uuid,
    pub kind: WorkspaceProposalKind,
    pub status: WorkspaceProposalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<EpicId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<FeatureId>,
}

pub struct WorkspacePlanningService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> WorkspacePlanningService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn propose_epic(
        &mut self,
        workflow_token: &str,
        request: ProposeEpic,
    ) -> Result<WorkspaceProposalOutcome, AppError> {
        let payload = serde_json::to_value(&request)?;
        self.submit(
            workflow_token,
            WorkspaceProposalKind::CreateEpic,
            &request.title,
            &request.body,
            &request.idempotency_key,
            request.proposed_at,
            payload,
        )
    }

    pub fn propose_epic_research(
        &mut self,
        workflow_token: &str,
        request: ProposeEpicResearch,
    ) -> Result<WorkspaceProposalOutcome, AppError> {
        if request.sources.is_empty() {
            return Err(AppError::PlanningDocumentInvalid(
                "an Epic research proposal must record the sources it was read from".to_owned(),
            ));
        }
        for source in &request.sources {
            if source.path.trim().is_empty() || source.path.chars().any(char::is_control) {
                return Err(AppError::PlanningDocumentInvalid(
                    "a research source path is empty or unsafe".to_owned(),
                ));
            }
            if !is_content_hash(&source.content_hash) {
                return Err(AppError::PlanningDocumentInvalid(
                    "a research source hash is not a SHA-256 digest".to_owned(),
                ));
            }
        }
        let payload = serde_json::to_value(&request)?;
        self.submit(
            workflow_token,
            WorkspaceProposalKind::ImportEpicResearch,
            &request.title,
            &request.body,
            &request.idempotency_key,
            request.proposed_at,
            payload,
        )
    }

    pub fn propose_feature(
        &mut self,
        workflow_token: &str,
        request: ProposeFeature,
    ) -> Result<WorkspaceProposalOutcome, AppError> {
        let payload = serde_json::to_value(&request)?;
        let outcome = self.submit(
            workflow_token,
            WorkspaceProposalKind::CreateFeature,
            &request.title,
            &request.outcome,
            &request.idempotency_key,
            request.proposed_at,
            payload,
        )?;
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    fn submit(
        &mut self,
        workflow_token: &str,
        kind: WorkspaceProposalKind,
        title: &str,
        body: &str,
        idempotency_key: &str,
        proposed_at: OffsetDateTime,
        payload: serde_json::Value,
    ) -> Result<WorkspaceProposalOutcome, AppError> {
        validate_title(title)?;
        validate_body(body)?;
        if idempotency_key.trim().is_empty()
            || idempotency_key.len() > 512
            || idempotency_key.chars().any(char::is_control)
        {
            return Err(AppError::EmptyIdempotencyKey);
        }

        let principal =
            WorkflowOperationService::new(self.store).authenticate(workflow_token, proposed_at)?;
        let workspace_id = authorise_workspace_planner(&principal)?;
        let repository =
            WorkflowOperationService::new(self.store).assigned_repository(&principal)?;

        let payload_json = serde_json::to_string(&payload)?;
        let created_at = timestamp(proposed_at)?;
        let title = title.trim().to_owned();
        let session_id = principal.session_id;
        let head = repository.head.clone();
        let repository_id = repository.repository_id;

        self.store.write(|transaction| {
            if let Some((id, existing_kind, existing_payload, status)) = transaction
                .query_row(
                    "SELECT id, kind, payload_json, status
                     FROM workspace_planning_proposals WHERE idempotency_key = ?1",
                    [idempotency_key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?
            {
                // Replaying a key with identical content returns the original proposal; replaying
                // it with different content is a conflict, never a silent second proposal.
                if existing_kind != kind.wire_name() || existing_payload != payload_json {
                    return Err(AppError::IdempotencyConflict);
                }
                return Ok(WorkspaceProposalOutcome {
                    proposal_id: parse_uuid(&id)?,
                    kind,
                    status: WorkspaceProposalStatus::parse(&status)?,
                    title: title.clone(),
                });
            }
            let proposal_id = Uuid::new_v4();
            transaction.execute(
                "INSERT INTO workspace_planning_proposals (
                     id, workspace_id, repository_id, session_id, kind, payload_json,
                     idempotency_key, observed_revision, status, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'awaiting_approval', ?9)",
                params![
                    proposal_id.to_string(),
                    workspace_id.to_string(),
                    repository_id.to_string(),
                    session_id.to_string(),
                    kind.wire_name(),
                    payload_json,
                    idempotency_key,
                    head,
                    created_at,
                ],
            )?;
            Ok(WorkspaceProposalOutcome {
                proposal_id,
                kind,
                status: WorkspaceProposalStatus::AwaitingApproval,
                title: title.clone(),
            })
        })
    }

    pub fn list(&self, workspace_id: WorkspaceId) -> Result<Vec<WorkspaceProposal>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, repository_id, kind, status, payload_json, observed_revision,
                        created_at, decided_at
                 FROM workspace_planning_proposals
                 WHERE workspace_id = ?1
                 ORDER BY created_at DESC, id",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(
                        id,
                        repository_id,
                        kind,
                        status,
                        payload_json,
                        observed_revision,
                        created_at,
                        decided_at,
                    )| {
                        let payload: serde_json::Value = serde_json::from_str(&payload_json)?;
                        Ok(WorkspaceProposal {
                            id: parse_uuid(&id)?,
                            workspace_id,
                            repository_id: parse_id(&repository_id)?,
                            kind: WorkspaceProposalKind::parse(&kind)?,
                            status: WorkspaceProposalStatus::parse(&status)?,
                            title: payload
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            observed_revision,
                            created_at,
                            decided_at,
                            payload,
                        })
                    },
                )
                .collect()
        })
    }

    pub fn read(&self, proposal_id: Uuid) -> Result<WorkspaceProposal, AppError> {
        let workspace_id: WorkspaceId = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT workspace_id FROM workspace_planning_proposals WHERE id = ?1",
                    [proposal_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(AppError::WorkspacePlanningProposalNotFound)
                .and_then(|value| parse_id(&value))
        })?;
        self.list(workspace_id)?
            .into_iter()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or(AppError::WorkspacePlanningProposalNotFound)
    }

    /// Records the user's decision. Approval of an already-approved proposal returns the original
    /// decision so a retry never produces a second Epic or Feature.
    pub fn decide(
        &mut self,
        proposal_id: Uuid,
        status: WorkspaceProposalStatus,
        decided_at: OffsetDateTime,
        outcome: Option<&WorkspaceProposalDecision>,
    ) -> Result<WorkspaceProposalDecision, AppError> {
        if status == WorkspaceProposalStatus::AwaitingApproval {
            return Err(AppError::Domain(
                "a decision must approve or reject the proposal".to_owned(),
            ));
        }
        let decided = timestamp(decided_at)?;
        let outcome_json = outcome.map(serde_json::to_string).transpose()?;
        self.store.write(|transaction| {
            let (kind, existing_status, existing_outcome) = transaction
                .query_row(
                    "SELECT kind, status, outcome_json
                     FROM workspace_planning_proposals WHERE id = ?1",
                    [proposal_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(AppError::WorkspacePlanningProposalNotFound)?;
            let kind = WorkspaceProposalKind::parse(&kind)?;
            let existing_status = WorkspaceProposalStatus::parse(&existing_status)?;
            if existing_status != WorkspaceProposalStatus::AwaitingApproval {
                if existing_status != status {
                    return Err(AppError::WorkspacePlanningProposalDecided);
                }
                return existing_outcome
                    .as_deref()
                    .map(|value| serde_json::from_str(value).map_err(AppError::from))
                    .unwrap_or_else(|| {
                        Ok(WorkspaceProposalDecision {
                            proposal_id,
                            kind,
                            status,
                            epic_id: None,
                            feature_id: None,
                        })
                    });
            }
            transaction.execute(
                "UPDATE workspace_planning_proposals
                 SET status = ?2, decided_at = ?3, outcome_json = ?4
                 WHERE id = ?1 AND status = 'awaiting_approval'",
                params![
                    proposal_id.to_string(),
                    status.wire_name(),
                    decided,
                    outcome_json,
                ],
            )?;
            Ok(outcome.cloned().unwrap_or(WorkspaceProposalDecision {
                proposal_id,
                kind,
                status,
                epic_id: None,
                feature_id: None,
            }))
        })
    }
}

/// Only a workspace-planning session may submit these proposals, and only for the workspace it is
/// bound to. Every other role and owner fails closed.
fn authorise_workspace_planner(principal: &WorkflowPrincipal) -> Result<WorkspaceId, AppError> {
    match (principal.role, principal.owner) {
        (ManagedSessionRole::WorkspacePlanning, HierarchyOwner::Workspace(workspace_id)) => {
            Ok(workspace_id)
        }
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
}

fn validate_title(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(AppError::PlanningDocumentInvalid(
            "proposal title is empty, too long, or contains control characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_body(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::PlanningDocumentInvalid(
            "proposal body cannot be blank".to_owned(),
        ));
    }
    if value.len() > MAX_PROPOSAL_BYTES {
        return Err(AppError::PlanningDocumentInvalid(
            "proposal body exceeds 2 MiB".to_owned(),
        ));
    }
    if value.contains('\0') {
        return Err(AppError::PlanningDocumentInvalid(
            "proposal body contains a NUL byte".to_owned(),
        ));
    }
    Ok(())
}

fn is_content_hash(value: &str) -> bool {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn timestamp(value: OffsetDateTime) -> Result<String, AppError> {
    value
        .format(&Rfc3339)
        .map_err(|error| AppError::Domain(error.to_string()))
}

fn parse_uuid(value: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value).map_err(|error| AppError::Domain(error.to_string()))
}

fn parse_id<T>(value: &str) -> Result<T, AppError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error: T::Err| AppError::Domain(error.to_string()))
}
