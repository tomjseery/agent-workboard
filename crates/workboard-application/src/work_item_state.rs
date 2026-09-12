use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    CheckoutId, HierarchyOwner, ManagedSessionRole, WORK_ITEM_STATE_SCHEMA_VERSION,
    WorkItemCheckpointId, WorkItemId, WorkItemState, WorkItemStateOutcome,
    WorkItemStatePublicationStatus, WorkItemStateReconciliation, WorkItemStateUpdate,
    WorkItemStateView, WorkItemStatus, WorkItemTerminalIntent, WorkspaceId,
};

use crate::AppError;
use crate::planning_store::{DocumentFrontMatter, PlanningStore};
use crate::storage::SqliteStore;
use crate::workflow_operations::WorkflowOperationService;

const STATE_HEADING: &str = "## Workboard state";
const MAX_STATE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const MAX_ITEMS: usize = 64;

pub struct WorkItemStateService<'a> {
    store: &'a mut SqliteStore,
}

#[derive(Clone, Copy)]
enum StateActor {
    Managed {
        session_id: workboard_core::ConversationId,
        checkout_id: CheckoutId,
    },
    Human,
}

struct DocumentContext {
    workspace_id: WorkspaceId,
    status: WorkItemStatus,
    state_revision: u64,
    document_revision: u64,
    document_id: workboard_core::DocumentId,
    relative_path: PathBuf,
    content_hash: String,
    store_path: PathBuf,
}

struct StagedUpdate {
    checkpoint_id: WorkItemCheckpointId,
    state: WorkItemState,
    front_matter: DocumentFrontMatter,
    body: String,
    context: DocumentContext,
    actor_checkout_id: Option<CheckoutId>,
}

impl<'a> WorkItemStateService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn read(&self, work_item_id: WorkItemId) -> Result<WorkItemStateView, AppError> {
        read_view(self.store, work_item_id)
    }

    pub fn read_for_workspace(
        &self,
        workspace_id: WorkspaceId,
        work_item_id: WorkItemId,
    ) -> Result<WorkItemStateView, AppError> {
        authorize_workspace(self.store, workspace_id, work_item_id)?;
        read_view(self.store, work_item_id)
    }

    pub fn update_managed(
        &mut self,
        workflow_token: &str,
        request: WorkItemStateUpdate,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        let principal =
            WorkflowOperationService::new(self.store).authenticate(workflow_token, recorded_at)?;
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
        self.update(
            StateActor::Managed {
                session_id: principal.session_id,
                checkout_id: principal.checkout_id,
            },
            request,
            recorded_at,
        )
    }

    pub fn update_human(
        &mut self,
        workspace_id: WorkspaceId,
        request: WorkItemStateUpdate,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        authorize_workspace(self.store, workspace_id, request.work_item_id)?;
        self.update(StateActor::Human, request, recorded_at)
    }

    pub fn reconcile_human(
        &mut self,
        workspace_id: WorkspaceId,
        work_item_id: WorkItemId,
        idempotency_key: &str,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        authorize_workspace(self.store, workspace_id, work_item_id)?;
        self.resume_staged(work_item_id, idempotency_key, recorded_at)
    }

    fn update(
        &mut self,
        actor: StateActor,
        request: WorkItemStateUpdate,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        validate_update(&request)?;
        let request_hash = hash_serialized(&request)?;
        if let Some((item_id, hash, status)) =
            existing_update(self.store, &request.idempotency_key)?
        {
            if item_id != request.work_item_id || hash != request_hash {
                return Err(AppError::IdempotencyConflict);
            }
            return if status == WorkItemStatePublicationStatus::Completed {
                existing_outcome(self.store, &request.idempotency_key)
            } else {
                self.resume_staged(request.work_item_id, &request.idempotency_key, recorded_at)
            };
        }
        let context = document_context(self.store, request.work_item_id)?;
        validate_revisions(&request, &context)?;
        validate_transition(context.status, request.status, request.terminal_intent)?;
        let state = state_from_update(&request);
        let planning_store = PlanningStore::create_or_link(&context.store_path)?;
        if planning_store.document_content_hash(&context.relative_path)? != context.content_hash {
            return Err(AppError::PlanningDocumentConcurrentEdit(
                context.store_path.join(&context.relative_path),
            ));
        }
        let document = planning_store.read_document(&context.relative_path)?;
        let front_matter = DocumentFrontMatter {
            status: Some(request.status),
            ..document.front_matter
        };
        let body = render_state_body(&document.body, &state)?;
        let candidate_hash = PlanningStore::rendered_document_hash(&front_matter, &body)?;
        let checkpoint_id = WorkItemCheckpointId::generate();
        let (actor_kind, session_id, actor_checkout_id) = match actor {
            StateActor::Managed {
                session_id,
                checkout_id,
            } => (
                "managed_session",
                Some(session_id.to_string()),
                Some(checkout_id),
            ),
            StateActor::Human => ("local_human", None, None),
        };
        self.store.write(|transaction| {
            let current = document_context_connection(transaction, request.work_item_id)?;
            validate_revisions(&request, &current)?;
            validate_transition(current.status, request.status, request.terminal_intent)?;
            if current.content_hash != context.content_hash {
                return Err(AppError::WorkItemDocumentRevisionStale {
                    expected: request.expected_document_revision,
                    current: current.document_revision,
                });
            }
            transaction.execute(
                "INSERT INTO work_item_state_updates (
                     id, workspace_id, work_item_id, actor_kind, session_id, checkout_id, idempotency_key,
                     request_hash, expected_revision, expected_document_revision,
                     expected_document_hash, candidate_document_hash, state_json,
                     publication_status, recorded_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'pending', ?14)",
                params![
                    checkpoint_id.to_string(),
                    context.workspace_id.to_string(),
                    request.work_item_id.to_string(),
                    actor_kind,
                    session_id,
                    actor_checkout_id.map(|id| id.to_string()),
                    request.idempotency_key,
                    request_hash,
                    as_i64(request.expected_revision)?,
                    as_i64(request.expected_document_revision)?,
                    context.content_hash,
                    candidate_hash,
                    serde_json::to_string(&state)?,
                    timestamp(recorded_at),
                ],
            )?;
            Ok(())
        })?;
        self.publish_and_finalize(
            StagedUpdate {
                checkpoint_id,
                state,
                front_matter,
                body,
                context,
                actor_checkout_id,
            },
            recorded_at,
        )
    }

    fn resume_staged(
        &mut self,
        work_item_id: WorkItemId,
        idempotency_key: &str,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        let row = self.store.read(|connection| {
            connection.query_row(
                "SELECT id, state_json, expected_document_hash, checkout_id FROM work_item_state_updates
                 WHERE work_item_id = ?1 AND idempotency_key = ?2 AND publication_status <> 'completed'",
                params![work_item_id.to_string(), idempotency_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?)),
            ).optional().map_err(Into::into)
        })?.ok_or(AppError::WorkItemNotFound)?;
        let state: WorkItemState = serde_json::from_str(&row.1)?;
        let mut context = document_context(self.store, work_item_id)?;
        context.content_hash = row.2;
        let document = PlanningStore::create_or_link(&context.store_path)?
            .read_document(&context.relative_path)?;
        let front_matter = DocumentFrontMatter {
            status: Some(state.status),
            ..document.front_matter
        };
        let body = render_state_body(&document.body, &state)?;
        self.publish_and_finalize(
            StagedUpdate {
                checkpoint_id: parse_id(&row.0)?,
                state,
                front_matter,
                body,
                context,
                actor_checkout_id: row.3.as_deref().map(parse_id).transpose()?,
            },
            recorded_at,
        )
    }

    fn publish_and_finalize(
        &mut self,
        staged: StagedUpdate,
        recorded_at: OffsetDateTime,
    ) -> Result<WorkItemStateOutcome, AppError> {
        let planning_store = PlanningStore::create_or_link(&staged.context.store_path)?;
        let published = planning_store.publish_reconciled_update(
            &staged.context.relative_path,
            &staged.context.content_hash,
            &staged.front_matter,
            &staged.body,
            &format!("Update Work-item state revision {}", staged.state.revision),
        );
        let published = match published {
            Ok(value) => value,
            Err(error) => {
                return self.reconciliation_error(staged.checkpoint_id, error.to_string());
            }
        };
        let commit = published
            .observed_commit
            .clone()
            .ok_or_else(|| AppError::PlanningGit {
                message: "Work-item state publication produced no commit".to_owned(),
            })?;
        let finalized = self.store.write(|transaction| {
            let publication_status: String = transaction.query_row(
                "SELECT publication_status FROM work_item_state_updates WHERE id = ?1",
                [staged.checkpoint_id.to_string()], |row| row.get(0))?;
            if publication_status == "completed" { return Ok(()); }
            transaction.execute(
                "UPDATE documents SET content_hash = ?2, observed_commit = ?3, observed_at = ?4 WHERE id = ?1",
                params![staged.context.document_id.to_string(), published.content_hash, commit, timestamp(recorded_at)])?;
            transaction.execute(
                "INSERT INTO document_revisions (document_id, revision, content_hash, observed_commit, observed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![staged.context.document_id.to_string(), as_i64(staged.state.document_revision)?,
                    published.content_hash, commit, timestamp(recorded_at)])?;
            transaction.execute(
                "INSERT INTO work_item_states (work_item_id, schema_version, revision, document_revision,
                     state_json, checkpoint_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(work_item_id) DO UPDATE SET schema_version=excluded.schema_version,
                     revision=excluded.revision, document_revision=excluded.document_revision,
                     state_json=excluded.state_json, checkpoint_id=excluded.checkpoint_id, updated_at=excluded.updated_at",
                params![staged.state.work_item_id.to_string(), i64::from(staged.state.schema_version),
                    as_i64(staged.state.revision)?, as_i64(staged.state.document_revision)?,
                    serde_json::to_string(&staged.state)?, staged.checkpoint_id.to_string(), timestamp(recorded_at)])?;
            transaction.execute("UPDATE work_items SET status = ?2 WHERE id = ?1",
                params![staged.state.work_item_id.to_string(), wire_name(staged.state.status)?])?;
            if staged.state.status == WorkItemStatus::Review {
                transaction.execute(
                    "INSERT INTO work_item_integrations (
                         work_item_id, repository_id, source_checkout_id, source_head, status, updated_at
                     ) SELECT ?1, checkout.repository_id, checkout.id, checkout.head, 'pending', ?2
                     FROM checkouts checkout
                     WHERE checkout.head IS NOT NULL AND (
                         (?3 IS NOT NULL AND checkout.id = ?3)
                         OR (?3 IS NULL AND checkout.id IN (
                             SELECT effective.checkout_id FROM effective_work_item_checkouts effective
                             WHERE effective.work_item_id = ?1
                         ))
                     )
                     ON CONFLICT(work_item_id, repository_id) DO UPDATE SET
                         source_checkout_id=excluded.source_checkout_id, source_head=excluded.source_head,
                         status=CASE WHEN work_item_integrations.source_head=excluded.source_head
                              AND work_item_integrations.status='integrated' THEN 'integrated' ELSE 'pending' END,
                         integration_run_id=CASE WHEN work_item_integrations.source_head=excluded.source_head
                              AND work_item_integrations.status='integrated' THEN work_item_integrations.integration_run_id ELSE NULL END,
                         expected_target_head=CASE WHEN work_item_integrations.source_head=excluded.source_head
                              AND work_item_integrations.status='integrated' THEN work_item_integrations.expected_target_head ELSE NULL END,
                         result_head=CASE WHEN work_item_integrations.source_head=excluded.source_head
                              AND work_item_integrations.status='integrated' THEN work_item_integrations.result_head ELSE NULL END,
                         conflict=NULL, updated_at=excluded.updated_at",
                    params![staged.state.work_item_id.to_string(), timestamp(recorded_at),
                        staged.actor_checkout_id.map(|id| id.to_string())],
                )?;
            }
            transaction.execute(
                "UPDATE work_item_state_updates SET publication_status='completed', published_commit=?2,
                     failure=NULL, completed_at=?3 WHERE id=?1",
                params![staged.checkpoint_id.to_string(), commit, timestamp(recorded_at)])?;
            Ok(())
        });
        if let Err(error) = finalized {
            return self.reconciliation_error(staged.checkpoint_id, error.to_string());
        }
        Ok(WorkItemStateOutcome {
            checkpoint_id: staged.checkpoint_id,
            state: staged.state,
            publication_status: WorkItemStatePublicationStatus::Completed,
            published_commit: Some(commit),
        })
    }

    fn reconciliation_error<T>(
        &mut self,
        id: WorkItemCheckpointId,
        reason: String,
    ) -> Result<T, AppError> {
        let bounded: String = reason.chars().take(4096).collect();
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE work_item_state_updates SET publication_status='reconciliation_required', failure=?2
                 WHERE id=?1 AND publication_status <> 'completed'",
                params![id.to_string(), bounded])?;
            Ok(())
        })?;
        Err(AppError::WorkItemStateReconciliationRequired { reason })
    }
}

fn read_view(store: &SqliteStore, work_item_id: WorkItemId) -> Result<WorkItemStateView, AppError> {
    let context = document_context(store, work_item_id)?;
    store.read(|connection| {
        let state = connection
            .query_row(
                "SELECT state_json FROM work_item_states WHERE work_item_id=?1",
                [work_item_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|json| serde_json::from_str(&json))
            .transpose()?;
        let reconciliation = connection
            .query_row(
                "SELECT id, idempotency_key, expected_document_hash, candidate_document_hash,
                    COALESCE(failure, 'publication interrupted') FROM work_item_state_updates
             WHERE work_item_id=?1 AND publication_status <> 'completed'
             ORDER BY recorded_at DESC, id DESC LIMIT 1",
                [work_item_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .map(|row| -> Result<_, AppError> {
                Ok(WorkItemStateReconciliation {
                    checkpoint_id: parse_id(&row.0)?,
                    idempotency_key: row.1,
                    expected_document_hash: row.2,
                    candidate_document_hash: row.3,
                    reason: row.4,
                })
            })
            .transpose()?;
        Ok(WorkItemStateView {
            state,
            document_revision: context.document_revision,
            reconciliation,
        })
    })
}

pub(crate) fn read_projection_view(
    store: &SqliteStore,
    work_item_id: WorkItemId,
) -> Result<WorkItemStateView, AppError> {
    read_view(store, work_item_id)
}

fn existing_update(
    store: &SqliteStore,
    key: &str,
) -> Result<Option<(WorkItemId, String, WorkItemStatePublicationStatus)>, AppError> {
    store.read(|connection| {
        let row = connection.query_row(
            "SELECT work_item_id, request_hash, publication_status FROM work_item_state_updates WHERE idempotency_key=?1",
            [key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
            .optional()?;
        row.map(|row| Ok((parse_id(&row.0)?, row.1, parse_publication_status(&row.2)?))).transpose()
    })
}

fn existing_outcome(store: &SqliteStore, key: &str) -> Result<WorkItemStateOutcome, AppError> {
    store.read(|connection| {
        let row = connection.query_row(
            "SELECT id, state_json, published_commit FROM work_item_state_updates
             WHERE idempotency_key=?1 AND publication_status='completed'",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        Ok(WorkItemStateOutcome {
            checkpoint_id: parse_id(&row.0)?,
            state: serde_json::from_str(&row.1)?,
            publication_status: WorkItemStatePublicationStatus::Completed,
            published_commit: row.2,
        })
    })
}

fn authorize_workspace(
    store: &SqliteStore,
    workspace_id: WorkspaceId,
    work_item_id: WorkItemId,
) -> Result<(), AppError> {
    if document_context(store, work_item_id)?.workspace_id != workspace_id {
        Err(AppError::WorkflowOperationUnauthorized)
    } else {
        Ok(())
    }
}

fn document_context(
    store: &SqliteStore,
    work_item_id: WorkItemId,
) -> Result<DocumentContext, AppError> {
    store.read(|connection| document_context_connection(connection, work_item_id))
}

fn document_context_connection(
    connection: &Connection,
    work_item_id: WorkItemId,
) -> Result<DocumentContext, AppError> {
    let row = connection.query_row(
        "SELECT workspace.id, item.status, COALESCE(state.revision,0), COALESCE(MAX(revision.revision),1),
                document.id, document.relative_path, document.content_hash, path.path
         FROM work_items item JOIN features feature ON feature.id=item.feature_id
         JOIN epics epic ON epic.id=feature.epic_id JOIN workspaces workspace ON workspace.id=epic.workspace_id
         JOIN documents document ON document.work_item_id=item.id AND document.kind='work_item'
              AND document.repository_id=workspace.planning_store_repository_id
         LEFT JOIN document_revisions revision ON revision.document_id=document.id
         JOIN repository_paths path ON path.repository_id=workspace.planning_store_repository_id AND path.observed_until IS NULL
         LEFT JOIN work_item_states state ON state.work_item_id=item.id WHERE item.id=?1
         GROUP BY workspace.id,item.status,state.revision,document.id,document.relative_path,document.content_hash,path.path",
        [work_item_id.to_string()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?,
            row.get::<_, String>(5)?, row.get::<_, String>(6)?, row.get::<_, String>(7)?)))
        .optional()?.ok_or(AppError::WorkItemNotFound)?;
    Ok(DocumentContext {
        workspace_id: parse_id(&row.0)?,
        status: parse_wire(&row.1)?,
        state_revision: as_u64(row.2)?,
        document_revision: as_u64(row.3)?,
        document_id: parse_id(&row.4)?,
        relative_path: PathBuf::from(row.5),
        content_hash: row.6,
        store_path: PathBuf::from(row.7),
    })
}

fn validate_update(request: &WorkItemStateUpdate) -> Result<(), AppError> {
    if request.schema_version != WORK_ITEM_STATE_SCHEMA_VERSION {
        return invalid("unsupported Work-item state schema version");
    }
    validate_text(&request.current_state)?;
    validate_text(&request.next_action.description)?;
    if request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 512
        || request.idempotency_key.chars().any(char::is_control)
    {
        return Err(AppError::EmptyIdempotencyKey);
    }
    if [
        request.blockers.len(),
        request.decisions.len(),
        request.verification.len(),
        request.review.evidence.len(),
        request.delivery.evidence.len(),
    ]
    .into_iter()
    .any(|len| len > MAX_ITEMS)
    {
        return invalid("Work-item state contains too many entries");
    }
    for blocker in &request.blockers {
        for value in [
            &blocker.description,
            &blocker.owner,
            &blocker.unblock_action,
            &blocker.resume_when,
        ] {
            validate_text(value)?;
        }
    }
    for decision in &request.decisions {
        validate_text(&decision.decision)?;
        validate_text(&decision.rationale)?;
    }
    for check in &request.verification {
        validate_text(&check.check)?;
        if let Some(value) = &check.evidence {
            validate_text(value)?;
        }
    }
    for value in request
        .review
        .evidence
        .iter()
        .chain(request.delivery.evidence.iter())
    {
        validate_text(value)?;
    }
    if request.status == WorkItemStatus::Blocked
        && (request.blockers.is_empty()
            || request.next_action.kind != workboard_core::NextActionKind::Blocked)
    {
        return invalid("blocked status requires a blocker and blocked next action");
    }
    if request.terminal_intent == Some(WorkItemTerminalIntent::Cancel)
        && request.status != WorkItemStatus::Cancelled
    {
        return invalid("cancel terminal intent requires cancelled status");
    }
    if request.terminal_intent == Some(WorkItemTerminalIntent::Complete)
        && request.status != WorkItemStatus::Review
    {
        return invalid("complete terminal intent requires review status until integration");
    }
    if serde_json::to_vec(request)?.len() > MAX_STATE_BYTES {
        return invalid("Work-item state is too large");
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        invalid("Work-item state text is invalid")
    } else {
        Ok(())
    }
}

fn validate_revisions(
    request: &WorkItemStateUpdate,
    context: &DocumentContext,
) -> Result<(), AppError> {
    if request.expected_revision != context.state_revision {
        return Err(AppError::WorkItemStateRevisionStale {
            expected: request.expected_revision,
            current: context.state_revision,
        });
    }
    if request.expected_document_revision != context.document_revision {
        return Err(AppError::WorkItemDocumentRevisionStale {
            expected: request.expected_document_revision,
            current: context.document_revision,
        });
    }
    Ok(())
}

fn validate_transition(
    from: WorkItemStatus,
    to: WorkItemStatus,
    terminal: Option<WorkItemTerminalIntent>,
) -> Result<(), AppError> {
    let valid = from == to
        || match from {
            WorkItemStatus::Backlog => matches!(
                to,
                WorkItemStatus::Ready
                    | WorkItemStatus::InProgress
                    | WorkItemStatus::Blocked
                    | WorkItemStatus::Cancelled
            ),
            WorkItemStatus::Ready => matches!(
                to,
                WorkItemStatus::InProgress
                    | WorkItemStatus::Blocked
                    | WorkItemStatus::Review
                    | WorkItemStatus::Cancelled
            ),
            WorkItemStatus::InProgress => matches!(
                to,
                WorkItemStatus::Ready
                    | WorkItemStatus::Blocked
                    | WorkItemStatus::Review
                    | WorkItemStatus::Cancelled
            ),
            WorkItemStatus::Blocked => matches!(
                to,
                WorkItemStatus::Ready
                    | WorkItemStatus::InProgress
                    | WorkItemStatus::Review
                    | WorkItemStatus::Cancelled
            ),
            WorkItemStatus::Review => matches!(
                to,
                WorkItemStatus::InProgress | WorkItemStatus::Blocked | WorkItemStatus::Cancelled
            ),
            WorkItemStatus::Done | WorkItemStatus::Cancelled => false,
        };
    if !valid
        || to == WorkItemStatus::Done
        || (to == WorkItemStatus::Cancelled && terminal != Some(WorkItemTerminalIntent::Cancel))
    {
        Err(AppError::WorkItemStatusTransitionInvalid {
            from: wire_name(from)?,
            to: wire_name(to)?,
        })
    } else {
        Ok(())
    }
}

fn state_from_update(request: &WorkItemStateUpdate) -> WorkItemState {
    WorkItemState {
        schema_version: request.schema_version,
        work_item_id: request.work_item_id,
        revision: request.expected_revision + 1,
        document_revision: request.expected_document_revision + 1,
        current_state: request.current_state.clone(),
        next_action: request.next_action.clone(),
        blockers: request.blockers.clone(),
        decisions: request.decisions.clone(),
        verification: request.verification.clone(),
        review: request.review.clone(),
        delivery: request.delivery.clone(),
        status: request.status,
        terminal_intent: request.terminal_intent,
    }
}

fn render_state_body(body: &str, state: &WorkItemState) -> Result<String, AppError> {
    let retained = body
        .find(STATE_HEADING)
        .map_or(body, |position| &body[..position])
        .trim_end();
    Ok(format!(
        "{retained}\n\n{STATE_HEADING}\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(state)?
    ))
}

fn invalid<T>(message: &str) -> Result<T, AppError> {
    Err(AppError::PlanningDocumentInvalid(message.to_owned()))
}
fn hash_serialized(value: &impl Serialize) -> Result<String, AppError> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn parse_publication_status(value: &str) -> Result<WorkItemStatePublicationStatus, AppError> {
    match value {
        "pending" => Ok(WorkItemStatePublicationStatus::Pending),
        "reconciliation_required" => Ok(WorkItemStatePublicationStatus::ReconciliationRequired),
        "completed" => Ok(WorkItemStatePublicationStatus::Completed),
        _ => Err(AppError::Domain(format!(
            "unknown Work-item state publication status {value}"
        ))),
    }
}
fn wire_name<T: Serialize>(value: T) -> Result<String, AppError> {
    Ok(serde_json::to_string(&value)?.trim_matches('"').to_owned())
}
fn parse_wire<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, AppError> {
    Ok(serde_json::from_str(&format!("\"{value}\""))?)
}
fn as_i64(value: u64) -> Result<i64, AppError> {
    i64::try_from(value).map_err(|error| AppError::Domain(error.to_string()))
}
fn as_u64(value: i64) -> Result<u64, AppError> {
    u64::try_from(value).map_err(|error| AppError::Domain(error.to_string()))
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
fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use rusqlite::params;
    use sha2::Digest;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        DocumentId, DocumentKind, EpicId, FeatureId, NextActionKind, RepositoryId, Slug,
        WORK_ITEM_STATE_SCHEMA_VERSION, WorkItemDeliveryState, WorkItemDeliveryStatus,
        WorkItemNextAction, WorkItemReviewState, WorkItemReviewStatus, WorkItemStateUpdate,
        WorkItemStatus, WorkspaceId,
    };

    use super::{WorkItemStateService, read_projection_view, timestamp};
    use crate::planning_store::{DocumentFrontMatter, PlanningStore};
    use crate::storage::SqliteStore;

    struct Fixture {
        directory: TempDir,
        store: SqliteStore,
        planning_path: std::path::PathBuf,
        relative_path: std::path::PathBuf,
        workspace_id: WorkspaceId,
        work_item_id: workboard_core::WorkItemId,
        checkout_id: workboard_core::CheckoutId,
        token: String,
        at: OffsetDateTime,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = TempDir::new().expect("temporary directory");
            let planning_path = directory.path().join("planning");
            let planning_store =
                PlanningStore::create_or_link(&planning_path).expect("planning store");
            for arguments in [
                ["config", "user.name", "Workboard Test"],
                ["config", "user.email", "workboard@example.invalid"],
            ] {
                assert!(
                    Command::new("git")
                        .arg("-C")
                        .arg(&planning_path)
                        .args(arguments)
                        .status()
                        .expect("configure Git")
                        .success()
                );
            }
            let workspace_slug = Slug::new("demo").expect("workspace slug");
            let epic_slug = Slug::new("epic").expect("epic slug");
            let feature_slug = Slug::new("feature").expect("feature slug");
            let item_slug = Slug::new("item").expect("item slug");
            let relative_path = PlanningStore::work_item_path(
                &workspace_slug,
                &epic_slug,
                &feature_slug,
                &item_slug,
            );
            let document_id = DocumentId::generate();
            let stored = planning_store
                .publish_new(
                    &relative_path,
                    &DocumentFrontMatter {
                        id: document_id,
                        kind: DocumentKind::WorkItem,
                        key: "epic/feature/item".to_owned(),
                        status: Some(WorkItemStatus::Ready),
                        repositories: vec![Slug::new("code").expect("repository slug")],
                    },
                    "# Item\n\nImplement it.\n",
                    "Add Work item",
                )
                .expect("publish Work item");
            let mut store =
                SqliteStore::open(directory.path().join("workboard.sqlite")).expect("database");
            let workspace_id = WorkspaceId::generate();
            let planning_repository_id = RepositoryId::generate();
            let code_repository_id = RepositoryId::generate();
            let epic_id = EpicId::generate();
            let feature_id = FeatureId::generate();
            let work_item_id = workboard_core::WorkItemId::generate();
            let at_value = OffsetDateTime::now_utc();
            let at = timestamp(at_value);
            let checkout_id = workboard_core::CheckoutId::generate();
            let session_id = workboard_core::ConversationId::generate();
            let launch_id = workboard_core::LaunchIntentId::generate();
            let token = "managed-state-token".to_owned();
            store.write(|transaction| {
                transaction.execute("INSERT INTO workspaces (id,slug,title,planning_store_repository_id,created_at) VALUES (?1,'demo','Demo',?2,?3)",
                    params![workspace_id.to_string(),planning_repository_id.to_string(),at])?;
                transaction.execute("INSERT INTO repositories (id,workspace_id,slug,title,git_common_directory,default_branch,is_planning_store,created_at)
                    VALUES (?1,?2,'planning','Planning','planning.git','main',1,?4),(?3,?2,'code','Code','code.git','main',0,?4)",
                    params![planning_repository_id.to_string(),workspace_id.to_string(),code_repository_id.to_string(),at])?;
                transaction.execute("INSERT INTO repository_paths (id,repository_id,path,observed_from) VALUES (?1,?2,?3,?4)",
                    params![workboard_core::RepositoryPathId::generate().to_string(),planning_repository_id.to_string(),planning_path.to_string_lossy(),at])?;
                transaction.execute("INSERT INTO epics (id,workspace_id,slug,title,created_at) VALUES (?1,?2,'epic','Epic',?3)", params![epic_id.to_string(),workspace_id.to_string(),at])?;
                transaction.execute("INSERT INTO features (id,epic_id,slug,title,workflow_state,created_at) VALUES (?1,?2,'feature','Feature','planned',?3)", params![feature_id.to_string(),epic_id.to_string(),at])?;
                transaction.execute("INSERT INTO work_items (id,feature_id,key,slug,title,status,created_at,proposal_order)
                    VALUES (?1,?2,'epic/feature/item','item','Item','ready',?3,0)", params![work_item_id.to_string(),feature_id.to_string(),at])?;
                transaction.execute("INSERT INTO work_item_repositories (work_item_id,repository_id) VALUES (?1,?2)", params![work_item_id.to_string(),code_repository_id.to_string()])?;
                transaction.execute("INSERT INTO documents (id,repository_id,work_item_id,kind,relative_path,content_hash,observed_commit,observed_at)
                    VALUES (?1,?2,?3,'work_item',?4,?5,?6,?7)", params![document_id.to_string(),planning_repository_id.to_string(),work_item_id.to_string(),
                        relative_path.to_string_lossy(),stored.content_hash,stored.observed_commit,at])?;
                transaction.execute("INSERT INTO document_revisions (document_id,revision,content_hash,observed_commit,observed_at)
                    VALUES (?1,1,?2,?3,?4)", params![document_id.to_string(),stored.content_hash,stored.observed_commit,at])?;
                transaction.execute("INSERT INTO checkouts (id,repository_id,git_worktree_identity,branch,head,availability,created_at)
                    VALUES (?1,?2,'work-item','feature/item','code-head','available',?3)", params![checkout_id.to_string(),code_repository_id.to_string(),at])?;
                transaction.execute("INSERT INTO native_sessions (id,provider,native_id,discovered_at) VALUES (?1,'codex','native-state',?2)", params![session_id.to_string(),at])?;
                transaction.execute("INSERT INTO native_session_associations (id,session_id,work_item_id,role,associated_from)
                    VALUES (?1,?2,?3,'work_item_execution',?4)", params![workboard_core::AssociationIntervalId::generate().to_string(),session_id.to_string(),work_item_id.to_string(),at])?;
                transaction.execute("INSERT INTO launch_intents (id,work_item_id,checkout_id,provider,idempotency_key,token_hash,status,created_at,expires_at,role,workflow_token_hash,workflow_token_expires_at)
                    VALUES (?1,?2,?3,'codex','state-launch','launch-hash','bound',?4,?5,'work_item_execution',?6,?7)", params![launch_id.to_string(),work_item_id.to_string(),checkout_id.to_string(),at,
                        timestamp(at_value+time::Duration::minutes(2)),format!("{:x}",sha2::Sha256::digest(token.as_bytes())),timestamp(at_value+time::Duration::hours(1))])?;
                transaction.execute("INSERT INTO managed_sessions (id,launch_intent_id,session_id,checkout_id,role,status,managed_from)
                    VALUES (?1,?2,?3,?4,'work_item_execution','bound',?5)", params![workboard_core::ManagedSessionId::generate().to_string(),launch_id.to_string(),session_id.to_string(),checkout_id.to_string(),at])?;
                Ok(())
            }).expect("seed fixture");
            Self {
                directory,
                store,
                planning_path,
                relative_path,
                workspace_id,
                work_item_id,
                checkout_id,
                token,
                at: at_value,
            }
        }

        fn request(&self, key: &str) -> WorkItemStateUpdate {
            WorkItemStateUpdate {
                schema_version: WORK_ITEM_STATE_SCHEMA_VERSION,
                work_item_id: self.work_item_id,
                expected_revision: 0,
                expected_document_revision: 1,
                current_state: "Implementation is complete and focused checks pass.".to_owned(),
                next_action: WorkItemNextAction {
                    kind: NextActionKind::Review,
                    description: "Run independent review of commit abc123.".to_owned(),
                },
                blockers: vec![],
                decisions: vec![],
                verification: vec![],
                review: WorkItemReviewState {
                    status: WorkItemReviewStatus::Ready,
                    evidence: vec!["commit abc123".to_owned()],
                },
                delivery: WorkItemDeliveryState {
                    status: WorkItemDeliveryStatus::NotStarted,
                    evidence: vec![],
                },
                status: WorkItemStatus::Review,
                terminal_intent: None,
                idempotency_key: key.to_owned(),
            }
        }
    }

    #[test]
    fn human_update_publishes_exact_state_and_rejects_stale_or_conflicting_replays() {
        let mut fixture = Fixture::new();
        let request = fixture.request("human-update");
        let outcome = WorkItemStateService::new(&mut fixture.store)
            .update_human(
                fixture.workspace_id,
                request.clone(),
                OffsetDateTime::now_utc(),
            )
            .expect("update state");
        assert_eq!(outcome.state.revision, 1);
        assert_eq!(outcome.state.document_revision, 2);
        let repeated = WorkItemStateService::new(&mut fixture.store)
            .update_human(
                fixture.workspace_id,
                request.clone(),
                OffsetDateTime::now_utc(),
            )
            .expect("repeat update");
        assert_eq!(repeated.checkpoint_id, outcome.checkpoint_id);
        let view = WorkItemStateService::new(&mut fixture.store)
            .read_for_workspace(fixture.workspace_id, fixture.work_item_id)
            .expect("read state");
        assert_eq!(view.state, Some(outcome.state));
        let document = PlanningStore::create_or_link(&fixture.planning_path)
            .expect("store")
            .read_document(&fixture.relative_path)
            .expect("document");
        assert!(
            document
                .body
                .contains("Run independent review of commit abc123.")
        );
        let stale_request = fixture.request("stale-update");
        let stale = WorkItemStateService::new(&mut fixture.store)
            .update_human(
                fixture.workspace_id,
                stale_request,
                OffsetDateTime::now_utc(),
            )
            .expect_err("stale update");
        assert_eq!(stale.code(), "work_item_state_revision_stale");
        let mut conflict = request;
        conflict.current_state = "Changed replay".to_owned();
        assert_eq!(
            WorkItemStateService::new(&mut fixture.store)
                .update_human(fixture.workspace_id, conflict, OffsetDateTime::now_utc())
                .expect_err("conflicting replay")
                .code(),
            "idempotency_conflict"
        );
    }

    #[test]
    fn managed_update_authenticates_and_uses_the_same_projection_and_history() {
        let mut fixture = Fixture::new();
        let request = fixture.request("managed-update");
        let outcome = WorkItemStateService::new(&mut fixture.store)
            .update_managed(
                &fixture.token,
                request,
                fixture.at + time::Duration::minutes(3),
            )
            .expect("managed update");
        let view = WorkItemStateService::new(&mut fixture.store)
            .read(fixture.work_item_id)
            .expect("state view");
        assert_eq!(view.state, Some(outcome.state));
        let (actor, recorded_checkout): (String, String) = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT actor_kind, checkout_id FROM work_item_state_updates WHERE id=?1",
                        [outcome.checkpoint_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("history actor");
        assert_eq!(actor, "managed_session");
        assert_eq!(recorded_checkout, fixture.checkout_id.to_string());
        let integrated_checkout: String = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT source_checkout_id FROM work_item_integrations WHERE work_item_id=?1",
                        [fixture.work_item_id.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("managed integration source");
        assert_eq!(integrated_checkout, fixture.checkout_id.to_string());
        let unauthorized = fixture.request("wrong-token");
        assert_eq!(
            WorkItemStateService::new(&mut fixture.store)
                .update_managed(
                    "wrong",
                    unauthorized,
                    fixture.at + time::Duration::minutes(3),
                )
                .expect_err("wrong token")
                .code(),
            "workflow_operation_unauthorized"
        );
    }

    #[test]
    fn interrupted_git_publication_is_visible_after_restart_and_reconciles() {
        let mut fixture = Fixture::new();
        let hook = fixture.planning_path.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").expect("failing hook");
        let request = fixture.request("interrupted-update");
        let error = WorkItemStateService::new(&mut fixture.store)
            .update_human(fixture.workspace_id, request, OffsetDateTime::now_utc())
            .expect_err("publication failure");
        assert_eq!(error.code(), "work_item_state_reconciliation_required");
        drop(fixture.store);
        let mut reopened = SqliteStore::open(fixture.directory.path().join("workboard.sqlite"))
            .expect("reopen database");
        let view = WorkItemStateService::new(&mut reopened)
            .read(fixture.work_item_id)
            .expect("read reconciliation");
        assert!(view.state.is_none());
        assert_eq!(
            view.reconciliation
                .as_ref()
                .map(|value| value.idempotency_key.as_str()),
            Some("interrupted-update")
        );
        fs::remove_file(hook).expect("remove hook");
        let outcome = WorkItemStateService::new(&mut reopened)
            .reconcile_human(
                fixture.workspace_id,
                fixture.work_item_id,
                "interrupted-update",
                OffsetDateTime::now_utc(),
            )
            .expect("reconcile");
        assert_eq!(outcome.state.revision, 1);
        assert!(
            WorkItemStateService::new(&mut reopened)
                .read(fixture.work_item_id)
                .expect("read state")
                .reconciliation
                .is_none()
        );
    }

    #[test]
    fn database_finalization_failure_never_reports_success_and_reconciles_after_restart() {
        let mut fixture = Fixture::new();
        fixture
            .store
            .write(|transaction| {
                transaction.execute_batch(
                    "CREATE TRIGGER fail_state_document_revision
                 BEFORE INSERT ON document_revisions WHEN NEW.revision = 2
                 BEGIN SELECT RAISE(ABORT, 'injected finalization failure'); END;",
                )?;
                Ok(())
            })
            .expect("install failure trigger");
        let request = fixture.request("database-interruption");
        let error = WorkItemStateService::new(&mut fixture.store)
            .update_human(fixture.workspace_id, request, OffsetDateTime::now_utc())
            .expect_err("database finalization failure");
        assert_eq!(error.code(), "work_item_state_reconciliation_required");
        fixture
            .store
            .write(|transaction| {
                transaction.execute_batch("DROP TRIGGER fail_state_document_revision")?;
                Ok(())
            })
            .expect("remove failure trigger");
        drop(fixture.store);
        let mut reopened = SqliteStore::open(fixture.directory.path().join("workboard.sqlite"))
            .expect("reopen database");
        let before = WorkItemStateService::new(&mut reopened)
            .read(fixture.work_item_id)
            .expect("read interrupted state");
        assert!(before.state.is_none());
        assert!(before.reconciliation.is_some());
        let outcome = WorkItemStateService::new(&mut reopened)
            .reconcile_human(
                fixture.workspace_id,
                fixture.work_item_id,
                "database-interruption",
                OffsetDateTime::now_utc(),
            )
            .expect("reconcile database interruption");
        assert_eq!(outcome.state.document_revision, 2);
    }

    #[test]
    fn changed_document_cross_workspace_and_invalid_terminal_status_fail_closed() {
        let mut fixture = Fixture::new();
        let foreign_request = fixture.request("foreign");
        assert_eq!(
            WorkItemStateService::new(&mut fixture.store)
                .update_human(
                    WorkspaceId::generate(),
                    foreign_request,
                    OffsetDateTime::now_utc()
                )
                .expect_err("foreign workspace")
                .code(),
            "workflow_operation_unauthorized"
        );
        let mut terminal = fixture.request("done");
        terminal.status = WorkItemStatus::Done;
        assert_eq!(
            WorkItemStateService::new(&mut fixture.store)
                .update_human(fixture.workspace_id, terminal, OffsetDateTime::now_utc())
                .expect_err("done without integration")
                .code(),
            "work_item_status_transition_invalid"
        );
        fs::write(
            fixture.planning_path.join(&fixture.relative_path),
            "external edit",
        )
        .expect("external edit");
        let external_request = fixture.request("external");
        let error = WorkItemStateService::new(&mut fixture.store)
            .update_human(
                fixture.workspace_id,
                external_request,
                OffsetDateTime::now_utc(),
            )
            .expect_err("changed document");
        assert_eq!(error.code(), "planning_document_concurrent_edit");
    }

    #[test]
    fn authoritative_read_rejects_a_missing_planning_store_path() {
        let mut fixture = Fixture::new();
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE repository_paths SET observed_until=?1 WHERE observed_until IS NULL",
                    [timestamp(OffsetDateTime::now_utc())],
                )?;
                Ok(())
            })
            .expect("retire planning-store path");

        assert_eq!(
            read_projection_view(&fixture.store, fixture.work_item_id)
                .expect_err("missing authority")
                .code(),
            "work_item_not_found"
        );
    }
}
