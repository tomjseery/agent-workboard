use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    CheckoutAccessMode, CheckoutAvailability, CheckoutId, CheckoutPurpose, ConversationId,
    DocumentKind, Epic, Feature, HierarchyOwner, LiveStatus, ManagedSessionRequestId,
    ManagedSessionRole, MarkdownDocument, NextActionKind, RepositoryId, Resumability, Tool,
    WorkItem, WorkItemCheckpointId, WorkItemId, WorkItemStatus, WorkspaceId,
};

use crate::AppError;
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPrincipal {
    pub workspace_id: WorkspaceId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub session_id: ConversationId,
    pub checkout_id: CheckoutId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignedContext {
    pub schema_version: u32,
    pub principal: WorkflowPrincipal,
    pub epic: Option<Epic>,
    pub feature: Option<Feature>,
    pub work_item: Option<WorkItem>,
    pub dependencies: Vec<AssignedDependency>,
    pub repositories: Vec<AssignedRepository>,
    pub documents: Vec<AssignedDocument>,
    pub sessions: Vec<AssignedSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignedRepository {
    pub repository_id: RepositoryId,
    pub checkout_id: CheckoutId,
    pub path: PathBuf,
    pub git_worktree_identity: String,
    pub branch: Option<String>,
    pub head: String,
    pub availability: CheckoutAvailability,
    pub purpose: Option<CheckoutPurpose>,
    pub access_mode: Option<CheckoutAccessMode>,
    pub parent_feature_checkout_id: Option<CheckoutId>,
    pub isolation_generation: Option<u64>,
    pub reconciliation_generation: Option<u64>,
    pub instructions: Vec<RepositoryInstruction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignedDocument {
    pub document: MarkdownDocument,
    pub kind: DocumentKind,
    pub path: PathBuf,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignedDependency {
    pub work_item: WorkItem,
    pub document: AssignedDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignedSession {
    pub session_id: ConversationId,
    pub provider: Tool,
    pub role: ManagedSessionRole,
    pub managed_status: String,
    pub live_status: Option<LiveStatus>,
    pub last_activity: Option<OffsetDateTime>,
    pub checkout_id: CheckoutId,
    pub checkout_path: PathBuf,
    pub branch: Option<String>,
    pub resumability: Resumability,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryInstructionKind {
    Agents,
    Claude,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryInstruction {
    pub kind: RepositoryInstructionKind,
    pub path: PathBuf,
    pub content_hash: String,
    pub observed_revision: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointWorkItem {
    pub work_item_id: WorkItemId,
    pub next_action: NextActionKind,
    pub summary: String,
    pub idempotency_key: String,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemCheckpointOutcome {
    pub checkpoint_id: WorkItemCheckpointId,
    pub work_item_id: WorkItemId,
    pub next_action: NextActionKind,
    pub status: WorkItemStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestManagedSession {
    pub work_item_id: WorkItemId,
    pub repository_id: RepositoryId,
    pub tool: Tool,
    pub idempotency_key: String,
    pub requested_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedSessionRequestOutcome {
    pub request_id: ManagedSessionRequestId,
    pub work_item_id: WorkItemId,
    pub repository_id: RepositoryId,
    pub tool: Tool,
    pub checkout_id: CheckoutId,
    pub working_directory: PathBuf,
    pub title: String,
    pub readiness_generation: Option<u64>,
    pub status: String,
}

pub fn work_item_bootstrap_prompt(work_item_id: WorkItemId) -> String {
    format!(
        "Use the installed Agent Workboard workflow to execute assigned Work item {work_item_id}. Read the assigned hierarchy and repository instructions through the typed operation before changing files. Work only within the assigned checkout and Work-item scope. Record durable progress, blockers, decisions, verification, and next action through Workboard checkpoints. Do not use repository planning ledgers as managed-session state."
    )
}

pub struct WorkflowOperationService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> WorkflowOperationService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn authenticate(
        &self,
        workflow_token: &str,
        observed_at: OffsetDateTime,
    ) -> Result<WorkflowPrincipal, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT intent.workspace_id, intent.epic_id, intent.feature_id,
                            intent.work_item_id, association.workspace_id,
                            association.epic_id, association.feature_id,
                            association.work_item_id, managed.role, intent.provider,
                            managed.session_id, managed.checkout_id
                     FROM launch_intents intent
                     JOIN managed_sessions managed ON managed.launch_intent_id = intent.id
                     JOIN native_session_associations association
                       ON association.session_id = managed.session_id
                      AND association.associated_until IS NULL
                     WHERE intent.workflow_token_hash = ?1
                       AND intent.workflow_token_expires_at > ?2
                       AND intent.status = 'bound' AND managed.managed_until IS NULL",
                    params![token_hash(workflow_token), timestamp(observed_at)],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, String>(11)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                intent_workspace,
                intent_epic,
                intent_feature,
                intent_work_item,
                associated_workspace,
                associated_epic,
                associated_feature,
                associated_work_item,
                role,
                tool,
                session_id,
                checkout_id,
            )) = row
            else {
                return Err(AppError::WorkflowOperationUnauthorized);
            };
            let intended = parse_association_owner(
                intent_workspace,
                intent_epic,
                intent_feature,
                intent_work_item,
            )?;
            let associated = parse_association_owner(
                associated_workspace,
                associated_epic,
                associated_feature,
                associated_work_item,
            )?;
            if intended != associated {
                return Err(AppError::WorkflowOperationUnauthorized);
            }
            Ok(WorkflowPrincipal {
                workspace_id: workspace_for_owner(connection, intended)?,
                owner: intended,
                role: parse_role(&role)?,
                tool: parse_tool(&tool)?,
                session_id: parse_id(&session_id)?,
                checkout_id: parse_id(&checkout_id)?,
            })
        })
    }

    pub fn assigned_repository(
        &self,
        principal: &WorkflowPrincipal,
    ) -> Result<AssignedRepository, AppError> {
        self.assigned_repository_checkout(principal, principal.checkout_id)
    }

    pub fn assigned_repository_checkout(
        &self,
        principal: &WorkflowPrincipal,
        checkout_id: CheckoutId,
    ) -> Result<AssignedRepository, AppError> {
        let row = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT checkout.repository_id, path.path,
                            checkout.git_worktree_identity, checkout.branch,
                            checkout.head, checkout.availability,
                            readiness.purpose, readiness.access_mode,
                            readiness.owner_kind, readiness.owner_id,
                            readiness.parent_feature_checkout_id,
                            readiness.isolation_generation,
                            readiness.reconciliation_generation
                     FROM checkouts checkout
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     LEFT JOIN checkout_readiness readiness
                       ON readiness.checkout_id = checkout.id
                     WHERE checkout.id = ?1",
                    [checkout_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, Option<String>>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, Option<String>>(10)?,
                            row.get::<_, Option<i64>>(11)?,
                            row.get::<_, Option<i64>>(12)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let Some((
            repository_id,
            path,
            git_worktree_identity,
            branch,
            head,
            availability,
            purpose,
            access_mode,
            readiness_owner_kind,
            readiness_owner_id,
            parent_feature_checkout_id,
            isolation_generation,
            reconciliation_generation,
        )) = row
        else {
            return Err(assigned_context_error(
                "assigned_checkout_missing",
                "the managed session checkout has no current path",
            ));
        };
        if availability != "available" {
            return Err(assigned_context_error(
                "assigned_checkout_unavailable",
                "the managed session checkout is not available",
            ));
        }
        if let HierarchyOwner::WorkItem(work_item_id) = principal.owner {
            let expected_owner_id = work_item_id.to_string();
            if readiness_owner_kind.as_deref() != Some("work_item")
                || readiness_owner_id.as_deref() != Some(expected_owner_id.as_str())
            {
                return Err(assigned_context_error(
                    "assigned_checkout_readiness_mismatch",
                    "the managed Work-item checkout readiness belongs to another owner",
                ));
            }
        }
        let repository_id = parse_id(&repository_id)?;
        let path = PathBuf::from(path);
        let head = head
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                assigned_context_error(
                    "assigned_checkout_revision_missing",
                    "the managed session checkout has no recorded revision",
                )
            })?;
        let root = canonical_checkout_root(&path)?;
        let instructions = repository_instructions(&root, principal.tool, &head)?;
        Ok(AssignedRepository {
            repository_id,
            checkout_id,
            path: root,
            git_worktree_identity,
            branch,
            head,
            availability: CheckoutAvailability::Available,
            purpose: purpose.as_deref().map(parse_wire).transpose()?,
            access_mode: access_mode.as_deref().map(parse_wire).transpose()?,
            parent_feature_checkout_id: parent_feature_checkout_id
                .as_deref()
                .map(parse_id)
                .transpose()?,
            isolation_generation: isolation_generation
                .map(u64::try_from)
                .transpose()
                .map_err(|error| AppError::Domain(error.to_string()))?,
            reconciliation_generation: reconciliation_generation
                .map(u64::try_from)
                .transpose()
                .map_err(|error| AppError::Domain(error.to_string()))?,
            instructions,
        })
    }

    pub fn assigned_documents(
        &self,
        principal: &WorkflowPrincipal,
        owners: &[HierarchyOwner],
    ) -> Result<Vec<AssignedDocument>, AppError> {
        self.store.read(|connection| {
            let planning_root = connection
                .query_row(
                    "SELECT path.path FROM repositories repository
                     JOIN repository_paths path
                       ON path.repository_id = repository.id AND path.observed_until IS NULL
                     WHERE repository.workspace_id = ?1 AND repository.is_planning_store = 1",
                    [principal.workspace_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    assigned_context_error(
                        "assigned_planning_store_missing",
                        "the assigned workspace has no current planning-store path",
                    )
                })?;
            let planning_root = canonical_checkout_root(Path::new(&planning_root))?;
            let mut statement = connection.prepare(
                "SELECT document.id, document.repository_id, document.epic_id,
                        document.feature_id, document.work_item_id, document.kind,
                        document.relative_path, document.content_hash,
                        document.observed_commit,
                        COALESCE(MAX(revision.revision), 1)
                 FROM documents document
                 JOIN repositories repository ON repository.id = document.repository_id
                 LEFT JOIN document_revisions revision ON revision.document_id = document.id
                 WHERE repository.workspace_id = ?1
                 GROUP BY document.id, document.repository_id, document.epic_id,
                          document.feature_id, document.work_item_id, document.kind,
                          document.relative_path, document.content_hash,
                          document.observed_commit
                 ORDER BY document.kind, document.relative_path",
            )?;
            let rows = statement
                .query_map([principal.workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .filter_map(
                    |(
                        id,
                        repository_id,
                        epic_id,
                        feature_id,
                        work_item_id,
                        kind,
                        relative_path,
                        content_hash,
                        observed_commit,
                        revision,
                    )| {
                        let owner = match parse_owner(epic_id, feature_id, work_item_id) {
                            Ok(owner) => owner,
                            Err(error) => return Some(Err(error)),
                        };
                        if !owners.contains(&owner) {
                            return None;
                        }
                        let relative_path = PathBuf::from(relative_path);
                        if relative_path.is_absolute()
                            || relative_path.components().any(|component| {
                                matches!(
                                    component,
                                    std::path::Component::ParentDir
                                        | std::path::Component::RootDir
                                        | std::path::Component::Prefix(_)
                                )
                            })
                        {
                            return Some(Err(assigned_context_error(
                                "assigned_document_path_invalid",
                                "an assigned document path is not safely relative",
                            )));
                        }
                        let path = planning_root.join(&relative_path);
                        let resolved = match fs::canonicalize(&path) {
                            Ok(path) if path.starts_with(&planning_root) && path.is_file() => path,
                            Ok(_) => {
                                return Some(Err(assigned_context_error(
                                    "assigned_document_path_invalid",
                                    format!(
                                        "an assigned document escapes the planning store: {}",
                                        path.display()
                                    ),
                                )));
                            }
                            Err(error) => {
                                return Some(Err(assigned_context_error(
                                    "assigned_document_missing",
                                    format!(
                                        "failed to resolve assigned document {}: {error}",
                                        path.display()
                                    ),
                                )));
                            }
                        };
                        let bytes = match fs::read(&resolved) {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                return Some(Err(assigned_context_error(
                                    "assigned_document_invalid",
                                    format!(
                                        "failed to read assigned document {}: {error}",
                                        resolved.display()
                                    ),
                                )));
                            }
                        };
                        if format!("{:x}", Sha256::digest(bytes)) != content_hash {
                            return Some(Err(assigned_context_error(
                                "assigned_document_hash_mismatch",
                                format!(
                                    "assigned document content changed: {}",
                                    resolved.display()
                                ),
                            )));
                        }
                        Some((|| {
                            Ok(AssignedDocument {
                                document: MarkdownDocument {
                                    id: parse_id(&id)?,
                                    owner,
                                    repository_id: parse_id(&repository_id)?,
                                    relative_path,
                                    content_hash,
                                    observed_commit,
                                },
                                kind: parse_wire(&kind)?,
                                path: resolved,
                                revision: u64::try_from(revision)
                                    .map_err(|error| AppError::Domain(error.to_string()))?,
                            })
                        })())
                    },
                )
                .collect()
        })
    }

    pub fn assigned_dependency_ids(
        &self,
        work_item_id: WorkItemId,
    ) -> Result<Vec<WorkItemId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT dependency_work_item_id FROM work_item_dependencies
                 WHERE work_item_id = ?1 ORDER BY dependency_order",
            )?;
            statement
                .query_map([work_item_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| parse_id(&row?))
                .collect()
        })
    }

    pub fn assigned_sessions(
        &self,
        principal: &WorkflowPrincipal,
        owners: &[HierarchyOwner],
    ) -> Result<Vec<AssignedSession>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT session.id, session.provider, association.workspace_id,
                        association.epic_id, association.feature_id,
                        association.work_item_id, association.role,
                        association.associated_until, managed.status,
                        managed.managed_until, live.status, live.observed_at,
                        managed.checkout_id, path.path, checkout.branch,
                        CASE
                          WHEN EXISTS (
                            SELECT 1 FROM native_session_sources source
                            WHERE source.session_id = session.id AND source.missing = 0
                          ) THEN 'validated'
                          WHEN EXISTS (
                            SELECT 1 FROM native_session_sources source
                            WHERE source.session_id = session.id
                          ) THEN 'missing'
                          ELSE 'unknown'
                        END
                 FROM native_session_associations association
                 JOIN native_sessions session ON session.id = association.session_id
                 JOIN managed_sessions managed ON managed.session_id = session.id
                 JOIN checkouts checkout ON checkout.id = managed.checkout_id
                 JOIN checkout_paths path
                   ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                 LEFT JOIN live_observations live ON live.id = (
                   SELECT candidate.id FROM live_observations candidate
                   WHERE candidate.session_id = session.id
                   ORDER BY candidate.observed_at DESC, candidate.id DESC LIMIT 1
                 )
                 WHERE association.workspace_id = ?1
                    OR association.epic_id IN (
                      SELECT id FROM epics WHERE workspace_id = ?1
                    )
                    OR association.feature_id IN (
                      SELECT feature.id FROM features feature
                      JOIN epics epic ON epic.id = feature.epic_id
                      WHERE epic.workspace_id = ?1
                    )
                    OR association.work_item_id IN (
                      SELECT item.id FROM work_items item
                      JOIN features feature ON feature.id = item.feature_id
                      JOIN epics epic ON epic.id = feature.epic_id
                      WHERE epic.workspace_id = ?1
                    )
                 ORDER BY association.associated_until IS NULL DESC,
                          live.observed_at DESC, session.id",
            )?;
            let rows = statement
                .query_map([principal.workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, Option<String>>(14)?,
                        row.get::<_, String>(15)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .filter_map(
                    |(
                        session_id,
                        provider,
                        workspace_id,
                        epic_id,
                        feature_id,
                        work_item_id,
                        role,
                        associated_until,
                        managed_status,
                        managed_until,
                        live_status,
                        last_activity,
                        checkout_id,
                        checkout_path,
                        branch,
                        resumability,
                    )| {
                        let owner = match parse_association_owner(
                            workspace_id,
                            epic_id,
                            feature_id,
                            work_item_id,
                        ) {
                            Ok(owner) => owner,
                            Err(error) => return Some(Err(error)),
                        };
                        if !owners.contains(&owner) {
                            return None;
                        }
                        Some((|| {
                            let role = parse_role(&role)?;
                            Ok(AssignedSession {
                                session_id: parse_id(&session_id)?,
                                provider: parse_tool(&provider)?,
                                role,
                                managed_status,
                                live_status: live_status.as_deref().map(parse_wire).transpose()?,
                                last_activity: last_activity
                                    .as_deref()
                                    .map(parse_time)
                                    .transpose()?,
                                checkout_id: parse_id(&checkout_id)?,
                                checkout_path: PathBuf::from(checkout_path),
                                branch,
                                resumability: parse_wire(&resumability)?,
                                primary: role == ManagedSessionRole::WorkItemExecution
                                    && associated_until.is_none()
                                    && managed_until.is_none(),
                            })
                        })())
                    },
                )
                .collect()
        })
    }

    pub fn checkpoint(
        &mut self,
        workflow_token: &str,
        request: CheckpointWorkItem,
    ) -> Result<WorkItemCheckpointOutcome, AppError> {
        validate_idempotency_key(&request.idempotency_key)?;
        if request.summary.trim().is_empty()
            || request.summary.len() > 16 * 1024
            || request.summary.contains('\0')
        {
            return Err(AppError::PlanningDocumentInvalid(
                "checkpoint summary is invalid".to_owned(),
            ));
        }
        let principal = self.authenticate(workflow_token, request.recorded_at)?;
        if principal.owner != HierarchyOwner::WorkItem(request.work_item_id)
            || !matches!(
                principal.role,
                ManagedSessionRole::WorkItemExecution
                    | ManagedSessionRole::Debugging
                    | ManagedSessionRole::Review
            )
        {
            return Err(AppError::WorkflowOperationUnauthorized);
        }
        let next_action = wire_name(request.next_action)?;
        let status = checkpoint_status(request.next_action);
        let status_name = wire_name(status)?;
        self.store.write(|transaction| {
            if let Some((id, work_item_id, existing_action, summary)) = transaction
                .query_row(
                    "SELECT id, work_item_id, next_action_kind, summary
                     FROM work_item_checkpoints WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
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
                if parse_id::<WorkItemId>(&work_item_id)? != request.work_item_id
                    || existing_action != next_action
                    || summary != request.summary
                {
                    return Err(AppError::IdempotencyConflict);
                }
                return Ok(WorkItemCheckpointOutcome {
                    checkpoint_id: parse_id(&id)?,
                    work_item_id: request.work_item_id,
                    next_action: request.next_action,
                    status,
                });
            }
            let checkpoint_id = WorkItemCheckpointId::generate();
            transaction.execute(
                "INSERT INTO work_item_checkpoints (
                     id, work_item_id, session_id, idempotency_key, next_action_kind,
                     summary, recorded_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    checkpoint_id.to_string(),
                    request.work_item_id.to_string(),
                    principal.session_id.to_string(),
                    request.idempotency_key,
                    next_action,
                    request.summary,
                    timestamp(request.recorded_at),
                ],
            )?;
            transaction.execute(
                "UPDATE work_items SET status = ?2 WHERE id = ?1",
                params![request.work_item_id.to_string(), status_name],
            )?;
            Ok(WorkItemCheckpointOutcome {
                checkpoint_id,
                work_item_id: request.work_item_id,
                next_action: request.next_action,
                status,
            })
        })
    }

    pub fn request_session(
        &mut self,
        workflow_token: &str,
        request: RequestManagedSession,
    ) -> Result<ManagedSessionRequestOutcome, AppError> {
        validate_idempotency_key(&request.idempotency_key)?;
        let principal = self.authenticate(workflow_token, request.requested_at)?;
        authorize_session_target(
            self.store,
            principal.owner,
            request.work_item_id,
            request.repository_id,
        )?;
        if let Some(existing) = read_managed_session_request(self.store, &request)? {
            return Ok(existing);
        }
        let (checkout_id, working_directory, title, readiness_generation) =
            effective_checkout(self.store, request.work_item_id, request.repository_id)?;
        self.store.write(|transaction| {
            if let Some((
                id,
                work_item_id,
                provider,
                status,
                stored_repository_id,
                stored_checkout_id,
            )) = transaction
                .query_row(
                    "SELECT id, work_item_id, provider, status, repository_id, checkout_id
                     FROM managed_session_requests WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                        ))
                    },
                )
                .optional()?
            {
                let stored_repository_id = stored_repository_id
                    .ok_or(AppError::IdempotencyConflict)
                    .and_then(|id| parse_id::<RepositoryId>(&id))?;
                let stored_checkout_id = stored_checkout_id
                    .ok_or(AppError::IdempotencyConflict)
                    .and_then(|id| parse_id::<CheckoutId>(&id))?;
                if parse_id::<WorkItemId>(&work_item_id)? != request.work_item_id
                    || parse_tool(&provider)? != request.tool
                    || stored_repository_id != request.repository_id
                    || stored_checkout_id != checkout_id
                {
                    return Err(AppError::IdempotencyConflict);
                }
                return Ok(ManagedSessionRequestOutcome {
                    request_id: parse_id(&id)?,
                    work_item_id: request.work_item_id,
                    repository_id: stored_repository_id,
                    tool: request.tool,
                    checkout_id: stored_checkout_id,
                    working_directory,
                    title,
                    readiness_generation,
                    status,
                });
            }
            let request_id = ManagedSessionRequestId::generate();
            transaction.execute(
                "INSERT INTO managed_session_requests (
                 id, requesting_session_id, work_item_id, provider,
                     idempotency_key, status, requested_at, checkout_id,
                     readiness_generation, repository_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9)",
                params![
                    request_id.to_string(),
                    principal.session_id.to_string(),
                    request.work_item_id.to_string(),
                    tool_name(request.tool),
                    request.idempotency_key,
                    timestamp(request.requested_at),
                    checkout_id.to_string(),
                    readiness_generation
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    request.repository_id.to_string(),
                ],
            )?;
            Ok(ManagedSessionRequestOutcome {
                request_id,
                work_item_id: request.work_item_id,
                repository_id: request.repository_id,
                tool: request.tool,
                checkout_id,
                working_directory,
                title,
                readiness_generation,
                status: "pending".to_owned(),
            })
        })
    }

    pub fn existing_session_request(
        &self,
        workflow_token: &str,
        request: &RequestManagedSession,
    ) -> Result<Option<ManagedSessionRequestOutcome>, AppError> {
        validate_idempotency_key(&request.idempotency_key)?;
        let principal = self.authenticate(workflow_token, request.requested_at)?;
        authorize_session_target(
            self.store,
            principal.owner,
            request.work_item_id,
            request.repository_id,
        )?;
        read_managed_session_request(self.store, request)
    }

    pub fn record_session_launch(
        &mut self,
        request_id: ManagedSessionRequestId,
        intent_id: workboard_core::LaunchIntentId,
    ) -> Result<(), AppError> {
        let updated = self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE managed_session_requests
                     SET status = 'launched', launch_intent_id = ?2
                     WHERE id = ?1 AND status = 'pending'",
                    params![request_id.to_string(), intent_id.to_string()],
                )
                .map_err(Into::into)
        })?;
        if updated != 1 {
            return Err(AppError::IdempotencyConflict);
        }
        Ok(())
    }

    pub fn record_session_binding(
        &mut self,
        request_id: ManagedSessionRequestId,
    ) -> Result<(), AppError> {
        let updated = self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE managed_session_requests SET status = 'bound'
                     WHERE id = ?1 AND status = 'launched'",
                    [request_id.to_string()],
                )
                .map_err(Into::into)
        })?;
        if updated != 1 {
            return Err(AppError::IdempotencyConflict);
        }
        Ok(())
    }
}

fn authorize_session_target(
    store: &SqliteStore,
    owner: HierarchyOwner,
    work_item_id: WorkItemId,
    repository_id: RepositoryId,
) -> Result<(), AppError> {
    let allowed = store.read(|connection| {
        let allowed = match owner {
            HierarchyOwner::Epic(epic_id) => connection.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM work_items item
                     JOIN features feature ON feature.id = item.feature_id
                     WHERE item.id = ?1 AND feature.epic_id = ?2
                 )",
                params![work_item_id.to_string(), epic_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?,
            HierarchyOwner::Feature(feature_id) => connection.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM work_items WHERE id = ?1 AND feature_id = ?2
                 )",
                params![work_item_id.to_string(), feature_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?,
            HierarchyOwner::WorkItem(owner_work_item_id) => {
                i64::from(owner_work_item_id == work_item_id)
            }
            HierarchyOwner::Workspace(_) => 0,
        };
        let repository_allowed = connection.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM work_item_repositories
                 WHERE work_item_id = ?1 AND repository_id = ?2
             )",
            params![work_item_id.to_string(), repository_id.to_string()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(allowed != 0 && repository_allowed != 0)
    })?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::WorkflowOperationUnauthorized)
    }
}

fn canonical_checkout_root(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(assigned_context_error(
            "assigned_checkout_path_invalid",
            format!(
                "the managed session checkout path is unavailable: {}",
                path.display()
            ),
        ));
    }
    fs::canonicalize(path).map_err(|error| {
        assigned_context_error(
            "assigned_checkout_path_invalid",
            format!("failed to resolve {}: {error}", path.display()),
        )
    })
}

fn repository_instructions(
    root: &Path,
    tool: Tool,
    observed_revision: &str,
) -> Result<Vec<RepositoryInstruction>, AppError> {
    let (name, kind) = match tool {
        Tool::Claude => ("CLAUDE.md", RepositoryInstructionKind::Claude),
        Tool::Codex => ("AGENTS.md", RepositoryInstructionKind::Agents),
    };
    let candidate = root.join(name);
    if !candidate.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
        assigned_context_error(
            "repository_instruction_invalid",
            format!("failed to inspect {}: {error}", candidate.display()),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(assigned_context_error(
            "repository_instruction_invalid",
            format!(
                "repository instruction is not a regular file: {}",
                candidate.display()
            ),
        ));
    }
    let path = fs::canonicalize(&candidate).map_err(|error| {
        assigned_context_error(
            "repository_instruction_invalid",
            format!("failed to resolve {}: {error}", candidate.display()),
        )
    })?;
    if !path.starts_with(root) {
        return Err(assigned_context_error(
            "repository_instruction_escape",
            format!(
                "repository instruction escapes the checkout: {}",
                path.display()
            ),
        ));
    }
    let content = fs::read(&path).map_err(|error| {
        assigned_context_error(
            "repository_instruction_invalid",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    Ok(vec![RepositoryInstruction {
        kind,
        path,
        content_hash: format!("{:x}", Sha256::digest(content)),
        observed_revision: observed_revision.to_owned(),
        required: true,
    }])
}

fn assigned_context_error(code: impl Into<String>, message: impl Into<String>) -> AppError {
    AppError::External {
        code: code.into(),
        message: message.into(),
    }
}

fn read_managed_session_request(
    store: &SqliteStore,
    request: &RequestManagedSession,
) -> Result<Option<ManagedSessionRequestOutcome>, AppError> {
    store.read(|connection| {
        let row = connection
            .query_row(
                "SELECT request.id, request.work_item_id, request.provider, request.status,
                        request.repository_id, checkout.id, path.path, item.title,
                        COALESCE(readiness.reconciliation_generation,
                                 request.readiness_generation)
                 FROM managed_session_requests request
                 JOIN work_items item ON item.id = request.work_item_id
                 JOIN checkouts checkout ON checkout.id = request.checkout_id
                 JOIN checkout_paths path
                   ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                 LEFT JOIN checkout_readiness readiness ON readiness.checkout_id = checkout.id
                 WHERE request.idempotency_key = ?1",
                [request.idempotency_key.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            request_id,
            work_item_id,
            provider,
            status,
            repository_id,
            checkout_id,
            path,
            title,
            readiness_generation,
        )) = row
        else {
            return Ok(None);
        };
        if parse_id::<WorkItemId>(&work_item_id)? != request.work_item_id
            || parse_id::<RepositoryId>(&repository_id)? != request.repository_id
            || parse_tool(&provider)? != request.tool
        {
            return Err(AppError::IdempotencyConflict);
        }
        Ok(Some(ManagedSessionRequestOutcome {
            request_id: parse_id(&request_id)?,
            work_item_id: request.work_item_id,
            repository_id: request.repository_id,
            tool: request.tool,
            checkout_id: parse_id(&checkout_id)?,
            working_directory: PathBuf::from(path),
            title,
            readiness_generation: readiness_generation
                .map(u64::try_from)
                .transpose()
                .map_err(|error| AppError::Domain(error.to_string()))?,
            status,
        }))
    })
}

fn effective_checkout(
    store: &SqliteStore,
    work_item_id: WorkItemId,
    repository_id: RepositoryId,
) -> Result<(CheckoutId, PathBuf, String, Option<u64>), AppError> {
    store.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT checkout.id, path.path, item.title, readiness.reconciliation_generation
             FROM effective_work_item_checkouts effective
             JOIN checkouts checkout
               ON checkout.id = effective.checkout_id AND checkout.availability = 'available'
             JOIN checkout_paths path
               ON path.checkout_id = checkout.id AND path.observed_until IS NULL
             JOIN work_items item ON item.id = effective.work_item_id
             LEFT JOIN checkout_readiness readiness ON readiness.checkout_id = checkout.id
             WHERE effective.work_item_id = ?1 AND effective.repository_id = ?2",
        )?;
        let rows = statement
            .query_map(
                params![work_item_id.to_string(), repository_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let [(checkout_id, path, title, readiness_generation)] = rows.as_slice() else {
            return Err(AppError::ResumeCheckoutRequired);
        };
        Ok((
            parse_id(checkout_id)?,
            PathBuf::from(path),
            title.clone(),
            readiness_generation
                .map(u64::try_from)
                .transpose()
                .map_err(|error| AppError::Domain(error.to_string()))?,
        ))
    })
}

fn workspace_for_owner(
    connection: &rusqlite::Connection,
    owner: HierarchyOwner,
) -> Result<WorkspaceId, AppError> {
    let value = match owner {
        HierarchyOwner::Workspace(id) => return Ok(id),
        HierarchyOwner::Epic(id) => connection.query_row(
            "SELECT workspace_id FROM epics WHERE id = ?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )?,
        HierarchyOwner::Feature(id) => connection.query_row(
            "SELECT epic.workspace_id FROM features feature
             JOIN epics epic ON epic.id = feature.epic_id WHERE feature.id = ?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )?,
        HierarchyOwner::WorkItem(id) => connection.query_row(
            "SELECT epic.workspace_id FROM work_items item
             JOIN features feature ON feature.id = item.feature_id
             JOIN epics epic ON epic.id = feature.epic_id WHERE item.id = ?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )?,
    };
    parse_id(&value)
}

fn checkpoint_status(next_action: NextActionKind) -> WorkItemStatus {
    match next_action {
        NextActionKind::Actionable => WorkItemStatus::InProgress,
        NextActionKind::Blocked => WorkItemStatus::Blocked,
        NextActionKind::Paused => WorkItemStatus::Ready,
        NextActionKind::Review | NextActionKind::Delivery => WorkItemStatus::Review,
    }
}

fn parse_owner(
    epic_id: Option<String>,
    feature_id: Option<String>,
    work_item_id: Option<String>,
) -> Result<HierarchyOwner, AppError> {
    match (epic_id, feature_id, work_item_id) {
        (Some(id), None, None) => Ok(HierarchyOwner::Epic(parse_id(&id)?)),
        (None, Some(id), None) => Ok(HierarchyOwner::Feature(parse_id(&id)?)),
        (None, None, Some(id)) => Ok(HierarchyOwner::WorkItem(parse_id(&id)?)),
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
}

fn parse_association_owner(
    workspace_id: Option<String>,
    epic_id: Option<String>,
    feature_id: Option<String>,
    work_item_id: Option<String>,
) -> Result<HierarchyOwner, AppError> {
    match (workspace_id, epic_id, feature_id, work_item_id) {
        (Some(id), None, None, None) => Ok(HierarchyOwner::Workspace(parse_id(&id)?)),
        (None, epic_id, feature_id, work_item_id) => parse_owner(epic_id, feature_id, work_item_id),
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
}

fn parse_role(value: &str) -> Result<ManagedSessionRole, AppError> {
    match value {
        "workspace_planning" => Ok(ManagedSessionRole::WorkspacePlanning),
        "epic_navigation" => Ok(ManagedSessionRole::EpicNavigation),
        "feature_planning" => Ok(ManagedSessionRole::FeaturePlanning),
        "work_item_execution" => Ok(ManagedSessionRole::WorkItemExecution),
        "debugging" => Ok(ManagedSessionRole::Debugging),
        "review" => Ok(ManagedSessionRole::Review),
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
}

fn parse_time(value: &str) -> Result<OffsetDateTime, AppError> {
    let nanoseconds = value
        .parse::<i128>()
        .map_err(|error| AppError::Domain(error.to_string()))?;
    OffsetDateTime::from_unix_timestamp_nanos(nanoseconds)
        .map_err(|error| AppError::Domain(error.to_string()))
}

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn validate_idempotency_key(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(AppError::EmptyIdempotencyKey)
    } else {
        Ok(())
    }
}

fn wire_name<T: Serialize>(value: T) -> Result<String, AppError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Domain("workflow wire value is invalid".to_owned()))
}

fn parse_wire<T>(value: &str) -> Result<T, AppError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn parse_id<T>(value: &str) -> Result<T, AppError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| AppError::Domain(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::params;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId, DocumentId, EpicId,
        FeatureId, HierarchyOwner, LaunchIntentId, ManagedSessionId, ManagedSessionRole,
        NextActionKind, RepositoryId, RepositoryPathId, Tool, WorkItemId, WorkItemStatus,
        WorkspaceId,
    };

    use super::{
        CheckpointWorkItem, RepositoryInstructionKind, RequestManagedSession,
        WorkflowOperationService, timestamp, token_hash, work_item_bootstrap_prompt,
    };
    use crate::AppError;
    use crate::storage::SqliteStore;
    use crate::workspace::WorkboardApplication;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
        database: PathBuf,
        checkout_path: PathBuf,
        at: OffsetDateTime,
        token: String,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let checkout_path = directory.path().join("checkout");
        std::fs::create_dir(&checkout_path).expect("checkout path");
        std::fs::write(checkout_path.join("AGENTS.md"), "# Codex instructions\n")
            .expect("Codex instructions");
        std::fs::write(checkout_path.join("CLAUDE.md"), "# Claude instructions\n")
            .expect("Claude instructions");
        let planning_path = directory.path().join("planning-store");
        let epic_relative = PathBuf::from("workspaces/demo/epics/launch/EPIC.md");
        let feature_relative =
            PathBuf::from("workspaces/demo/epics/launch/features/availability/FEATURE.md");
        let work_item_relative =
            PathBuf::from("workspaces/demo/epics/launch/features/availability/work-items/api.md");
        let dependency_relative = PathBuf::from(
            "workspaces/demo/epics/launch/features/availability/work-items/foundation.md",
        );
        for (relative, body) in [
            (&epic_relative, "# Launch\n"),
            (&feature_relative, "# Availability\n"),
            (&work_item_relative, "# Availability API\n"),
            (&dependency_relative, "# Foundation\n"),
        ] {
            let path = planning_path.join(relative);
            std::fs::create_dir_all(path.parent().expect("document parent"))
                .expect("document parent");
            std::fs::write(path, body).expect("planning document");
        }
        let database = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&database).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        let dependency_work_item_id = WorkItemId::generate();
        let checkout_id = CheckoutId::generate();
        let checkout_path_id = CheckoutPathId::generate();
        let session_id = ConversationId::generate();
        let intent_id = LaunchIntentId::generate();
        let token = "scoped-workflow-token".to_owned();
        let at = OffsetDateTime::parse(
            "2026-08-28T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("fixture timestamp");
        let now = timestamp(at);
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (
                         id, slug, title, planning_store_repository_id, created_at
                     ) VALUES (?1, 'demo', 'Demo', ?2, ?3)",
                    params![
                        workspace_id.to_string(),
                        planning_repository_id.to_string(),
                        now
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory,
                         default_branch, is_planning_store, created_at
                     ) VALUES (?1, ?2, 'planning-store', 'Planning', 'planning.git', 'main', 1, ?4),
                              (?3, ?2, 'code', 'Code', 'code.git', 'main', 0, ?4)",
                    params![
                        planning_repository_id.to_string(),
                        workspace_id.to_string(),
                        repository_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repository_paths (
                         id, repository_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        RepositoryPathId::generate().to_string(),
                        planning_repository_id.to_string(),
                        planning_path.to_string_lossy(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'launch', 'Launch', ?3)",
                    params![epic_id.to_string(), workspace_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO features (
                         id, epic_id, slug, title, workflow_state, created_at
                     ) VALUES (?1, ?2, 'availability', 'Availability', 'planned', ?3)",
                    params![feature_id.to_string(), epic_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at
                     ) VALUES (?1, ?2, 'launch/availability/api', 'api',
                               'Availability API', 'ready', ?3)",
                    params![work_item_id.to_string(), feature_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at,
                         proposal_order
                     ) VALUES (?1, ?2, 'launch/availability/foundation',
                               'foundation', 'Foundation', 'done', ?3, 0)",
                    params![
                        dependency_work_item_id.to_string(),
                        feature_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "UPDATE work_items SET proposal_order = 1 WHERE id = ?1",
                    [work_item_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![work_item_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![
                        dependency_work_item_id.to_string(),
                        repository_id.to_string()
                    ],
                )?;
                for (document_id, epic, feature, work_item, kind, relative, body) in [
                    (
                        DocumentId::generate().to_string(),
                        Some(epic_id.to_string()),
                        None,
                        None,
                        "epic",
                        epic_relative.as_path(),
                        "# Launch\n",
                    ),
                    (
                        DocumentId::generate().to_string(),
                        None,
                        Some(feature_id.to_string()),
                        None,
                        "feature",
                        feature_relative.as_path(),
                        "# Availability\n",
                    ),
                    (
                        DocumentId::generate().to_string(),
                        None,
                        None,
                        Some(work_item_id.to_string()),
                        "work_item",
                        work_item_relative.as_path(),
                        "# Availability API\n",
                    ),
                    (
                        DocumentId::generate().to_string(),
                        None,
                        None,
                        Some(dependency_work_item_id.to_string()),
                        "work_item",
                        dependency_relative.as_path(),
                        "# Foundation\n",
                    ),
                ] {
                    transaction.execute(
                        "INSERT INTO documents (
                             id, repository_id, epic_id, feature_id, work_item_id,
                             kind, relative_path, content_hash, observed_commit,
                             observed_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                                   'planning-head', ?9)",
                        params![
                            document_id,
                            planning_repository_id.to_string(),
                            epic,
                            feature,
                            work_item,
                            kind,
                            relative.to_string_lossy(),
                            format!("{:x}", Sha256::digest(body.as_bytes())),
                            now,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO work_item_dependencies (
                         work_item_id, dependency_work_item_id, dependency_order
                     ) VALUES (?1, ?2, 0)",
                    params![
                        work_item_id.to_string(),
                        dependency_work_item_id.to_string()
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, head,
                         availability, created_at
                     ) VALUES (?1, ?2, 'feature-checkout', 'feature/availability',
                               'fixture-head', 'available', ?3)",
                    params![checkout_id.to_string(), repository_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        checkout_path_id.to_string(),
                        checkout_id.to_string(),
                        checkout_path.to_string_lossy(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_readiness (
                         checkout_id, schema_version, repository_id, checkout_path_id,
                         purpose, access_mode, owner_kind, owner_id, session_id,
                         session_key, parent_feature_checkout_id, base_revision,
                         source_revision, path, git_worktree_identity, branch, head,
                         availability, isolation_generation, reconciliation_generation,
                         evidence_json, observed_at
                     ) VALUES (?1, 1, ?2, ?3, 'work_item_write', 'write_isolated',
                               'work_item', ?4, NULL, '', NULL, 'fixture-head',
                               'fixture-head', ?5, 'feature-checkout',
                               'feature/availability', 'fixture-head', 'available',
                               1, 1, '[]', ?6)",
                    params![
                        checkout_id.to_string(),
                        repository_id.to_string(),
                        checkout_path_id.to_string(),
                        work_item_id.to_string(),
                        checkout_path.to_string_lossy(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        feature_id.to_string(),
                        repository_id.to_string(),
                        checkout_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES (?1, 'codex', 'thread-one', ?2)",
                    params![session_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_associations (
                         id, session_id, work_item_id, role, associated_from
                     ) VALUES (?1, ?2, ?3, 'work_item_execution', ?4)",
                    params![
                        AssociationIntervalId::generate().to_string(),
                        session_id.to_string(),
                        work_item_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO launch_intents (
                         id, work_item_id, checkout_id, provider, idempotency_key,
                         token_hash, status, created_at, expires_at, role,
                         workflow_token_hash, workflow_token_expires_at
                     ) VALUES (?1, ?2, ?3, 'codex', 'launch-one', 'launch-hash', 'bound',
                               ?4, ?5, 'work_item_execution', ?6, ?7)",
                    params![
                        intent_id.to_string(),
                        work_item_id.to_string(),
                        checkout_id.to_string(),
                        now,
                        timestamp(at + time::Duration::minutes(2)),
                        token_hash(&token),
                        timestamp(at + time::Duration::hours(12)),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO managed_sessions (
                         id, launch_intent_id, session_id, checkout_id, role,
                         status, managed_from
                     ) VALUES (?1, ?2, ?3, ?4, 'work_item_execution', 'bound', ?5)",
                    params![
                        ManagedSessionId::generate().to_string(),
                        intent_id.to_string(),
                        session_id.to_string(),
                        checkout_id.to_string(),
                        now,
                    ],
                )?;
                Ok(())
            })
            .expect("seed workflow operation fixture");
        Fixture {
            _directory: directory,
            store,
            work_item_id,
            repository_id,
            database,
            checkout_path,
            at,
            token,
        }
    }

    #[test]
    fn scoped_workflow_operations_are_authenticated_and_idempotent() {
        let mut fixture = fixture();
        let principal = WorkflowOperationService::new(&mut fixture.store)
            .authenticate(&fixture.token, fixture.at + time::Duration::minutes(3))
            .expect("authenticate workflow principal");
        assert_eq!(
            principal.owner,
            HierarchyOwner::WorkItem(fixture.work_item_id)
        );
        assert_eq!(principal.role, ManagedSessionRole::WorkItemExecution);
        let assigned = WorkflowOperationService::new(&mut fixture.store)
            .assigned_repository(&principal)
            .expect("read assigned repository");
        assert_eq!(assigned.repository_id, fixture.repository_id);
        assert_eq!(assigned.head, "fixture-head");
        assert_eq!(assigned.instructions.len(), 1);
        assert_eq!(
            assigned.instructions[0].kind,
            RepositoryInstructionKind::Agents
        );
        assert_eq!(
            assigned.instructions[0].path,
            std::fs::canonicalize(fixture.checkout_path.join("AGENTS.md"))
                .expect("canonical instruction path")
        );
        assert!(assigned.instructions[0].required);
        let mut application =
            WorkboardApplication::open(&fixture.database).expect("open application");
        let context = application
            .assigned_hierarchy(&fixture.token, fixture.at + time::Duration::minutes(3))
            .expect("read assigned context");
        assert_eq!(context.schema_version, 2);
        assert_eq!(
            context.work_item.as_ref().map(|item| item.id),
            Some(fixture.work_item_id)
        );
        assert_eq!(context.repositories.len(), 1);
        assert_eq!(context.documents.len(), 4);
        assert_eq!(context.dependencies.len(), 1);
        assert_eq!(
            context.dependencies[0].work_item.slug.as_str(),
            "foundation"
        );
        assert_eq!(context.sessions.len(), 1);
        assert_eq!(context.sessions[0].session_id, principal.session_id);
        assert!(context.sessions[0].primary);
        let encoded = serde_json::to_string(&context).expect("serialize assigned context");
        assert!(!encoded.contains("thread-one"));
        assert!(matches!(
            WorkflowOperationService::new(&mut fixture.store)
                .authenticate("wrong", fixture.at + time::Duration::minutes(3)),
            Err(AppError::WorkflowOperationUnauthorized)
        ));

        let checkpoint = CheckpointWorkItem {
            work_item_id: fixture.work_item_id,
            next_action: NextActionKind::Review,
            summary: "Implementation is complete and the suite passes.".to_owned(),
            idempotency_key: "checkpoint-review".to_owned(),
            recorded_at: fixture.at + time::Duration::minutes(4),
        };
        let first = WorkflowOperationService::new(&mut fixture.store)
            .checkpoint(&fixture.token, checkpoint.clone())
            .expect("checkpoint Work item");
        let repeated = WorkflowOperationService::new(&mut fixture.store)
            .checkpoint(&fixture.token, checkpoint)
            .expect("repeat checkpoint");
        assert_eq!(first, repeated);
        assert_eq!(first.status, WorkItemStatus::Review);

        let request = RequestManagedSession {
            work_item_id: fixture.work_item_id,
            repository_id: fixture.repository_id,
            tool: Tool::Claude,
            idempotency_key: "request-review-session".to_owned(),
            requested_at: fixture.at + time::Duration::minutes(5),
        };
        let first = WorkflowOperationService::new(&mut fixture.store)
            .request_session(&fixture.token, request.clone())
            .expect("request session");
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE checkouts SET availability = 'missing' WHERE id = ?1",
                    [first.checkout_id.to_string()],
                )?;
                Ok(())
            })
            .expect("mark requested checkout missing");
        let repeated = WorkflowOperationService::new(&mut fixture.store)
            .request_session(&fixture.token, request.clone())
            .expect("repeat session request with missing checkout");
        assert_eq!(first, repeated);
        assert_eq!(first.status, "pending");

        let other_repository_id = RepositoryId::generate();
        let other_checkout_id = CheckoutId::generate();
        let other_checkout_path = fixture._directory.path().join("other-checkout");
        std::fs::create_dir(&other_checkout_path).expect("other checkout path");
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory,
                         default_branch, is_planning_store, created_at
                     ) SELECT ?1, workspace_id, 'other-code', 'Other Code',
                              'other.git', 'main', 0, ?3
                       FROM repositories WHERE id = ?2",
                    params![
                        other_repository_id.to_string(),
                        fixture.repository_id.to_string(),
                        timestamp(fixture.at),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![
                        fixture.work_item_id.to_string(),
                        other_repository_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, availability, created_at
                     ) VALUES (?1, ?2, 'other-feature-checkout', 'feature/other',
                               'available', ?3)",
                    params![
                        other_checkout_id.to_string(),
                        other_repository_id.to_string(),
                        timestamp(fixture.at),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(),
                        other_checkout_id.to_string(),
                        other_checkout_path.to_string_lossy(),
                        timestamp(fixture.at),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) SELECT feature_id, ?2, ?3, ?4
                       FROM work_items WHERE id = ?1",
                    params![
                        fixture.work_item_id.to_string(),
                        other_repository_id.to_string(),
                        other_checkout_id.to_string(),
                        timestamp(fixture.at),
                    ],
                )?;
                Ok(())
            })
            .expect("seed another repository checkout");
        assert!(matches!(
            WorkflowOperationService::new(&mut fixture.store).request_session(
                &fixture.token,
                RequestManagedSession {
                    repository_id: other_repository_id,
                    ..request
                }
            ),
            Err(AppError::IdempotencyConflict)
        ));
        assert!(matches!(
            WorkflowOperationService::new(&mut fixture.store)
                .authenticate(&fixture.token, fixture.at + time::Duration::hours(13),),
            Err(AppError::WorkflowOperationUnauthorized)
        ));
    }

    #[test]
    fn work_item_bootstrap_requires_typed_hierarchy_and_checkpoints() {
        let work_item_id = WorkItemId::generate();
        let prompt = work_item_bootstrap_prompt(work_item_id);

        assert!(prompt.contains(&work_item_id.to_string()));
        assert!(prompt.contains("assigned hierarchy"));
        assert!(prompt.contains("Workboard checkpoints"));
        assert!(prompt.contains("Do not use repository planning ledgers"));
    }
}
