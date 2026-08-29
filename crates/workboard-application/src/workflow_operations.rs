use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    CheckoutId, ConversationId, HierarchyOwner, ManagedSessionRequestId, ManagedSessionRole,
    NextActionKind, Tool, WorkItemCheckpointId, WorkItemId, WorkItemStatus, WorkspaceId,
    WorkspaceSnapshot,
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
pub struct AssignedHierarchy {
    pub principal: WorkflowPrincipal,
    pub snapshot: WorkspaceSnapshot,
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
    pub tool: Tool,
    pub idempotency_key: String,
    pub requested_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedSessionRequestOutcome {
    pub request_id: ManagedSessionRequestId,
    pub work_item_id: WorkItemId,
    pub tool: Tool,
    pub checkout_id: CheckoutId,
    pub working_directory: PathBuf,
    pub title: String,
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
                    "SELECT intent.epic_id, intent.feature_id, intent.work_item_id,
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
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                intent_epic,
                intent_feature,
                intent_work_item,
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
            let intended = parse_owner(intent_epic, intent_feature, intent_work_item)?;
            let associated =
                parse_owner(associated_epic, associated_feature, associated_work_item)?;
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
        let target_allowed = self.store.read(|connection| {
            let allowed = match principal.owner {
                HierarchyOwner::Epic(epic_id) => connection.query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM work_items item
                         JOIN features feature ON feature.id = item.feature_id
                         WHERE item.id = ?1 AND feature.epic_id = ?2
                     )",
                    params![request.work_item_id.to_string(), epic_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )?,
                HierarchyOwner::Feature(feature_id) => connection.query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM work_items WHERE id = ?1 AND feature_id = ?2
                     )",
                    params![request.work_item_id.to_string(), feature_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )?,
                HierarchyOwner::WorkItem(work_item_id) => {
                    i64::from(work_item_id == request.work_item_id)
                }
            };
            Ok(allowed != 0)
        })?;
        if !target_allowed {
            return Err(AppError::WorkflowOperationUnauthorized);
        }
        let (checkout_id, working_directory, title) =
            effective_checkout(self.store, request.work_item_id)?;
        self.store.write(|transaction| {
            if let Some((id, work_item_id, provider, status)) = transaction
                .query_row(
                    "SELECT id, work_item_id, provider, status
                     FROM managed_session_requests WHERE idempotency_key = ?1",
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
                    || parse_tool(&provider)? != request.tool
                {
                    return Err(AppError::IdempotencyConflict);
                }
                return Ok(ManagedSessionRequestOutcome {
                    request_id: parse_id(&id)?,
                    work_item_id: request.work_item_id,
                    tool: request.tool,
                    checkout_id,
                    working_directory,
                    title,
                    status,
                });
            }
            let request_id = ManagedSessionRequestId::generate();
            transaction.execute(
                "INSERT INTO managed_session_requests (
                     id, requesting_session_id, work_item_id, provider,
                     idempotency_key, status, requested_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                params![
                    request_id.to_string(),
                    principal.session_id.to_string(),
                    request.work_item_id.to_string(),
                    tool_name(request.tool),
                    request.idempotency_key,
                    timestamp(request.requested_at),
                ],
            )?;
            Ok(ManagedSessionRequestOutcome {
                request_id,
                work_item_id: request.work_item_id,
                tool: request.tool,
                checkout_id,
                working_directory,
                title,
                status: "pending".to_owned(),
            })
        })
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

fn effective_checkout(
    store: &SqliteStore,
    work_item_id: WorkItemId,
) -> Result<(CheckoutId, PathBuf, String), AppError> {
    store.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT checkout.id, path.path, item.title
             FROM effective_work_item_checkouts effective
             JOIN checkouts checkout
               ON checkout.id = effective.checkout_id AND checkout.availability = 'available'
             JOIN checkout_paths path
               ON path.checkout_id = checkout.id AND path.observed_until IS NULL
             JOIN work_items item ON item.id = effective.work_item_id
             WHERE effective.work_item_id = ?1",
        )?;
        let rows = statement
            .query_map([work_item_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let [(checkout_id, path, title)] = rows.as_slice() else {
            return Err(AppError::ResumeCheckoutRequired);
        };
        Ok((parse_id(checkout_id)?, PathBuf::from(path), title.clone()))
    })
}

fn workspace_for_owner(
    connection: &rusqlite::Connection,
    owner: HierarchyOwner,
) -> Result<WorkspaceId, AppError> {
    let value = match owner {
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

fn parse_role(value: &str) -> Result<ManagedSessionRole, AppError> {
    match value {
        "epic_navigation" => Ok(ManagedSessionRole::EpicNavigation),
        "feature_planning" => Ok(ManagedSessionRole::FeaturePlanning),
        "work_item_execution" => Ok(ManagedSessionRole::WorkItemExecution),
        "debugging" => Ok(ManagedSessionRole::Debugging),
        "review" => Ok(ManagedSessionRole::Review),
        _ => Err(AppError::WorkflowOperationUnauthorized),
    }
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
    use rusqlite::params;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId, EpicId, FeatureId,
        HierarchyOwner, LaunchIntentId, ManagedSessionId, ManagedSessionRole, NextActionKind,
        RepositoryId, Tool, WorkItemId, WorkItemStatus, WorkspaceId,
    };

    use super::{
        CheckpointWorkItem, RequestManagedSession, WorkflowOperationService, timestamp, token_hash,
        work_item_bootstrap_prompt,
    };
    use crate::AppError;
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        work_item_id: WorkItemId,
        at: OffsetDateTime,
        token: String,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let checkout_path = directory.path().join("checkout");
        std::fs::create_dir(&checkout_path).expect("checkout path");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        let checkout_id = CheckoutId::generate();
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
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![work_item_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, availability, created_at
                     ) VALUES (?1, ?2, 'feature-checkout', 'feature/availability',
                               'available', ?3)",
                    params![checkout_id.to_string(), repository_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(),
                        checkout_id.to_string(),
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
            tool: Tool::Claude,
            idempotency_key: "request-review-session".to_owned(),
            requested_at: fixture.at + time::Duration::minutes(5),
        };
        let first = WorkflowOperationService::new(&mut fixture.store)
            .request_session(&fixture.token, request.clone())
            .expect("request session");
        let repeated = WorkflowOperationService::new(&mut fixture.store)
            .request_session(&fixture.token, request)
            .expect("repeat session request");
        assert_eq!(first, repeated);
        assert_eq!(first.status, "pending");
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
