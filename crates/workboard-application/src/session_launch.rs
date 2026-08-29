use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_core::{
    AssociationIntervalId, CheckoutId, ConversationId, ConversationRef, HierarchyOwner,
    LiveEvidenceSource, LiveObservationId, ManagedLaunchMode, ManagedSessionId, ManagedSessionRole,
    ProcessIdentity, RestoreMembershipId, Tool,
};

use crate::AppError;
use crate::hooks::{
    HOOK_OBSERVATION_TTL_SECONDS, HookIngestionMutation, NativeHookEventKind, parse_hook,
};
use crate::integration_service::record_hook_observation;
use crate::native_launch::{
    ManagedLaunchExecutor, ManagedLaunchPreview, PrepareManagedLaunch, PreparedManagedLaunch,
    ResumeContext, prepare_managed_launch, validate_native_source,
};
use crate::planning_workflow::activate_planning_for_binding;
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginManagedSessionLaunch {
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub mode: ManagedLaunchMode,
    pub checkout_id: CheckoutId,
    pub working_directory: PathBuf,
    pub title: String,
    pub terminal_window: Option<String>,
    pub terminal_executable: PathBuf,
    pub native_executable: PathBuf,
    pub idempotency_key: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub resume_context: Option<ResumeContext>,
    pub initial_prompt: Option<String>,
}

pub struct PreparedSessionLaunch {
    pub intent_id: workboard_core::LaunchIntentId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub checkout_id: CheckoutId,
    pub prepared: PreparedManagedLaunch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedSessionLaunchPreview {
    pub intent_id: workboard_core::LaunchIntentId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub checkout_id: CheckoutId,
    pub launch: ManagedLaunchPreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutedSessionLaunch {
    pub intent_id: workboard_core::LaunchIntentId,
    pub terminal_process: ProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmedSessionBinding {
    pub intent_id: Option<workboard_core::LaunchIntentId>,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub native_id: String,
    pub session_id: ConversationId,
    pub checkout_id: CheckoutId,
    pub restore_membership_id: Option<RestoreMembershipId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum HookIngestionOutcome {
    Bound {
        binding: ConfirmedSessionBinding,
    },
    Observed {
        tool: Tool,
        native_id: String,
        session_id: ConversationId,
        event: NativeHookEventKind,
    },
}

pub struct SessionLaunchService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> SessionLaunchService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn begin(
        &mut self,
        request: BeginManagedSessionLaunch,
    ) -> Result<PreparedSessionLaunch, AppError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(AppError::EmptyIdempotencyKey);
        }
        if request.expires_at <= request.created_at {
            return Err(AppError::Domain(
                "launch intent expiry must follow creation".to_owned(),
            ));
        }
        validate_owner_checkout(self.store, request.owner, request.checkout_id)?;
        validate_checkout_cwd(self.store, request.checkout_id, &request.working_directory)?;
        if let ManagedLaunchMode::Resume(native_id) = &request.mode {
            reject_confirmed_live(self.store, request.tool, native_id, request.created_at)?;
        }
        let expected_native_id = match (&request.mode, &request.resume_context) {
            (ManagedLaunchMode::New, None) => None,
            (ManagedLaunchMode::New, Some(_)) => {
                return Err(AppError::Domain(
                    "new session launch cannot carry resume context".to_owned(),
                ));
            }
            (ManagedLaunchMode::Resume(_), None) => {
                return Err(AppError::ConversationNotResumable(
                    "managed resume requires recorded native source evidence".to_owned(),
                ));
            }
            (ManagedLaunchMode::Resume(native_id), Some(context)) => {
                if !paths_equal(&context.working_directory, &request.working_directory) {
                    return Err(AppError::CallerIdentityMismatch);
                }
                let conversation = ConversationRef::new(request.tool, native_id.clone())
                    .map_err(|error| AppError::Domain(error.to_string()))?;
                validate_native_source(&conversation, context)?;
                Some(native_id.clone())
            }
        };
        let duplicate = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT 1 FROM launch_intents WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(Into::into)
        })?;
        if duplicate.is_some() {
            return Err(AppError::DuplicateConfirmed);
        }
        let token = Uuid::new_v4().to_string();
        let workflow_token = Uuid::new_v4().to_string();
        let prepared = prepare_managed_launch(PrepareManagedLaunch {
            tool: request.tool,
            mode: request.mode.clone(),
            working_directory: request.working_directory.clone(),
            title: request.title.clone(),
            terminal_window: request.terminal_window.clone(),
            terminal: request.terminal_executable.clone(),
            native: request.native_executable.clone(),
            launch_token: token.clone(),
            workflow_token: Some(workflow_token.clone()),
            initial_prompt: request.initial_prompt.clone(),
        })?;
        let intent_id = workboard_core::LaunchIntentId::generate();
        let launch_token_hash = token_hash(&token);
        let workflow_token_hash = token_hash(&workflow_token);
        self.store.write(|transaction| {
            insert_launch_intent(
                transaction,
                intent_id,
                &request,
                expected_native_id.as_deref(),
                &launch_token_hash,
                &workflow_token_hash,
            )
        })?;
        Ok(PreparedSessionLaunch {
            intent_id,
            owner: request.owner,
            role: request.role,
            tool: request.tool,
            checkout_id: request.checkout_id,
            prepared,
        })
    }

    pub fn preview(prepared: &PreparedSessionLaunch) -> ManagedSessionLaunchPreview {
        ManagedSessionLaunchPreview {
            intent_id: prepared.intent_id,
            owner: prepared.owner,
            role: prepared.role,
            tool: prepared.tool,
            checkout_id: prepared.checkout_id,
            launch: prepared.prepared.preview.clone(),
        }
    }

    pub fn execute(
        &mut self,
        prepared: &PreparedSessionLaunch,
        executor: &impl ManagedLaunchExecutor,
    ) -> Result<ExecutedSessionLaunch, AppError> {
        let launched = match executor.launch(&prepared.prepared.launch) {
            Ok(launched) => launched,
            Err(error) => {
                self.fail(prepared.intent_id, &error.to_string())?;
                return Err(error);
            }
        };
        let updated = self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE launch_intents
                     SET status = 'launched', terminal_pid = ?2
                     WHERE id = ?1 AND status = 'pending'",
                    params![
                        prepared.intent_id.to_string(),
                        launched.product_identity.pid(),
                    ],
                )
                .map_err(Into::into)
        })?;
        if updated != 1 {
            return Err(AppError::LaunchLeaseLost);
        }
        Ok(ExecutedSessionLaunch {
            intent_id: prepared.intent_id,
            terminal_process: launched.product_identity,
        })
    }

    pub fn cancel(&mut self, intent_id: workboard_core::LaunchIntentId) -> Result<(), AppError> {
        let updated = self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE launch_intents SET status = 'cancelled'
                     WHERE id = ?1 AND status IN ('pending', 'launched')",
                    [intent_id.to_string()],
                )
                .map_err(Into::into)
        })?;
        if updated != 1 {
            return Err(AppError::LaunchLeaseLost);
        }
        Ok(())
    }

    pub fn reconcile_expired(&mut self, now: OffsetDateTime) -> Result<usize, AppError> {
        self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE launch_intents SET status = 'expired'
                     WHERE status IN ('pending', 'launched') AND expires_at <= ?1",
                    [timestamp(now)],
                )
                .map_err(Into::into)
        })
    }

    pub fn binding_for_intent(
        &self,
        intent_id: workboard_core::LaunchIntentId,
    ) -> Result<Option<ConfirmedSessionBinding>, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT intent.epic_id, intent.feature_id, intent.work_item_id,
                            managed.role, intent.provider, session.native_id,
                            session.id, managed.checkout_id, membership.id
                     FROM launch_intents intent
                     JOIN managed_sessions managed ON managed.launch_intent_id = intent.id
                     JOIN native_sessions session ON session.id = managed.session_id
                     LEFT JOIN restore_memberships membership
                       ON membership.session_id = session.id AND membership.active_until IS NULL
                     WHERE intent.id = ?1 AND intent.status = 'bound'",
                    [intent_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, Option<String>>(8)?,
                        ))
                    },
                )
                .optional()?;
            row.map(
                |(
                    epic_id,
                    feature_id,
                    work_item_id,
                    role,
                    tool,
                    native_id,
                    session_id,
                    checkout_id,
                    restore_membership_id,
                )| {
                    Ok(ConfirmedSessionBinding {
                        intent_id: Some(intent_id),
                        owner: parse_owner(epic_id, feature_id, work_item_id)?,
                        role: parse_role(&role)?,
                        tool: parse_tool(&tool)?,
                        native_id,
                        session_id: parse_id(&session_id)?,
                        checkout_id: parse_id(&checkout_id)?,
                        restore_membership_id: restore_membership_id
                            .as_deref()
                            .map(parse_id)
                            .transpose()?,
                    })
                },
            )
            .transpose()
        })
    }

    pub fn bind_hook(
        &mut self,
        mutation: &HookIngestionMutation,
    ) -> Result<ConfirmedSessionBinding, AppError> {
        let observation = parse_hook(mutation)?;
        if observation.event != NativeHookEventKind::SessionStart {
            return Err(AppError::InvalidHookInput(
                "managed launch binding requires SessionStart".to_owned(),
            ));
        }
        let token = mutation
            .launch_token
            .as_deref()
            .ok_or(AppError::LaunchTokenInvalid)?;
        let intent = read_launch_intent(self.store, &token_hash(token))?
            .ok_or(AppError::LaunchTokenInvalid)?;
        if !matches!(intent.status.as_str(), "pending" | "launched") {
            return Err(AppError::LaunchTokenInvalid);
        }
        if intent.expires_at < observation.observed_at {
            return Err(AppError::LaunchTokenInvalid);
        }
        if intent.tool != mutation.tool {
            return Err(AppError::CallerIdentityMismatch);
        }
        if intent
            .expected_native_id
            .as_deref()
            .is_some_and(|expected| expected != observation.conversation.native_id())
        {
            return Err(AppError::CallerIdentityMismatch);
        }
        validate_checkout_cwd(self.store, intent.checkout_id, &observation.cwd)?;
        let expires_at =
            observation.observed_at + time::Duration::seconds(HOOK_OBSERVATION_TTL_SECONDS);
        self.store.write(|transaction| {
            bind_transaction(
                transaction,
                Some(intent.id),
                intent.owner,
                intent.role,
                intent.checkout_id,
                mutation.tool,
                observation.conversation.native_id(),
                observation.observed_at,
                expires_at,
                &observation.cwd,
                mutation.process.as_ref(),
                "bound",
            )
        })
    }

    pub fn ingest_hook(
        &mut self,
        mutation: &HookIngestionMutation,
    ) -> Result<HookIngestionOutcome, AppError> {
        if mutation.launch_token.is_some() {
            let observation = parse_hook(mutation)?;
            let binding = self.bind_hook(mutation)?;
            self.store.write(|transaction| {
                record_hook_observation(transaction, mutation.tool, observation.observed_at)
            })?;
            return Ok(HookIngestionOutcome::Bound { binding });
        }
        let observation = parse_hook(mutation)?;
        let expires_at =
            observation.observed_at + time::Duration::seconds(HOOK_OBSERVATION_TTL_SECONDS);
        let native_id = observation.conversation.native_id().to_owned();
        let session_id = self.store.write(|transaction| {
            record_hook_observation(transaction, mutation.tool, observation.observed_at)?;
            let session_id = ensure_native_session(
                transaction,
                mutation.tool,
                &native_id,
                observation.observed_at,
            )?;
            insert_live_observation(
                transaction,
                LiveObservationInput {
                    session_id,
                    tool: mutation.tool,
                    status: observation.event.status(),
                    observed_at: observation.observed_at,
                    expires_at,
                    cwd: &observation.cwd,
                    process: mutation.process.as_ref(),
                },
            )?;
            if observation.event == NativeHookEventKind::SessionEnd {
                close_managed_session(transaction, session_id, observation.observed_at)?;
            }
            Ok(session_id)
        })?;
        Ok(HookIngestionOutcome::Observed {
            tool: mutation.tool,
            native_id,
            session_id,
            event: observation.event,
        })
    }

    pub fn adopt_hook(
        &mut self,
        owner: HierarchyOwner,
        checkout_id: CheckoutId,
        mutation: &HookIngestionMutation,
    ) -> Result<ConfirmedSessionBinding, AppError> {
        if !matches!(owner, HierarchyOwner::WorkItem(_)) {
            return Err(AppError::WorkItemRequired);
        }
        let observation = parse_hook(mutation)?;
        if !observation.event.status().indicates_live() || mutation.process.is_none() {
            return Err(AppError::CallerIdentityUncorrelated);
        }
        validate_owner_checkout(self.store, owner, checkout_id)?;
        validate_checkout_cwd(self.store, checkout_id, &observation.cwd)?;
        let expires_at =
            observation.observed_at + time::Duration::seconds(HOOK_OBSERVATION_TTL_SECONDS);
        self.store.write(|transaction| {
            bind_transaction(
                transaction,
                None,
                owner,
                ManagedSessionRole::WorkItemExecution,
                checkout_id,
                mutation.tool,
                observation.conversation.native_id(),
                observation.observed_at,
                expires_at,
                &observation.cwd,
                mutation.process.as_ref(),
                "adopted",
            )
        })
    }

    pub fn adopt_observed(
        &mut self,
        owner: HierarchyOwner,
        checkout_id: CheckoutId,
        conversation: &ConversationRef,
        cwd: &Path,
        observed_at: OffsetDateTime,
    ) -> Result<ConfirmedSessionBinding, AppError> {
        if !matches!(owner, HierarchyOwner::WorkItem(_)) {
            return Err(AppError::WorkItemRequired);
        }
        validate_owner_checkout(self.store, owner, checkout_id)?;
        validate_checkout_cwd(self.store, checkout_id, cwd)?;
        let evidence = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT session.id, observation.status, observation.expires_at,
                            observation.cwd
                     FROM native_sessions session
                     JOIN live_observations observation ON observation.session_id = session.id
                     WHERE session.provider = ?1 AND session.native_id = ?2
                       AND observation.source IN ('claude_hook', 'codex_hook', 'codex_app_server')
                     ORDER BY observation.observed_at DESC LIMIT 1",
                    params![tool_name(conversation.tool()), conversation.native_id()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let (session_id, status, expires_at, evidence_cwd) =
            evidence.ok_or(AppError::CallerIdentityUncorrelated)?;
        if parse_timestamp(&expires_at)? <= observed_at {
            return Err(AppError::CallerIdentityExpired);
        }
        if !matches!(status.as_str(), "active" | "idle") {
            return Err(AppError::CallerIdentityNotActive);
        }
        if evidence_cwd
            .as_deref()
            .is_none_or(|path| !paths_equal(Path::new(path), cwd))
        {
            return Err(AppError::CallerIdentityMismatch);
        }
        let session_id = parse_id::<ConversationId>(&session_id)?;
        let expires_at = observed_at + time::Duration::seconds(HOOK_OBSERVATION_TTL_SECONDS);
        self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT 1 FROM native_sessions WHERE id = ?1",
                    [session_id.to_string()],
                    |_| Ok(()),
                )
                .optional()?;
            if existing.is_none() {
                return Err(AppError::CallerIdentityUncorrelated);
            }
            bind_transaction(
                transaction,
                None,
                owner,
                ManagedSessionRole::WorkItemExecution,
                checkout_id,
                conversation.tool(),
                conversation.native_id(),
                observed_at,
                expires_at,
                cwd,
                None,
                "adopted",
            )
        })
    }

    fn fail(
        &mut self,
        intent_id: workboard_core::LaunchIntentId,
        failure: &str,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE launch_intents SET status = 'failed', failure = ?2 WHERE id = ?1",
                params![intent_id.to_string(), failure],
            )?;
            Ok(())
        })
    }
}

struct LaunchIntentRecord {
    id: workboard_core::LaunchIntentId,
    owner: HierarchyOwner,
    role: ManagedSessionRole,
    tool: Tool,
    checkout_id: CheckoutId,
    status: String,
    expires_at: OffsetDateTime,
    expected_native_id: Option<String>,
}

fn read_launch_intent(
    store: &SqliteStore,
    token_hash: &str,
) -> Result<Option<LaunchIntentRecord>, AppError> {
    store.read(|connection| {
        let row = connection
            .query_row(
                "SELECT id, epic_id, feature_id, work_item_id, role, provider,
                        checkout_id, status, expires_at, expected_native_id
                 FROM launch_intents WHERE token_hash = ?1",
                [token_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                id,
                epic_id,
                feature_id,
                work_item_id,
                role,
                tool,
                checkout_id,
                status,
                expires_at,
                expected_native_id,
            )| {
                Ok(LaunchIntentRecord {
                    id: parse_id(&id)?,
                    owner: parse_owner(epic_id, feature_id, work_item_id)?,
                    role: parse_role(&role)?,
                    tool: parse_tool(&tool)?,
                    checkout_id: parse_id(&checkout_id)?,
                    status,
                    expires_at: parse_timestamp(&expires_at)?,
                    expected_native_id,
                })
            },
        )
        .transpose()
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_transaction(
    transaction: &Transaction<'_>,
    intent_id: Option<workboard_core::LaunchIntentId>,
    owner: HierarchyOwner,
    role: ManagedSessionRole,
    checkout_id: CheckoutId,
    tool: Tool,
    native_id: &str,
    observed_at: OffsetDateTime,
    expires_at: OffsetDateTime,
    cwd: &Path,
    process: Option<&ProcessIdentity>,
    managed_status: &str,
) -> Result<ConfirmedSessionBinding, AppError> {
    let session_id = ensure_native_session(transaction, tool, native_id, observed_at)?;
    let current_owner = transaction
        .query_row(
            "SELECT epic_id, feature_id, work_item_id
             FROM native_session_associations
             WHERE session_id = ?1 AND associated_until IS NULL",
            [session_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((epic_id, feature_id, work_item_id)) = current_owner {
        if parse_owner(epic_id, feature_id, work_item_id)? != owner {
            return Err(AppError::ConversationAlreadyAssigned);
        }
    } else {
        insert_association(transaction, session_id, owner, role, observed_at)?;
    }
    let managed_session_id = transaction
        .query_row(
            "SELECT id FROM managed_sessions
             WHERE session_id = ?1 AND managed_until IS NULL",
            [session_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|id| parse_id::<ManagedSessionId>(&id))
        .transpose()?
        .unwrap_or_else(ManagedSessionId::generate);
    transaction.execute(
        "INSERT OR IGNORE INTO managed_sessions (
             id, launch_intent_id, session_id, checkout_id, role, status, managed_from
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            managed_session_id.to_string(),
            intent_id.map(|id| id.to_string()),
            session_id.to_string(),
            checkout_id.to_string(),
            role_name(role)?,
            managed_status,
            timestamp(observed_at),
        ],
    )?;
    let restore_membership_id =
        ensure_restore_membership(transaction, session_id, owner, observed_at)?;
    ensure_restore_entry(transaction, session_id, owner, observed_at)?;
    activate_planning_for_binding(transaction, owner, role, observed_at)?;
    insert_live_observation(
        transaction,
        LiveObservationInput {
            session_id,
            tool,
            status: workboard_core::LiveStatus::Active,
            observed_at,
            expires_at,
            cwd,
            process,
        },
    )?;
    if let Some(intent_id) = intent_id {
        transaction.execute(
            "UPDATE launch_intents SET status = 'bound'
             WHERE id = ?1 AND status IN ('pending', 'launched')",
            [intent_id.to_string()],
        )?;
    }
    Ok(ConfirmedSessionBinding {
        intent_id,
        owner,
        role,
        tool,
        native_id: native_id.to_owned(),
        session_id,
        checkout_id,
        restore_membership_id,
    })
}

fn ensure_native_session(
    transaction: &Transaction<'_>,
    tool: Tool,
    native_id: &str,
    observed_at: OffsetDateTime,
) -> Result<ConversationId, AppError> {
    let existing = transaction
        .query_row(
            "SELECT id FROM native_sessions WHERE provider = ?1 AND native_id = ?2",
            params![tool_name(tool), native_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        return parse_id(&existing);
    }
    let session_id = ConversationId::generate();
    transaction.execute(
        "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            session_id.to_string(),
            tool_name(tool),
            native_id,
            timestamp(observed_at),
        ],
    )?;
    Ok(session_id)
}

fn close_managed_session(
    transaction: &Transaction<'_>,
    session_id: ConversationId,
    observed_at: OffsetDateTime,
) -> Result<(), AppError> {
    let timestamp = timestamp(observed_at);
    transaction.execute(
        "UPDATE managed_sessions
         SET status = 'stopped', managed_until = ?2
         WHERE session_id = ?1 AND managed_until IS NULL",
        params![session_id.to_string(), timestamp],
    )?;
    Ok(())
}

fn insert_launch_intent(
    transaction: &Transaction<'_>,
    intent_id: workboard_core::LaunchIntentId,
    request: &BeginManagedSessionLaunch,
    expected_native_id: Option<&str>,
    token_hash: &str,
    workflow_token_hash: &str,
) -> Result<(), AppError> {
    let (epic_id, feature_id, work_item_id) = owner_columns(request.owner);
    transaction.execute(
        "INSERT INTO launch_intents (
             id, epic_id, feature_id, work_item_id, checkout_id, provider,
             idempotency_key, token_hash, status, created_at, expires_at, role,
             expected_native_id, workflow_token_hash, workflow_token_expires_at,
             terminal_window
         ) VALUES (
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?10, ?11, ?12,
             ?13, ?14, ?15
         )",
        params![
            intent_id.to_string(),
            epic_id,
            feature_id,
            work_item_id,
            request.checkout_id.to_string(),
            tool_name(request.tool),
            request.idempotency_key,
            token_hash,
            timestamp(request.created_at),
            timestamp(request.expires_at),
            role_name(request.role)?,
            expected_native_id,
            workflow_token_hash,
            timestamp(request.created_at + time::Duration::hours(12)),
            request.terminal_window,
        ],
    )?;
    Ok(())
}

fn validate_owner_checkout(
    store: &SqliteStore,
    owner: HierarchyOwner,
    checkout_id: CheckoutId,
) -> Result<(), AppError> {
    let valid = store.read(|connection| {
        let exists = match owner {
            HierarchyOwner::WorkItem(work_item_id) => connection.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM effective_work_item_checkouts
                     WHERE work_item_id = ?1 AND checkout_id = ?2
                 )",
                params![work_item_id.to_string(), checkout_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?,
            HierarchyOwner::Feature(feature_id) => connection.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM feature_checkouts
                     WHERE feature_id = ?1 AND checkout_id = ?2
                 )",
                params![feature_id.to_string(), checkout_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?,
            HierarchyOwner::Epic(epic_id) => connection.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM epics epic
                     JOIN repositories repository ON repository.workspace_id = epic.workspace_id
                     JOIN checkouts checkout ON checkout.repository_id = repository.id
                     WHERE epic.id = ?1 AND checkout.id = ?2
                 )",
                params![epic_id.to_string(), checkout_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?,
        };
        Ok(exists != 0)
    })?;
    if valid {
        Ok(())
    } else {
        Err(AppError::ResumeCheckoutNotScanned)
    }
}

fn validate_checkout_cwd(
    store: &SqliteStore,
    checkout_id: CheckoutId,
    cwd: &Path,
) -> Result<(), AppError> {
    let expected = store.read(|connection| {
        connection
            .query_row(
                "SELECT path.path
                 FROM checkouts checkout
                 JOIN checkout_paths path
                   ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                 WHERE checkout.id = ?1 AND checkout.availability = 'available'",
                [checkout_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Into::into)
    })?;
    let expected = expected.ok_or(AppError::ResumeCheckoutRequired)?;
    if paths_equal(Path::new(&expected), cwd) {
        Ok(())
    } else {
        Err(AppError::CallerIdentityMismatch)
    }
}

fn reject_confirmed_live(
    store: &SqliteStore,
    tool: Tool,
    native_id: &str,
    now: OffsetDateTime,
) -> Result<(), AppError> {
    let live = store.read(|connection| {
        connection
            .query_row(
                "SELECT observation.status, observation.expires_at
                 FROM native_sessions session
                 JOIN live_observations observation ON observation.session_id = session.id
                 WHERE session.provider = ?1 AND session.native_id = ?2
                 ORDER BY observation.observed_at DESC LIMIT 1",
                params![tool_name(tool), native_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(Into::into)
    })?;
    if live.is_some_and(|(status, expires_at)| {
        matches!(status.as_str(), "active" | "idle")
            && parse_timestamp(&expires_at).is_ok_and(|expires_at| expires_at > now)
    }) {
        Err(AppError::DuplicateConfirmed)
    } else {
        Ok(())
    }
}

fn insert_association(
    transaction: &Transaction<'_>,
    session_id: ConversationId,
    owner: HierarchyOwner,
    role: ManagedSessionRole,
    observed_at: OffsetDateTime,
) -> Result<(), AppError> {
    let (epic_id, feature_id, work_item_id) = owner_columns(owner);
    transaction.execute(
        "INSERT INTO native_session_associations (
             id, session_id, epic_id, feature_id, work_item_id, role, associated_from
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            AssociationIntervalId::generate().to_string(),
            session_id.to_string(),
            epic_id,
            feature_id,
            work_item_id,
            role_name(role)?,
            timestamp(observed_at),
        ],
    )?;
    Ok(())
}

fn ensure_restore_membership(
    transaction: &Transaction<'_>,
    session_id: ConversationId,
    owner: HierarchyOwner,
    observed_at: OffsetDateTime,
) -> Result<Option<RestoreMembershipId>, AppError> {
    let feature_id = match owner {
        HierarchyOwner::Epic(_) => None,
        HierarchyOwner::Feature(feature_id) => Some(feature_id),
        HierarchyOwner::WorkItem(work_item_id) => transaction
            .query_row(
                "SELECT feature_id FROM work_items WHERE id = ?1",
                [work_item_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|id| parse_id(&id))
            .transpose()?,
    };
    let Some(feature_id) = feature_id else {
        return Ok(None);
    };
    let existing = transaction
        .query_row(
            "SELECT id FROM restore_memberships
             WHERE session_id = ?1 AND active_until IS NULL",
            [session_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|id| parse_id(&id))
        .transpose()?;
    if existing.is_some() {
        return Ok(existing);
    }
    let id = RestoreMembershipId::generate();
    transaction.execute(
        "INSERT INTO restore_memberships (
             id, session_id, feature_id, active_from, active_until
         ) VALUES (?1, ?2, ?3, ?4, NULL)",
        params![
            id.to_string(),
            session_id.to_string(),
            feature_id.to_string(),
            timestamp(observed_at),
        ],
    )?;
    Ok(Some(id))
}

fn ensure_restore_entry(
    transaction: &Transaction<'_>,
    session_id: ConversationId,
    owner: HierarchyOwner,
    observed_at: OffsetDateTime,
) -> Result<(), AppError> {
    let (epic_id, feature_id, work_item_id) = owner_columns(owner);
    transaction.execute(
        "INSERT INTO restore_entries (
             session_id, epic_id, feature_id, work_item_id, added_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id) DO UPDATE SET
             epic_id = excluded.epic_id,
             feature_id = excluded.feature_id,
             work_item_id = excluded.work_item_id,
             removed_at = NULL,
             remove_reason = NULL",
        params![
            session_id.to_string(),
            epic_id,
            feature_id,
            work_item_id,
            timestamp(observed_at),
        ],
    )?;
    Ok(())
}

struct LiveObservationInput<'a> {
    session_id: ConversationId,
    tool: Tool,
    status: workboard_core::LiveStatus,
    observed_at: OffsetDateTime,
    expires_at: OffsetDateTime,
    cwd: &'a Path,
    process: Option<&'a ProcessIdentity>,
}

fn insert_live_observation(
    transaction: &Transaction<'_>,
    input: LiveObservationInput<'_>,
) -> Result<(), AppError> {
    let source = match input.tool {
        Tool::Claude => LiveEvidenceSource::ClaudeHook,
        Tool::Codex => LiveEvidenceSource::CodexHook,
    };
    transaction.execute(
        "INSERT INTO live_observations (
             id, session_id, source, status, observed_at, expires_at, cwd,
             pid, process_created_at, executable, parent_pid
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            LiveObservationId::generate().to_string(),
            input.session_id.to_string(),
            live_source_name(source),
            live_status_name(input.status),
            timestamp(input.observed_at),
            timestamp(input.expires_at),
            path_text(input.cwd)?,
            input.process.map(ProcessIdentity::pid),
            input
                .process
                .map(|identity| timestamp(identity.created_at())),
            input
                .process
                .map(ProcessIdentity::executable)
                .map(path_text)
                .transpose()?,
            input.process.and_then(ProcessIdentity::parent_pid),
        ],
    )?;
    Ok(())
}

fn live_status_name(status: workboard_core::LiveStatus) -> &'static str {
    match status {
        workboard_core::LiveStatus::Active => "active",
        workboard_core::LiveStatus::Idle => "idle",
        workboard_core::LiveStatus::Stopped => "stopped",
        workboard_core::LiveStatus::Unknown => "unknown",
        workboard_core::LiveStatus::SystemError => "system_error",
        workboard_core::LiveStatus::NotLoaded => "not_loaded",
    }
}

fn owner_columns(owner: HierarchyOwner) -> (Option<String>, Option<String>, Option<String>) {
    match owner {
        HierarchyOwner::Epic(id) => (Some(id.to_string()), None, None),
        HierarchyOwner::Feature(id) => (None, Some(id.to_string()), None),
        HierarchyOwner::WorkItem(id) => (None, None, Some(id.to_string())),
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
        _ => Err(AppError::Domain(
            "launch intent owner is invalid".to_owned(),
        )),
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(AppError::Domain("launch provider is invalid".to_owned())),
    }
}

fn role_name(role: ManagedSessionRole) -> Result<String, AppError> {
    serde_json::to_value(role)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Domain("managed session role is invalid".to_owned()))
}

fn parse_role(value: &str) -> Result<ManagedSessionRole, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn live_source_name(source: LiveEvidenceSource) -> &'static str {
    match source {
        LiveEvidenceSource::ProductLaunch => "product_launch",
        LiveEvidenceSource::ClaudeHook => "claude_hook",
        LiveEvidenceSource::CodexHook => "codex_hook",
        LiveEvidenceSource::CodexAppServer => "codex_app_server",
        LiveEvidenceSource::WindowsProcess => "windows_process",
    }
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, AppError> {
    let nanoseconds = value
        .parse::<i128>()
        .map_err(|error| AppError::Domain(error.to_string()))?;
    OffsetDateTime::from_unix_timestamp_nanos(nanoseconds)
        .map_err(|error| AppError::Domain(error.to_string()))
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

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

#[cfg(windows)]
fn paths_equal(left: &Path, right: &Path) -> bool {
    windows_path_text(left).eq_ignore_ascii_case(&windows_path_text(right))
}

#[cfg(windows)]
fn windows_path_text(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let value = resolved.as_os_str().to_string_lossy();
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{value}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use rusqlite::params;
    use serde_json::json;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        CheckoutId, ConversationRef, EpicId, FeatureId, HierarchyOwner, ManagedLaunchMode,
        ManagedSessionRole, ProcessIdentity, RepositoryId, Tool, WorkItemId, WorkspaceId,
    };

    use super::{BeginManagedSessionLaunch, ManagedLaunchExecutor, SessionLaunchService};
    use crate::AppError;
    use crate::hooks::HookIngestionMutation;
    use crate::native_launch::{LaunchedProcess, ResumeContext, ResumeSource};
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        work_item_id: WorkItemId,
        checkout_id: CheckoutId,
        checkout_path: PathBuf,
        terminal: PathBuf,
        native: PathBuf,
        observed_at: OffsetDateTime,
    }

    struct SuccessfulExecutor {
        process: ProcessIdentity,
    }

    impl ManagedLaunchExecutor for SuccessfulExecutor {
        fn launch(
            &self,
            _specification: &workboard_core::ManagedLaunchSpec,
        ) -> Result<LaunchedProcess, AppError> {
            Ok(LaunchedProcess {
                product_identity: self.process.clone(),
                observed_identity: Some(self.process.clone()),
            })
        }
    }

    struct FailingExecutor;

    impl ManagedLaunchExecutor for FailingExecutor {
        fn launch(
            &self,
            _specification: &workboard_core::ManagedLaunchSpec,
        ) -> Result<LaunchedProcess, AppError> {
            Err(AppError::External {
                code: "fake_launch_failure".to_owned(),
                message: "fake terminal rejected launch".to_owned(),
            })
        }
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let checkout_path = directory.path().join("checkout");
        fs::create_dir(&checkout_path).expect("create checkout");
        let terminal = directory.path().join(terminal_name());
        let native = directory.path().join(native_name());
        fs::write(&terminal, []).expect("terminal fixture");
        fs::write(&native, []).expect("native fixture");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        let checkout_id = CheckoutId::generate();
        let observed_at = OffsetDateTime::parse(
            "2026-08-27T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("fixture time");
        let now = observed_at.unix_timestamp_nanos().to_string();
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (
                         id, slug, title, planning_store_repository_id, created_at
                     ) VALUES (?1, 'demo', 'Demo', ?2, ?3)",
                    params![
                        workspace_id.to_string(),
                        planning_repository_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory,
                         is_planning_store, created_at
                     ) VALUES (?1, ?2, 'planning-store', 'Planning store', ?3, 1, ?4)",
                    params![
                        planning_repository_id.to_string(),
                        workspace_id.to_string(),
                        directory.path().join("planning.git").to_string_lossy(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory,
                         is_planning_store, created_at
                     ) VALUES (?1, ?2, 'demo-code', 'Demo code', ?3, 0, ?4)",
                    params![
                        repository_id.to_string(),
                        workspace_id.to_string(),
                        directory.path().join("code.git").to_string_lossy(),
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
                     ) VALUES (
                         ?1, ?2, 'launch/availability/api', 'api', 'Availability API',
                         'ready', ?3
                     )",
                    params![work_item_id.to_string(), feature_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, availability, created_at
                     ) VALUES (
                         ?1, ?2, 'availability-checkout', 'feature/availability', 'available', ?3
                     )",
                    params![checkout_id.to_string(), repository_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        workboard_core::CheckoutPathId::generate().to_string(),
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
                Ok(())
            })
            .expect("seed launch fixture");
        Fixture {
            _directory: directory,
            store,
            work_item_id,
            checkout_id,
            checkout_path,
            terminal,
            native,
            observed_at,
        }
    }

    fn request(fixture: &Fixture, idempotency_key: &str) -> BeginManagedSessionLaunch {
        BeginManagedSessionLaunch {
            owner: HierarchyOwner::WorkItem(fixture.work_item_id),
            role: ManagedSessionRole::WorkItemExecution,
            tool: Tool::Codex,
            mode: ManagedLaunchMode::New,
            checkout_id: fixture.checkout_id,
            working_directory: fixture.checkout_path.clone(),
            title: "Availability API".to_owned(),
            terminal_window: Some(format!("workboard-work-item-{}", fixture.work_item_id)),
            terminal_executable: fixture.terminal.clone(),
            native_executable: fixture.native.clone(),
            idempotency_key: idempotency_key.to_owned(),
            created_at: fixture.observed_at,
            expires_at: fixture.observed_at + time::Duration::minutes(2),
            resume_context: None,
            initial_prompt: None,
        }
    }

    fn add_resume_source(
        fixture: &Fixture,
        request: &mut BeginManagedSessionLaunch,
        native_id: &str,
    ) {
        let source_path = fixture._directory.path().join("resume.jsonl");
        fs::write(
            &source_path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{native_id}\",\"cwd\":\"{}\"}}}}\n",
                fixture.checkout_path.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .expect("resume source fixture");
        let conversation = workboard_native::NativeConversation::new(
            native_id,
            workboard_native::ConversationKind::TopLevel,
        );
        request.resume_context = Some(ResumeContext {
            working_directory: fixture.checkout_path.clone(),
            title: request.title.clone(),
            sources: vec![ResumeSource {
                path: source_path,
                missing: false,
                snapshot_json: serde_json::to_string(&conversation).expect("snapshot JSON"),
            }],
        });
    }

    fn hook(
        fixture: &Fixture,
        tool: Tool,
        native_id: &str,
        cwd: &Path,
        launch_token: Option<String>,
    ) -> HookIngestionMutation {
        HookIngestionMutation {
            tool,
            payload_json: json!({
                "session_id": native_id,
                "cwd": cwd,
                "hook_event_name": "SessionStart",
                "source": "startup"
            })
            .to_string(),
            observed_at: "2026-08-27T12:00:01Z".to_owned(),
            launch_token,
            process: Some(
                ProcessIdentity::new(42, fixture.observed_at, &fixture.native, Some(7))
                    .expect("process identity"),
            ),
        }
    }

    fn lifecycle_hook(
        fixture: &Fixture,
        native_id: &str,
        event: &str,
        observed_at: &str,
    ) -> HookIngestionMutation {
        HookIngestionMutation {
            tool: Tool::Codex,
            payload_json: json!({
                "session_id": native_id,
                "cwd": fixture.checkout_path,
                "hook_event_name": event
            })
            .to_string(),
            observed_at: observed_at.to_owned(),
            launch_token: None,
            process: None,
        }
    }

    #[cfg(windows)]
    fn terminal_name() -> &'static str {
        "wt.exe"
    }

    #[cfg(not(windows))]
    fn terminal_name() -> &'static str {
        "xdg-terminal-exec"
    }

    #[cfg(windows)]
    fn native_name() -> &'static str {
        "codex.exe"
    }

    #[cfg(not(windows))]
    fn native_name() -> &'static str {
        "codex"
    }

    #[test]
    fn new_launch_binds_exact_native_identity_and_restore_membership() {
        let mut fixture = fixture();
        let first_request = request(&fixture, "launch-api");
        let duplicate_request = request(&fixture, "launch-api");
        let prepared = SessionLaunchService::new(&mut fixture.store)
            .begin(first_request)
            .expect("begin launch");
        let token = prepared.prepared.launch.launch_token().to_owned();
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).begin(duplicate_request),
            Err(AppError::DuplicateConfirmed)
        ));
        let process = ProcessIdentity::new(
            42,
            fixture.observed_at,
            &fixture.terminal,
            Some(std::process::id()),
        )
        .expect("terminal process");
        SessionLaunchService::new(&mut fixture.store)
            .execute(&prepared, &SuccessfulExecutor { process })
            .expect("execute launch");
        let mutation = hook(
            &fixture,
            Tool::Codex,
            "thread-one",
            &fixture.checkout_path,
            Some(token),
        );
        let binding = SessionLaunchService::new(&mut fixture.store)
            .bind_hook(&mutation)
            .expect("bind hook");
        assert_eq!(
            binding.owner,
            HierarchyOwner::WorkItem(fixture.work_item_id)
        );
        assert_eq!(binding.checkout_id, fixture.checkout_id);
        assert!(binding.restore_membership_id.is_some());
        assert_eq!(
            SessionLaunchService::new(&mut fixture.store)
                .binding_for_intent(prepared.intent_id)
                .expect("read confirmed binding"),
            Some(binding.clone())
        );
        let counts = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT
                             (SELECT COUNT(*) FROM native_sessions),
                             (SELECT COUNT(*) FROM native_session_associations),
                             (SELECT COUNT(*) FROM managed_sessions),
                             (SELECT COUNT(*) FROM restore_memberships),
                             (SELECT COUNT(*) FROM live_observations),
                             (SELECT status FROM launch_intents WHERE id = ?1),
                             (SELECT terminal_window FROM launch_intents WHERE id = ?1)",
                        [prepared.intent_id.to_string()],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)?,
                                row.get::<_, i64>(4)?,
                                row.get::<_, String>(5)?,
                                row.get::<_, String>(6)?,
                            ))
                        },
                    )
                    .map_err(Into::into)
            })
            .expect("binding counts");
        assert_eq!(
            counts,
            (
                1,
                1,
                1,
                1,
                1,
                "bound".to_owned(),
                format!("workboard-work-item-{}", fixture.work_item_id)
            )
        );
    }

    #[test]
    fn wrong_expired_cancelled_and_wrong_cwd_tokens_fail_closed() {
        let mut wrong_token_fixture = fixture();
        let wrong_token_request = request(&wrong_token_fixture, "wrong-token");
        let prepared = SessionLaunchService::new(&mut wrong_token_fixture.store)
            .begin(wrong_token_request)
            .expect("begin wrong-token launch");
        let mutation = hook(
            &wrong_token_fixture,
            Tool::Codex,
            "thread-wrong-token",
            &wrong_token_fixture.checkout_path,
            Some("wrong-token".to_owned()),
        );
        assert!(matches!(
            SessionLaunchService::new(&mut wrong_token_fixture.store).bind_hook(&mutation),
            Err(AppError::LaunchTokenInvalid)
        ));

        let mut wrong_cwd_fixture = fixture();
        let wrong_cwd_request = request(&wrong_cwd_fixture, "wrong-cwd");
        let prepared_wrong_cwd = SessionLaunchService::new(&mut wrong_cwd_fixture.store)
            .begin(wrong_cwd_request)
            .expect("begin wrong-cwd launch");
        let wrong_cwd = wrong_cwd_fixture.checkout_path.join("other");
        let mutation = hook(
            &wrong_cwd_fixture,
            Tool::Codex,
            "thread-wrong-cwd",
            &wrong_cwd,
            Some(prepared_wrong_cwd.prepared.launch.launch_token().to_owned()),
        );
        assert!(matches!(
            SessionLaunchService::new(&mut wrong_cwd_fixture.store).bind_hook(&mutation),
            Err(AppError::CallerIdentityMismatch)
        ));

        let mut cancelled_fixture = fixture();
        let cancelled_request = request(&cancelled_fixture, "cancelled");
        let prepared_cancelled = SessionLaunchService::new(&mut cancelled_fixture.store)
            .begin(cancelled_request)
            .expect("begin cancelled launch");
        let mutation = hook(
            &cancelled_fixture,
            Tool::Codex,
            "thread-cancelled",
            &cancelled_fixture.checkout_path,
            Some(prepared_cancelled.prepared.launch.launch_token().to_owned()),
        );
        SessionLaunchService::new(&mut cancelled_fixture.store)
            .cancel(prepared_cancelled.intent_id)
            .expect("cancel launch");
        assert!(matches!(
            SessionLaunchService::new(&mut cancelled_fixture.store).bind_hook(&mutation),
            Err(AppError::LaunchTokenInvalid)
        ));

        let mut expired_fixture = fixture();
        let mut expired_request = request(&expired_fixture, "expired");
        expired_request.expires_at =
            expired_fixture.observed_at + time::Duration::milliseconds(500);
        let prepared_expired = SessionLaunchService::new(&mut expired_fixture.store)
            .begin(expired_request)
            .expect("begin expired launch");
        let mutation = hook(
            &expired_fixture,
            Tool::Codex,
            "thread-expired",
            &expired_fixture.checkout_path,
            Some(prepared_expired.prepared.launch.launch_token().to_owned()),
        );
        assert!(matches!(
            SessionLaunchService::new(&mut expired_fixture.store).bind_hook(&mutation),
            Err(AppError::LaunchTokenInvalid)
        ));
        assert_eq!(
            SessionLaunchService::new(&mut expired_fixture.store)
                .reconcile_expired(expired_fixture.observed_at + time::Duration::seconds(2))
                .expect("reconcile expired launches"),
            1
        );
        assert_ne!(prepared.intent_id, prepared_expired.intent_id);
    }

    #[test]
    fn wrong_requested_cwd_creates_no_launch_intent() {
        let mut fixture = fixture();
        let mut request = request(&fixture, "wrong-requested-cwd");
        request.working_directory = fixture.checkout_path.join("other");

        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).begin(request),
            Err(AppError::CallerIdentityMismatch)
        ));
        let intents = fixture
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM launch_intents", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("launch intent count");
        assert_eq!(intents, 0);
    }

    #[test]
    fn launch_crash_is_durable_and_repairable() {
        let mut fixture = fixture();
        let request = request(&fixture, "crash");
        let prepared = SessionLaunchService::new(&mut fixture.store)
            .begin(request)
            .expect("begin launch");
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).execute(&prepared, &FailingExecutor),
            Err(AppError::External { .. })
        ));
        let (status, failure): (String, Option<String>) = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT status, failure FROM launch_intents WHERE id = ?1",
                        [prepared.intent_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("failed launch state");
        assert_eq!(status, "failed");
        assert_eq!(failure.as_deref(), Some("fake terminal rejected launch"));
    }

    #[test]
    fn explicit_adoption_requires_exact_process_evidence_and_blocks_duplicate_resume() {
        let mut fixture = fixture();
        let missing_process = HookIngestionMutation {
            process: None,
            ..hook(
                &fixture,
                Tool::Codex,
                "thread-adopted",
                &fixture.checkout_path,
                None,
            )
        };
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).adopt_hook(
                HierarchyOwner::WorkItem(fixture.work_item_id),
                fixture.checkout_id,
                &missing_process,
            ),
            Err(AppError::CallerIdentityUncorrelated)
        ));
        let mutation = hook(
            &fixture,
            Tool::Codex,
            "thread-adopted",
            &fixture.checkout_path,
            None,
        );
        let binding = SessionLaunchService::new(&mut fixture.store)
            .adopt_hook(
                HierarchyOwner::WorkItem(fixture.work_item_id),
                fixture.checkout_id,
                &mutation,
            )
            .expect("adopt exact session");
        assert!(binding.intent_id.is_none());
        let mut resume = request(&fixture, "resume-adopted");
        resume.mode = ManagedLaunchMode::Resume("thread-adopted".to_owned());
        resume.created_at = fixture.observed_at + time::Duration::seconds(2);
        resume.expires_at = fixture.observed_at + time::Duration::minutes(2);
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).begin(resume),
            Err(AppError::DuplicateConfirmed)
        ));
    }

    #[test]
    fn resume_binding_rejects_a_different_native_identity() {
        let mut fixture = fixture();
        let mut request = request(&fixture, "resume-mismatch");
        request.mode = ManagedLaunchMode::Resume("expected-thread".to_owned());
        add_resume_source(&fixture, &mut request, "expected-thread");
        let prepared = SessionLaunchService::new(&mut fixture.store)
            .begin(request)
            .expect("begin resume");
        let mutation = hook(
            &fixture,
            Tool::Codex,
            "different-thread",
            &fixture.checkout_path,
            Some(prepared.prepared.launch.launch_token().to_owned()),
        );
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).bind_hook(&mutation),
            Err(AppError::CallerIdentityMismatch)
        ));
    }

    #[test]
    fn ongoing_hooks_end_liveness_but_preserve_restore_membership() {
        let mut fixture = fixture();
        let launch_request = request(&fixture, "ongoing-hooks");
        let prepared = SessionLaunchService::new(&mut fixture.store)
            .begin(launch_request)
            .expect("begin launch");
        let start = hook(
            &fixture,
            Tool::Codex,
            "thread-ongoing",
            &fixture.checkout_path,
            Some(prepared.prepared.launch.launch_token().to_owned()),
        );
        let outcome = SessionLaunchService::new(&mut fixture.store)
            .ingest_hook(&start)
            .expect("bind start hook");
        assert!(matches!(outcome, super::HookIngestionOutcome::Bound { .. }));
        let idle = lifecycle_hook(&fixture, "thread-ongoing", "Stop", "2026-08-27T12:00:02Z");
        SessionLaunchService::new(&mut fixture.store)
            .ingest_hook(&idle)
            .expect("idle hook");
        let end = lifecycle_hook(
            &fixture,
            "thread-ongoing",
            "SessionEnd",
            "2026-08-27T12:00:03Z",
        );
        SessionLaunchService::new(&mut fixture.store)
            .ingest_hook(&end)
            .expect("end hook");
        let state = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT
                             (SELECT status FROM live_observations
                              ORDER BY observed_at DESC LIMIT 1),
                             (SELECT status FROM managed_sessions),
                             (SELECT active_until IS NULL FROM restore_memberships),
                             (SELECT removed_at IS NULL FROM restore_entries),
                             (SELECT COUNT(*) FROM live_observations)",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)?,
                                row.get::<_, i64>(4)?,
                            ))
                        },
                    )
                    .map_err(Into::into)
            })
            .expect("managed lifecycle state");
        assert_eq!(state, ("stopped".to_owned(), "stopped".to_owned(), 1, 1, 3));
    }

    #[test]
    fn exact_unmanaged_hook_allows_adoption_without_cwd_inference() {
        let mut fixture = fixture();
        let unmanaged = hook(
            &fixture,
            Tool::Codex,
            "thread-unmanaged",
            &fixture.checkout_path,
            None,
        );
        SessionLaunchService::new(&mut fixture.store)
            .ingest_hook(&unmanaged)
            .expect("observe unmanaged session");
        let conversation =
            ConversationRef::new(Tool::Codex, "thread-unmanaged").expect("conversation identity");
        let binding = SessionLaunchService::new(&mut fixture.store)
            .adopt_observed(
                HierarchyOwner::WorkItem(fixture.work_item_id),
                fixture.checkout_id,
                &conversation,
                &fixture.checkout_path,
                fixture.observed_at + time::Duration::seconds(2),
            )
            .expect("adopt observed session");
        assert_eq!(binding.native_id, "thread-unmanaged");

        let missing = ConversationRef::new(Tool::Codex, "thread-missing")
            .expect("missing conversation identity");
        assert!(matches!(
            SessionLaunchService::new(&mut fixture.store).adopt_observed(
                HierarchyOwner::WorkItem(fixture.work_item_id),
                fixture.checkout_id,
                &missing,
                &fixture.checkout_path,
                fixture.observed_at + time::Duration::seconds(2),
            ),
            Err(AppError::CallerIdentityUncorrelated)
        ));
    }
}
