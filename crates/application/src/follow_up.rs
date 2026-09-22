use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_core::{
    CheckoutId, ConversationId, FeatureId, HierarchyOwner, SessionFollowUpId, Tool,
    WORKBOARD_WORKFLOW_TOKEN_ENV, WorkItemId, WorkspaceId,
};

use crate::AppError;
use crate::storage::SqliteStore;
use crate::workflow_operations::WorkflowOperationService;

const MAX_FOLLOW_UP_BYTES: usize = 64 * 1024;
const LEASE_DURATION: time::Duration = time::Duration::minutes(2);
const CREDENTIAL_DURATION: time::Duration = time::Duration::days(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendSessionFollowUp {
    pub owner: HierarchyOwner,
    pub session_id: Option<ConversationId>,
    pub expected_binding_generation: u32,
    pub text: String,
    pub idempotency_key: String,
    pub requested_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFollowUpStatus {
    Pending,
    Leased,
    Delivered,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFollowUpOutcome {
    pub follow_up_id: SessionFollowUpId,
    pub owner: HierarchyOwner,
    pub session_id: ConversationId,
    pub binding_generation: u32,
    pub checkout_id: CheckoutId,
    pub checkout_generation: u64,
    pub sequence: u64,
    pub status: SessionFollowUpStatus,
    pub attempt_count: u32,
    pub receipt: Option<String>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFollowUp {
    pub follow_up_id: SessionFollowUpId,
    pub tool: Tool,
    pub native_id: String,
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub capability_bundle_root: PathBuf,
    pub workflow_token: String,
    pub text: String,
    pub client_message_id: String,
    pub active_turn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUpDeliveryFailureKind {
    Deferred,
    Rejected,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpDeliveryFailure {
    pub kind: FollowUpDeliveryFailureKind,
    pub message: String,
}

pub trait FollowUpExecutor {
    fn reconcile(
        &self,
        request: &ProviderFollowUp,
    ) -> Result<Option<String>, FollowUpDeliveryFailure>;

    fn deliver(&self, request: &ProviderFollowUp) -> Result<String, FollowUpDeliveryFailure>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemFollowUpExecutor;

impl FollowUpExecutor for SystemFollowUpExecutor {
    fn reconcile(
        &self,
        request: &ProviderFollowUp,
    ) -> Result<Option<String>, FollowUpDeliveryFailure> {
        match request.tool {
            Tool::Claude => workboard_adapter_claude::ClaudeFollowUpClient::default()
                .reconcile(&claude_request(request))
                .map_err(map_claude_failure),
            Tool::Codex => workboard_adapter_codex::CodexAppServerClient::new(&request.executable)
                .reconcile_follow_up(&codex_request(request))
                .map(|receipt| receipt.map(codex_receipt))
                .map_err(map_codex_failure),
        }
    }

    fn deliver(&self, request: &ProviderFollowUp) -> Result<String, FollowUpDeliveryFailure> {
        match request.tool {
            Tool::Claude => workboard_adapter_claude::ClaudeFollowUpClient::default()
                .deliver(&claude_request(request))
                .map_err(map_claude_failure),
            Tool::Codex => workboard_adapter_codex::CodexAppServerClient::new(&request.executable)
                .send_follow_up(&codex_request(request))
                .map(codex_receipt)
                .map_err(map_codex_failure),
        }
    }
}

pub struct FollowUpService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> FollowUpService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn queue_authenticated(
        &mut self,
        workflow_token: &str,
        request: SendSessionFollowUp,
    ) -> Result<SessionFollowUpOutcome, AppError> {
        let principal = WorkflowOperationService::new(self.store)
            .authenticate(workflow_token, request.requested_at)?;
        if principal.owner != request.owner {
            return Err(AppError::WorkflowOperationUnauthorized);
        }
        self.queue(principal.workspace_id, request)
    }

    pub fn queue_for_board(
        &mut self,
        workspace_id: WorkspaceId,
        request: SendSessionFollowUp,
    ) -> Result<SessionFollowUpOutcome, AppError> {
        let actual_workspace = owner_workspace(self.store, request.owner)?;
        if actual_workspace != workspace_id {
            return Err(AppError::WorkflowOperationUnauthorized);
        }
        self.queue(workspace_id, request)
    }

    pub fn deliver_next(
        &mut self,
        session_id: Option<ConversationId>,
        now: OffsetDateTime,
        executor: &impl FollowUpExecutor,
    ) -> Result<Option<SessionFollowUpOutcome>, AppError> {
        let leased = self.lease_next(session_id, now)?;
        let Some(leased) = leased else {
            return Ok(None);
        };
        let LeasedFollowUp {
            request,
            attempt_count,
            previous_failure,
            lease_token,
        } = leased;
        let delivery = if attempt_count > 1 {
            match executor.reconcile(&request) {
                Ok(Some(receipt)) => Ok(receipt),
                Ok(None)
                    if previous_failure
                        .as_deref()
                        .is_some_and(|failure| failure.starts_with("uncertain:")) =>
                {
                    Err(FollowUpDeliveryFailure {
                        kind: FollowUpDeliveryFailureKind::Rejected,
                        message: "the earlier provider delivery could not be reconciled safely"
                            .to_owned(),
                    })
                }
                Ok(None) => executor.deliver(&request),
                Err(failure) => Err(failure),
            }
        } else {
            executor.deliver(&request)
        };
        let outcome = self.finish_delivery(request.follow_up_id, &lease_token, delivery, now)?;
        Ok(Some(outcome))
    }

    fn queue(
        &mut self,
        workspace_id: WorkspaceId,
        request: SendSessionFollowUp,
    ) -> Result<SessionFollowUpOutcome, AppError> {
        validate_request(&request)?;
        let instruction_hash = digest(&request.text);
        if let Some(existing) = read_by_idempotency(self.store, &request.idempotency_key)? {
            let same = self.store.read(|connection| {
                connection
                    .query_row(
                        "SELECT feature_id, work_item_id, session_id, binding_generation,
                            instruction_hash
                     FROM session_follow_ups WHERE id = ?1",
                        [existing.follow_up_id.to_string()],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, u32>(3)?,
                                row.get::<_, String>(4)?,
                            ))
                        },
                    )
                    .map_err(Into::into)
            })?;
            let owner = parse_owner(same.0, same.1)?;
            if owner != request.owner
                || request
                    .session_id
                    .is_some_and(|id| id.to_string() != same.2)
                || same.3 != request.expected_binding_generation
                || same.4 != instruction_hash
            {
                return Err(AppError::IdempotencyConflict);
            }
            return Ok(existing);
        }
        let candidates = resolve_candidates(self.store, request.owner, request.session_id)?;
        let [target] = candidates.as_slice() else {
            return Err(follow_up_error(
                if candidates.is_empty() {
                    "follow_up_session_unavailable"
                } else {
                    "follow_up_session_selection_required"
                },
                if candidates.is_empty() {
                    "no current bound session can receive this follow-up"
                } else {
                    "more than one current bound session can receive this follow-up"
                },
            ));
        };
        if target.binding_generation != request.expected_binding_generation {
            return Err(follow_up_error(
                "follow_up_binding_generation_mismatch",
                "the selected Workboard session has been rebound",
            ));
        }
        let follow_up_id = SessionFollowUpId::generate();
        let owner = request.owner;
        let (feature_id, work_item_id) = owner_columns(owner)?;
        self.store.write(|transaction| {
            let sequence: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM session_follow_ups
                 WHERE session_id = ?1 AND binding_generation = ?2",
                params![target.session_id.to_string(), target.binding_generation],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO session_follow_ups (
                     id, workspace_id, feature_id, work_item_id, session_id, association_id,
                     managed_session_id, binding_generation, checkout_id, checkout_generation,
                     sequence, idempotency_key, instruction, instruction_hash, status, created_at
                 ) VALUES (
                     ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                     'pending', ?15
                 )",
                params![
                    follow_up_id.to_string(),
                    workspace_id.to_string(),
                    feature_id,
                    work_item_id,
                    target.session_id.to_string(),
                    target.association_id,
                    target.managed_session_id,
                    target.binding_generation,
                    target.checkout_id.to_string(),
                    i64::try_from(target.checkout_generation)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    sequence,
                    request.idempotency_key,
                    request.text,
                    instruction_hash,
                    timestamp(request.requested_at),
                ],
            )?;
            read_outcome(transaction, follow_up_id)
        })
    }

    fn lease_next(
        &mut self,
        session_id: Option<ConversationId>,
        now: OffsetDateTime,
    ) -> Result<Option<LeasedFollowUp>, AppError> {
        self.store.write(|transaction| {
            let row = transaction
                .query_row(
                    "SELECT follow_up.id, follow_up.session_id, follow_up.managed_session_id,
                            follow_up.binding_generation, follow_up.checkout_id,
                            follow_up.checkout_generation, follow_up.instruction,
                            follow_up.idempotency_key, follow_up.attempt_count,
                            follow_up.failure, session.provider, session.native_id,
                            path.path, intent.capability_bundle_root, live.status, live.executable,
                            association.id, managed.binding_generation,
                            readiness.reconciliation_generation,
                            CASE WHEN association.associated_until IS NULL
                                   AND managed.managed_until IS NULL
                                   AND managed.status IN ('bound', 'adopted')
                                   AND managed.session_id = follow_up.session_id
                                   AND managed.checkout_id = follow_up.checkout_id
                                   AND association.session_id = follow_up.session_id
                                   AND association.feature_id IS follow_up.feature_id
                                   AND association.work_item_id IS follow_up.work_item_id
                                 THEN 1 ELSE 0 END
                     FROM session_follow_ups follow_up
                     JOIN native_sessions session ON session.id = follow_up.session_id
                     JOIN managed_sessions managed ON managed.id = follow_up.managed_session_id
                     JOIN native_session_associations association
                       ON association.id = follow_up.association_id
                     JOIN checkout_paths path
                       ON path.checkout_id = follow_up.checkout_id
                      AND path.observed_until IS NULL
                     JOIN checkout_readiness readiness
                       ON readiness.checkout_id = follow_up.checkout_id
                     JOIN launch_intents intent ON intent.id = managed.launch_intent_id
                     LEFT JOIN live_observations live ON live.id = (
                         SELECT candidate.id FROM live_observations candidate
                         WHERE candidate.session_id = follow_up.session_id
                         ORDER BY candidate.observed_at DESC, candidate.id DESC LIMIT 1
                     )
                     WHERE (?1 IS NULL OR follow_up.session_id = ?1)
                       AND (
                           follow_up.status = 'pending'
                           OR (follow_up.status = 'leased' AND follow_up.leased_until <= ?2)
                       )
                     ORDER BY follow_up.created_at, follow_up.session_id, follow_up.sequence
                     LIMIT 1",
                    params![session_id.map(|id| id.to_string()), timestamp(now)],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, u32>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, u32>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, String>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, Option<String>>(13)?,
                            row.get::<_, Option<String>>(14)?,
                            row.get::<_, Option<String>>(15)?,
                            row.get::<_, String>(16)?,
                            row.get::<_, u32>(17)?,
                            row.get::<_, i64>(18)?,
                            row.get::<_, i64>(19)?,
                        ))
                    },
                )
                .optional()?;
            let Some(row) = row else {
                return Ok(None);
            };
            let follow_up_id = parse_id(&row.0)?;
            if row.19 != 1
                || row.16.is_empty()
                || row.17 != row.3
                || row.18 != row.5
                || row.13.is_none()
                || row.15.is_none()
                || !matches!(row.14.as_deref(), Some("active" | "idle"))
            {
                transaction.execute(
                    "UPDATE session_follow_ups
                     SET status = 'failed', failure = 'binding or provider evidence changed',
                         lease_token = NULL, leased_until = NULL
                     WHERE id = ?1",
                    [row.0],
                )?;
                return Ok(None);
            }
            let lease_token = Uuid::new_v4().to_string();
            let attempt_count = row
                .8
                .checked_add(1)
                .ok_or_else(|| AppError::Domain("follow-up attempt count overflowed".to_owned()))?;
            let updated = transaction.execute(
                "UPDATE session_follow_ups
                 SET status = 'leased', lease_token = ?2, leased_until = ?3,
                     attempt_count = ?4
                 WHERE id = ?1 AND (
                     status = 'pending' OR (status = 'leased' AND leased_until <= ?5)
                 )",
                params![
                    row.0,
                    lease_token,
                    timestamp(now + LEASE_DURATION),
                    attempt_count,
                    timestamp(now),
                ],
            )?;
            if updated != 1 {
                return Ok(None);
            }
            let workflow_token = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO workflow_credentials (
                     id, managed_session_id, binding_generation, token_hash, created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    row.2,
                    row.3,
                    digest(&workflow_token),
                    timestamp(now),
                    timestamp(now + CREDENTIAL_DURATION),
                ],
            )?;
            Ok(Some(LeasedFollowUp {
                request: ProviderFollowUp {
                    follow_up_id,
                    tool: parse_wire(&row.10)?,
                    native_id: row.11,
                    executable: PathBuf::from(row.15.expect("checked executable")),
                    working_directory: PathBuf::from(row.12),
                    capability_bundle_root: PathBuf::from(
                        row.13.expect("checked capability bundle"),
                    ),
                    workflow_token,
                    text: row.6,
                    client_message_id: row.7,
                    active_turn: row.14.as_deref() == Some("active"),
                },
                attempt_count,
                previous_failure: row.9,
                lease_token,
            }))
        })
    }

    fn finish_delivery(
        &mut self,
        follow_up_id: SessionFollowUpId,
        lease_token: &str,
        delivery: Result<String, FollowUpDeliveryFailure>,
        now: OffsetDateTime,
    ) -> Result<SessionFollowUpOutcome, AppError> {
        self.store.write(|transaction| {
            match delivery {
                Ok(receipt) => {
                    if receipt.trim().is_empty() || receipt.len() > 64 * 1024 {
                        return Err(follow_up_error(
                            "follow_up_receipt_invalid",
                            "the provider returned an invalid follow-up receipt",
                        ));
                    }
                    transaction.execute(
                        "UPDATE session_follow_ups
                         SET status = 'delivered', receipt = ?3, delivered_at = ?4,
                             failure = NULL, lease_token = NULL, leased_until = NULL
                         WHERE id = ?1 AND status = 'leased' AND lease_token = ?2",
                        params![
                            follow_up_id.to_string(),
                            lease_token,
                            receipt,
                            timestamp(now),
                        ],
                    )?;
                }
                Err(failure) => {
                    let (status, prefix) = match failure.kind {
                        FollowUpDeliveryFailureKind::Deferred => ("pending", "deferred"),
                        FollowUpDeliveryFailureKind::Rejected => ("failed", "rejected"),
                        FollowUpDeliveryFailureKind::Uncertain => ("pending", "uncertain"),
                    };
                    transaction.execute(
                        "UPDATE session_follow_ups
                         SET status = ?3, failure = ?4, lease_token = NULL, leased_until = NULL
                         WHERE id = ?1 AND status = 'leased' AND lease_token = ?2",
                        params![
                            follow_up_id.to_string(),
                            lease_token,
                            status,
                            format!("{prefix}: {}", failure.message),
                        ],
                    )?;
                }
            }
            read_outcome(transaction, follow_up_id)
        })
    }
}

#[derive(Debug)]
struct FollowUpTarget {
    association_id: String,
    managed_session_id: String,
    session_id: ConversationId,
    binding_generation: u32,
    checkout_id: CheckoutId,
    checkout_generation: u64,
}

struct LeasedFollowUp {
    request: ProviderFollowUp,
    attempt_count: u32,
    previous_failure: Option<String>,
    lease_token: String,
}

fn resolve_candidates(
    store: &SqliteStore,
    owner: HierarchyOwner,
    session_id: Option<ConversationId>,
) -> Result<Vec<FollowUpTarget>, AppError> {
    let (feature_id, work_item_id) = owner_columns(owner)?;
    store.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT association.id, managed.id, session.id, managed.binding_generation,
                    managed.checkout_id, readiness.reconciliation_generation, association.role
             FROM native_session_associations association
             JOIN native_sessions session ON session.id = association.session_id
             JOIN managed_sessions managed ON managed.session_id = session.id
             JOIN checkout_readiness readiness ON readiness.checkout_id = managed.checkout_id
             WHERE association.associated_until IS NULL AND managed.managed_until IS NULL
               AND managed.status IN ('bound', 'adopted')
               AND association.feature_id IS ?1 AND association.work_item_id IS ?2
               AND (?3 IS NULL OR session.id = ?3)
             ORDER BY session.id",
        )?;
        statement
            .query_map(
                params![
                    feature_id,
                    work_item_id,
                    session_id.map(|id| id.to_string())
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )?
            .map(|row| {
                let row = row?;
                if !role_can_receive(owner, &row.6) {
                    return Ok(None);
                }
                Ok(Some(FollowUpTarget {
                    association_id: row.0,
                    managed_session_id: row.1,
                    session_id: parse_id(&row.2)?,
                    binding_generation: row.3,
                    checkout_id: parse_id(&row.4)?,
                    checkout_generation: u64::try_from(row.5)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                }))
            })
            .filter_map(Result::transpose)
            .collect()
    })
}

fn read_by_idempotency(
    store: &SqliteStore,
    idempotency_key: &str,
) -> Result<Option<SessionFollowUpOutcome>, AppError> {
    store.read(|connection| {
        connection
            .query_row(
                "SELECT id FROM session_follow_ups WHERE idempotency_key = ?1",
                [idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|id| read_outcome(connection, parse_id(&id)?))
            .transpose()
    })
}

fn read_outcome(
    connection: &rusqlite::Connection,
    follow_up_id: SessionFollowUpId,
) -> Result<SessionFollowUpOutcome, AppError> {
    connection
        .query_row(
            "SELECT feature_id, work_item_id, session_id, binding_generation, checkout_id,
                    checkout_generation, sequence, status, attempt_count, receipt, failure
             FROM session_follow_ups WHERE id = ?1",
            [follow_up_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .map_err(Into::into)
        .and_then(|row| {
            Ok(SessionFollowUpOutcome {
                follow_up_id,
                owner: parse_owner(row.0, row.1)?,
                session_id: parse_id(&row.2)?,
                binding_generation: row.3,
                checkout_id: parse_id(&row.4)?,
                checkout_generation: u64::try_from(row.5)
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                sequence: u64::try_from(row.6)
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                status: parse_wire(&row.7)?,
                attempt_count: row.8,
                receipt: row.9,
                failure: row.10,
            })
        })
}

fn owner_workspace(store: &SqliteStore, owner: HierarchyOwner) -> Result<WorkspaceId, AppError> {
    store.read(|connection| match owner {
        HierarchyOwner::Feature(id) => connection
            .query_row(
                "SELECT epic.workspace_id FROM features feature
                 JOIN epics epic ON epic.id = feature.epic_id WHERE feature.id = ?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
            .and_then(|id| parse_id(&id)),
        HierarchyOwner::WorkItem(id) => connection
            .query_row(
                "SELECT epic.workspace_id FROM work_items item
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id WHERE item.id = ?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
            .and_then(|id| parse_id(&id)),
        HierarchyOwner::Workspace(_) | HierarchyOwner::Epic(_) => {
            Err(AppError::WorkflowOperationUnauthorized)
        }
    })
}

fn validate_request(request: &SendSessionFollowUp) -> Result<(), AppError> {
    if request.expected_binding_generation == 0
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 512
        || request.idempotency_key.contains('\0')
        || request.text.trim().is_empty()
        || request.text.len() > MAX_FOLLOW_UP_BYTES
        || request.text.contains('\0')
    {
        return Err(follow_up_error(
            "follow_up_request_invalid",
            "the follow-up request is invalid",
        ));
    }
    owner_columns(request.owner).map(|_| ())
}

fn owner_columns(owner: HierarchyOwner) -> Result<(Option<String>, Option<String>), AppError> {
    match owner {
        HierarchyOwner::Feature(id) => Ok((Some(id.to_string()), None)),
        HierarchyOwner::WorkItem(id) => Ok((None, Some(id.to_string()))),
        HierarchyOwner::Workspace(_) | HierarchyOwner::Epic(_) => {
            Err(AppError::WorkflowOperationUnauthorized)
        }
    }
}

fn role_can_receive(owner: HierarchyOwner, role: &str) -> bool {
    match owner {
        HierarchyOwner::Feature(_) => role == "feature_planning",
        HierarchyOwner::WorkItem(_) => {
            matches!(role, "work_item_execution" | "debugging" | "review")
        }
        HierarchyOwner::Workspace(_) | HierarchyOwner::Epic(_) => false,
    }
}

fn parse_owner(
    feature_id: Option<String>,
    work_item_id: Option<String>,
) -> Result<HierarchyOwner, AppError> {
    match (feature_id, work_item_id) {
        (Some(id), None) => Ok(HierarchyOwner::Feature(parse_id::<FeatureId>(&id)?)),
        (None, Some(id)) => Ok(HierarchyOwner::WorkItem(parse_id::<WorkItemId>(&id)?)),
        _ => Err(AppError::Domain("invalid follow-up owner".to_owned())),
    }
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

fn parse_wire<T>(value: &str) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn follow_up_error(code: &str, message: &str) -> AppError {
    AppError::External {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

pub fn workflow_token_environment(token: &str) -> (&'static str, String) {
    (WORKBOARD_WORKFLOW_TOKEN_ENV, token.to_owned())
}

fn claude_request(request: &ProviderFollowUp) -> workboard_adapter_claude::ClaudeFollowUpRequest {
    workboard_adapter_claude::ClaudeFollowUpRequest {
        executable: request.executable.clone(),
        native_id: request.native_id.clone(),
        working_directory: request.working_directory.clone(),
        capability_bundle_root: request.capability_bundle_root.clone(),
        workflow_token: request.workflow_token.clone(),
        text: request.text.clone(),
        client_message_id: request.client_message_id.clone(),
        active_turn: request.active_turn,
    }
}

fn codex_request(request: &ProviderFollowUp) -> workboard_adapter_codex::CodexFollowUpRequest {
    workboard_adapter_codex::CodexFollowUpRequest {
        native_id: request.native_id.clone(),
        working_directory: request.working_directory.clone(),
        capability_bundle_root: request.capability_bundle_root.clone(),
        workflow_token: request.workflow_token.clone(),
        text: request.text.clone(),
        client_message_id: request.client_message_id.clone(),
    }
}

fn codex_receipt(receipt: workboard_adapter_codex::CodexFollowUpReceipt) -> String {
    serde_json::json!({
        "provider": "codex",
        "receiptHash": digest(&receipt.turn_id),
        "steered": receipt.steered
    })
    .to_string()
}

fn map_claude_failure(
    failure: workboard_adapter_claude::ClaudeFollowUpFailure,
) -> FollowUpDeliveryFailure {
    let kind = match failure.kind {
        workboard_adapter_claude::ClaudeFollowUpFailureKind::Deferred => {
            FollowUpDeliveryFailureKind::Deferred
        }
        workboard_adapter_claude::ClaudeFollowUpFailureKind::Rejected => {
            FollowUpDeliveryFailureKind::Rejected
        }
        workboard_adapter_claude::ClaudeFollowUpFailureKind::Uncertain => {
            FollowUpDeliveryFailureKind::Uncertain
        }
    };
    FollowUpDeliveryFailure {
        kind,
        message: failure.message,
    }
}

fn map_codex_failure(
    failure: workboard_adapter_codex::CodexAppServerFailure,
) -> FollowUpDeliveryFailure {
    use workboard_adapter_codex::CodexAppServerFailureKind;

    let kind = match failure.kind {
        CodexAppServerFailureKind::Timeout | CodexAppServerFailureKind::ProcessExited => {
            FollowUpDeliveryFailureKind::Uncertain
        }
        CodexAppServerFailureKind::Io
        | CodexAppServerFailureKind::Protocol
        | CodexAppServerFailureKind::MessageTooLarge
        | CodexAppServerFailureKind::StderrTooLarge
        | CodexAppServerFailureKind::MessageLimitExceeded
        | CodexAppServerFailureKind::PageLimitExceeded
        | CodexAppServerFailureKind::ThreadLimitExceeded
        | CodexAppServerFailureKind::UnsupportedSchema => FollowUpDeliveryFailureKind::Rejected,
    };
    FollowUpDeliveryFailure {
        kind,
        message: failure.message,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        CheckoutId, ConversationId, FeatureId, HierarchyOwner, RepositoryId, WorkItemId,
        WorkspaceId,
    };

    use super::{
        FollowUpDeliveryFailure, FollowUpDeliveryFailureKind, FollowUpExecutor, FollowUpService,
        ProviderFollowUp, SendSessionFollowUp, SessionFollowUpStatus, read_by_idempotency,
    };
    use crate::AppError;
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        workspace_id: WorkspaceId,
        work_item_id: WorkItemId,
        session_id: ConversationId,
    }

    #[derive(Debug)]
    struct AcknowledgingExecutor;

    impl FollowUpExecutor for AcknowledgingExecutor {
        fn reconcile(
            &self,
            _request: &ProviderFollowUp,
        ) -> Result<Option<String>, FollowUpDeliveryFailure> {
            Ok(None)
        }

        fn deliver(&self, request: &ProviderFollowUp) -> Result<String, FollowUpDeliveryFailure> {
            Ok(format!("receipt:{}", request.client_message_id))
        }
    }

    struct DeferredExecutor;

    impl FollowUpExecutor for DeferredExecutor {
        fn reconcile(
            &self,
            _request: &ProviderFollowUp,
        ) -> Result<Option<String>, FollowUpDeliveryFailure> {
            Ok(None)
        }

        fn deliver(&self, _request: &ProviderFollowUp) -> Result<String, FollowUpDeliveryFailure> {
            Err(FollowUpDeliveryFailure {
                kind: FollowUpDeliveryFailureKind::Deferred,
                message: "active turn".to_owned(),
            })
        }
    }

    struct UncertainExecutor;

    impl FollowUpExecutor for UncertainExecutor {
        fn reconcile(
            &self,
            _request: &ProviderFollowUp,
        ) -> Result<Option<String>, FollowUpDeliveryFailure> {
            Ok(None)
        }

        fn deliver(&self, _request: &ProviderFollowUp) -> Result<String, FollowUpDeliveryFailure> {
            Err(FollowUpDeliveryFailure {
                kind: FollowUpDeliveryFailureKind::Uncertain,
                message: "connection lost".to_owned(),
            })
        }
    }

    #[test]
    fn queues_idempotently_and_delivers_fifo_against_the_exact_binding() {
        let mut fixture = fixture();
        let now = OffsetDateTime::now_utc();
        let first_request = request(&fixture, "first", "follow-up-one", now);
        let second_request = request(&fixture, "second", "follow-up-two", now);

        let first = FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, first_request.clone())
            .expect("queue first");
        let replay = FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, first_request)
            .expect("replay first");
        let second = FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, second_request)
            .expect("queue second");

        assert_eq!(first, replay);
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        let delivered_first = FollowUpService::new(&mut fixture.store)
            .deliver_next(Some(fixture.session_id), now, &AcknowledgingExecutor)
            .expect("deliver first")
            .expect("first outcome");
        let delivered_second = FollowUpService::new(&mut fixture.store)
            .deliver_next(Some(fixture.session_id), now, &AcknowledgingExecutor)
            .expect("deliver second")
            .expect("second outcome");

        assert_eq!(delivered_first.follow_up_id, first.follow_up_id);
        assert_eq!(delivered_second.follow_up_id, second.follow_up_id);
        assert_eq!(delivered_first.status, SessionFollowUpStatus::Delivered);
        assert_eq!(delivered_second.status, SessionFollowUpStatus::Delivered);
        assert_eq!(delivered_first.binding_generation, 3);
        assert_eq!(delivered_first.checkout_generation, 7);
    }

    #[test]
    fn rejects_changed_replays_and_stale_binding_generations() {
        let mut fixture = fixture();
        let now = OffsetDateTime::now_utc();
        let original = request(&fixture, "original", "stable-key", now);
        FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, original.clone())
            .expect("queue original");
        let mut changed = original;
        changed.text = "changed".to_owned();
        assert!(matches!(
            FollowUpService::new(&mut fixture.store).queue_for_board(fixture.workspace_id, changed),
            Err(AppError::IdempotencyConflict)
        ));

        let mut stale = request(&fixture, "stale", "stale-key", now);
        stale.expected_binding_generation = 2;
        let error = FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, stale)
            .expect_err("stale binding");
        assert_eq!(error.code(), "follow_up_binding_generation_mismatch");
    }

    #[test]
    fn defers_active_delivery_and_retries_only_after_reconciliation() {
        let mut fixture = fixture();
        let now = OffsetDateTime::now_utc();
        let request = request(&fixture, "deferred", "deferred-key", now);
        let queued = FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, request)
            .expect("queue follow-up");
        let deferred = FollowUpService::new(&mut fixture.store)
            .deliver_next(Some(fixture.session_id), now, &DeferredExecutor)
            .expect("defer delivery")
            .expect("deferred outcome");
        assert_eq!(deferred.status, SessionFollowUpStatus::Pending);
        assert_eq!(deferred.attempt_count, 1);

        let delivered = FollowUpService::new(&mut fixture.store)
            .deliver_next(
                Some(fixture.session_id),
                now + time::Duration::seconds(1),
                &AcknowledgingExecutor,
            )
            .expect("retry delivery")
            .expect("delivered outcome");
        assert_eq!(delivered.follow_up_id, queued.follow_up_id);
        assert_eq!(delivered.status, SessionFollowUpStatus::Delivered);
        assert_eq!(delivered.attempt_count, 2);
    }

    #[test]
    fn uncertain_delivery_is_never_replayed_blindly() {
        let mut fixture = fixture();
        let now = OffsetDateTime::now_utc();
        let request = request(&fixture, "uncertain", "uncertain-key", now);
        FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, request)
            .expect("queue follow-up");
        let uncertain = FollowUpService::new(&mut fixture.store)
            .deliver_next(Some(fixture.session_id), now, &UncertainExecutor)
            .expect("uncertain delivery")
            .expect("uncertain outcome");
        assert_eq!(uncertain.status, SessionFollowUpStatus::Pending);

        let failed = FollowUpService::new(&mut fixture.store)
            .deliver_next(
                Some(fixture.session_id),
                now + time::Duration::seconds(1),
                &AcknowledgingExecutor,
            )
            .expect("reconcile uncertain delivery")
            .expect("failed outcome");
        assert_eq!(failed.status, SessionFollowUpStatus::Failed);
        assert!(
            failed
                .failure
                .as_deref()
                .is_some_and(|failure| failure.contains("could not be reconciled"))
        );
    }

    #[test]
    fn a_rebound_session_fails_the_queued_generation_without_redirecting_it() {
        let mut fixture = fixture();
        let now = OffsetDateTime::now_utc();
        let request = request(&fixture, "bound", "rebound-key", now);
        FollowUpService::new(&mut fixture.store)
            .queue_for_board(fixture.workspace_id, request)
            .expect("queue follow-up");
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE managed_sessions SET binding_generation = 4
                     WHERE session_id = ?1 AND managed_until IS NULL",
                    [fixture.session_id.to_string()],
                )?;
                Ok(())
            })
            .expect("advance binding generation");

        assert!(
            FollowUpService::new(&mut fixture.store)
                .deliver_next(Some(fixture.session_id), now, &AcknowledgingExecutor)
                .expect("reject rebound delivery")
                .is_none()
        );
        let failed = read_by_idempotency(&fixture.store, "rebound-key")
            .expect("read failed follow-up")
            .expect("failed follow-up");
        assert_eq!(failed.status, SessionFollowUpStatus::Failed);
        assert_eq!(failed.binding_generation, 3);
    }

    fn request(
        fixture: &Fixture,
        text: &str,
        idempotency_key: &str,
        requested_at: OffsetDateTime,
    ) -> SendSessionFollowUp {
        SendSessionFollowUp {
            owner: HierarchyOwner::WorkItem(fixture.work_item_id),
            session_id: Some(fixture.session_id),
            expected_binding_generation: 3,
            text: text.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            requested_at,
        }
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let checkout_path = directory.path().join("checkout");
        let bundle_path = directory.path().join("bundle");
        std::fs::create_dir_all(&checkout_path).expect("checkout directory");
        std::fs::create_dir_all(&bundle_path).expect("bundle directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = workboard_core::EpicId::generate();
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        let checkout_id = CheckoutId::generate();
        let checkout_path_id = workboard_core::CheckoutPathId::generate();
        let session_id = ConversationId::generate();
        let association_id = workboard_core::AssociationIntervalId::generate();
        let managed_session_id = workboard_core::ManagedSessionId::generate();
        let launch_intent_id = workboard_core::LaunchIntentId::generate();
        let now = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        let later = (OffsetDateTime::now_utc() + time::Duration::days(1))
            .unix_timestamp_nanos()
            .to_string();
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (
                         id, slug, title, planning_store_repository_id, created_at
                     ) VALUES (?1, 'workspace', 'Workspace', ?2, ?3)",
                    rusqlite::params![workspace_id.to_string(), repository_id.to_string(), now,],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory,
                         default_branch, is_planning_store, created_at
                     ) VALUES (?1, ?2, 'repository', 'Repository', ?3, 'main', 1, ?4)",
                    rusqlite::params![
                        repository_id.to_string(),
                        workspace_id.to_string(),
                        checkout_path.join(".git").display().to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'epic', 'Epic', ?3)",
                    rusqlite::params![epic_id.to_string(), workspace_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'feature', 'Feature', 'planned', ?3)",
                    rusqlite::params![feature_id.to_string(), epic_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at, proposal_order
                     ) VALUES (?1, ?2, 'epic/feature/item', 'item', 'Item', 'in_progress', ?3, 0)",
                    rusqlite::params![work_item_id.to_string(), feature_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    rusqlite::params![work_item_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, head, availability,
                         created_at
                     ) VALUES (?1, ?2, 'worktree', 'feature/item', 'head', 'available', ?3)",
                    rusqlite::params![checkout_id.to_string(), repository_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from
                     ) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        checkout_path_id.to_string(),
                        checkout_id.to_string(),
                        checkout_path.display().to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES (?1, 'codex', 'native-secret', ?2)",
                    rusqlite::params![session_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_readiness (
                         checkout_id, schema_version, repository_id, checkout_path_id, purpose,
                         access_mode, owner_kind, owner_id, session_key, base_revision,
                         source_revision, path, git_worktree_identity, branch, head, availability,
                         isolation_generation, reconciliation_generation, evidence_json, observed_at
                     ) VALUES (
                         ?1, 1, ?2, ?3, 'work_item_write', 'write_isolated', 'work_item', ?4, '',
                         'base', 'source', ?5, 'worktree', 'feature/item', 'head', 'available',
                         1, 7, '{}', ?6
                     )",
                    rusqlite::params![
                        checkout_id.to_string(),
                        repository_id.to_string(),
                        checkout_path_id.to_string(),
                        work_item_id.to_string(),
                        checkout_path.display().to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_associations (
                         id, session_id, work_item_id, role, associated_from
                     ) VALUES (?1, ?2, ?3, 'work_item_execution', ?4)",
                    rusqlite::params![
                        association_id.to_string(),
                        session_id.to_string(),
                        work_item_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO launch_intents (
                         id, work_item_id, checkout_id, provider, idempotency_key, token_hash,
                         status, created_at, expires_at, role, workflow_token_hash,
                         workflow_token_expires_at, capability_bundle_root
                     ) VALUES (
                         ?1, ?2, ?3, 'codex', 'launch-key', 'launch-token', 'bound', ?4, ?5,
                         'work_item_execution', 'workflow-token', ?5, ?6
                     )",
                    rusqlite::params![
                        launch_intent_id.to_string(),
                        work_item_id.to_string(),
                        checkout_id.to_string(),
                        now,
                        later,
                        bundle_path.display().to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO managed_sessions (
                         id, launch_intent_id, session_id, checkout_id, role, status,
                         managed_from, binding_generation
                     ) VALUES (?1, ?2, ?3, ?4, 'work_item_execution', 'bound', ?5, 3)",
                    rusqlite::params![
                        managed_session_id.to_string(),
                        launch_intent_id.to_string(),
                        session_id.to_string(),
                        checkout_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO live_observations (
                         id, session_id, source, status, observed_at, expires_at, cwd, executable
                     ) VALUES (?1, ?2, 'codex_app_server', 'idle', ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        workboard_core::LiveObservationId::generate().to_string(),
                        session_id.to_string(),
                        now,
                        later,
                        checkout_path.display().to_string(),
                        checkout_path.join("codex.exe").display().to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect("seed follow-up fixture");
        Fixture {
            _directory: directory,
            store,
            workspace_id,
            work_item_id,
            session_id,
        }
    }
}
