use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use workboard_core::{
    CheckoutId, CheckoutPathId, ConversationId, FeatureId, HierarchyOwner, LaunchIntentId,
    LaunchProfile, ManagedSessionRole, RecoveryAttemptId, RepositoryId, TerminalLayoutId,
    TerminalTabId, Tool, WorkspaceId,
};

use crate::AppError;
use crate::git::{GitCli, GitRepositoryDiscovery, GitWorktreeResolver};
use crate::native_launch::validate_native_source;
use crate::native_sources::NativeSourceService;
use crate::storage::SqliteStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderAvailability {
    pub claude: bool,
    pub codex: bool,
}

impl ProviderAvailability {
    fn supports(self, tool: Tool) -> bool {
        match tool {
            Tool::Claude => self.claude,
            Tool::Codex => self.codex,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RecoveryDisposition {
    ReadyPresent,
    ReadyRecreate,
    AlreadyLive,
    Conflict { code: String, message: String },
}

impl RecoveryDisposition {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::ReadyPresent | Self::ReadyRecreate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub session_id: ConversationId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub profile: LaunchProfile,
    pub native_id: String,
    pub checkout_id: CheckoutId,
    pub repository_id: RepositoryId,
    pub repository_path: PathBuf,
    pub checkout_path: PathBuf,
    pub branch: Option<String>,
    pub feature_id: Option<FeatureId>,
    pub window_key: String,
    pub window_title: String,
    pub tab_title: String,
    pub disposition: RecoveryDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryPreview {
    pub workspace_id: WorkspaceId,
    pub since: Option<String>,
    pub observed_at: String,
    pub entries: Vec<RecoveryEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcomeStatus {
    Skipped,
    Launched,
    Bound,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRecordedOutcome {
    pub status: RecoveryOutcomeStatus,
    pub launch_intent_id: Option<LaunchIntentId>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRecoveryOutcome {
    pub attempt_id: RecoveryAttemptId,
    pub session_id: ConversationId,
    pub status: RecoveryOutcomeStatus,
    pub launch_intent_id: Option<LaunchIntentId>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub observed_at: OffsetDateTime,
}

impl RecoveryOutcomeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Launched => "launched",
            Self::Bound => "bound",
            Self::Conflict => "conflict",
            Self::Failed => "failed",
        }
    }
}

struct RecoveryRow {
    session_id: String,
    epic_id: Option<String>,
    feature_id: Option<String>,
    work_item_id: Option<String>,
    role: String,
    provider: String,
    native_id: String,
    checkout_id: String,
    repository_id: String,
    repository_path: String,
    checkout_path: String,
    branch: Option<String>,
    checkout_identity: String,
    owner_title: String,
    group_feature_id: Option<String>,
    feature_title: Option<String>,
    terminal_window: Option<String>,
    latest_live_status: Option<String>,
    latest_live_expires_at: Option<String>,
    latest_launch_status: Option<String>,
    latest_launch_expires_at: Option<String>,
    profile_schema: Option<u32>,
    profile_model: Option<String>,
    profile_effort: Option<String>,
    profile_source: Option<String>,
}

pub struct RecoveryService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> RecoveryService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn preview(
        &mut self,
        workspace_id: WorkspaceId,
        since: Option<OffsetDateTime>,
        observed_at: OffsetDateTime,
        availability: ProviderAvailability,
    ) -> Result<RecoveryPreview, AppError> {
        let since_text = since.map(timestamp);
        let rows = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT restore.session_id, restore.epic_id, restore.feature_id,
                        restore.work_item_id, managed.role, session.provider,
                        session.native_id, checkout.id, checkout.repository_id,
                        repository_path.path, checkout_path.path, checkout.branch,
                        checkout.git_worktree_identity,
                        COALESCE(item.title, feature.title, epic.title, session.native_id),
                        COALESCE(item.feature_id, restore.feature_id),
                        COALESCE(item_feature.title, feature.title),
                        managed_launch.terminal_window,
                        (SELECT live.status FROM live_observations live
                         WHERE live.session_id = restore.session_id
                         ORDER BY live.observed_at DESC LIMIT 1),
                        (SELECT live.expires_at FROM live_observations live
                         WHERE live.session_id = restore.session_id
                         ORDER BY live.observed_at DESC LIMIT 1),
                        (SELECT launch.status FROM launch_intents launch
                         WHERE launch.provider = session.provider
                           AND launch.expected_native_id = session.native_id
                         ORDER BY launch.created_at DESC LIMIT 1),
                        (SELECT launch.expires_at FROM launch_intents launch
                         WHERE launch.provider = session.provider
                           AND launch.expected_native_id = session.native_id
                         ORDER BY launch.created_at DESC LIMIT 1),
                        profile.schema_version, profile.model, profile.effort, profile.source
                 FROM restore_entries restore
                 JOIN native_sessions session ON session.id = restore.session_id
                 JOIN managed_sessions managed ON managed.id = (
                     SELECT candidate.id FROM managed_sessions candidate
                     WHERE candidate.session_id = restore.session_id
                     ORDER BY candidate.managed_from DESC LIMIT 1
                 )
                 LEFT JOIN launch_intents managed_launch
                   ON managed_launch.id = managed.launch_intent_id
                 LEFT JOIN launch_profiles profile ON profile.id = managed.profile_id
                 JOIN checkouts checkout ON checkout.id = managed.checkout_id
                 JOIN repositories repository ON repository.id = checkout.repository_id
                 JOIN repository_paths repository_path
                   ON repository_path.repository_id = repository.id
                  AND repository_path.observed_until IS NULL
                 JOIN checkout_paths checkout_path ON checkout_path.id = (
                     SELECT candidate.id FROM checkout_paths candidate
                     WHERE candidate.checkout_id = checkout.id
                     ORDER BY candidate.observed_from DESC LIMIT 1
                 )
                 LEFT JOIN epics epic ON epic.id = restore.epic_id
                 LEFT JOIN features feature ON feature.id = restore.feature_id
                 LEFT JOIN work_items item ON item.id = restore.work_item_id
                 LEFT JOIN features item_feature ON item_feature.id = item.feature_id
                 WHERE repository.workspace_id = ?1
                   AND restore.removed_at IS NULL
                   AND (item.status IS NULL OR item.status NOT IN ('done', 'cancelled'))
                   AND (COALESCE(item_feature.workflow_state, feature.workflow_state) IS NULL
                        OR COALESCE(item_feature.workflow_state, feature.workflow_state)
                           NOT IN ('completed', 'cancelled'))
                   AND (?2 IS NULL OR managed.managed_until IS NULL
                        OR managed.managed_until >= ?2
                        OR EXISTS (
                            SELECT 1 FROM live_observations recent
                            WHERE recent.session_id = restore.session_id
                              AND recent.observed_at >= ?2
                        ))
                 ORDER BY COALESCE(item_feature.title, feature.title, epic.title),
                          restore.session_id",
            )?;
            statement
                .query_map(params![workspace_id.to_string(), since_text], |row| {
                    Ok(RecoveryRow {
                        session_id: row.get(0)?,
                        epic_id: row.get(1)?,
                        feature_id: row.get(2)?,
                        work_item_id: row.get(3)?,
                        role: row.get(4)?,
                        provider: row.get(5)?,
                        native_id: row.get(6)?,
                        checkout_id: row.get(7)?,
                        repository_id: row.get(8)?,
                        repository_path: row.get(9)?,
                        checkout_path: row.get(10)?,
                        branch: row.get(11)?,
                        checkout_identity: row.get(12)?,
                        owner_title: row.get(13)?,
                        group_feature_id: row.get(14)?,
                        feature_title: row.get(15)?,
                        terminal_window: row.get(16)?,
                        latest_live_status: row.get(17)?,
                        latest_live_expires_at: row.get(18)?,
                        latest_launch_status: row.get(19)?,
                        latest_launch_expires_at: row.get(20)?,
                        profile_schema: row.get(21)?,
                        profile_model: row.get(22)?,
                        profile_effort: row.get(23)?,
                        profile_source: row.get(24)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.classify(row, observed_at, availability)?);
        }
        Ok(RecoveryPreview {
            workspace_id,
            since: since_text,
            observed_at: timestamp(observed_at),
            entries,
        })
    }

    pub fn remove_from_restore(
        &mut self,
        session_id: ConversationId,
        reason: &str,
        removed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        if reason.trim().is_empty() {
            return Err(AppError::EmptyReason);
        }
        let changed = self.store.write(|transaction| {
            Ok(transaction.execute(
                "UPDATE restore_entries
                 SET removed_at = ?2, remove_reason = ?3
                 WHERE session_id = ?1 AND removed_at IS NULL",
                params![session_id.to_string(), timestamp(removed_at), reason.trim()],
            )?)
        })?;
        if changed == 0 {
            return Err(AppError::ConversationNotFound);
        }
        Ok(())
    }

    pub fn begin_attempt(
        &mut self,
        preview: &RecoveryPreview,
        selected: &[ConversationId],
        idempotency_key: &str,
        requested_at: OffsetDateTime,
    ) -> Result<RecoveryAttemptId, AppError> {
        if idempotency_key.trim().is_empty() {
            return Err(AppError::EmptyIdempotencyKey);
        }
        let mut plan = preview.clone();
        plan.entries
            .retain(|entry| selected.contains(&entry.session_id));
        let plan_json = serde_json::to_string(&plan)?;
        self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT id, plan_json FROM recovery_attempts WHERE idempotency_key = ?1",
                    [idempotency_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let (attempt_id, created) = if let Some((id, existing_plan)) = existing {
                let existing_plan: RecoveryPreview = serde_json::from_str(&existing_plan)?;
                let mut existing_sessions = existing_plan
                    .entries
                    .iter()
                    .map(|entry| entry.session_id)
                    .collect::<Vec<_>>();
                let mut requested_sessions = plan
                    .entries
                    .iter()
                    .map(|entry| entry.session_id)
                    .collect::<Vec<_>>();
                existing_sessions.sort_unstable_by_key(ToString::to_string);
                requested_sessions.sort_unstable_by_key(ToString::to_string);
                if existing_plan.workspace_id != plan.workspace_id
                    || existing_sessions != requested_sessions
                {
                    return Err(AppError::IdempotencyConflict);
                }
                transaction.execute(
                    "UPDATE recovery_attempts
                     SET status = 'running', completed_at = NULL WHERE id = ?1",
                    [id.as_str()],
                )?;
                (parse_id(&id)?, false)
            } else {
                let id = RecoveryAttemptId::generate();
                transaction.execute(
                    "INSERT INTO recovery_attempts (
                         id, workspace_id, idempotency_key, requested_at, plan_json, status
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
                    params![
                        id.to_string(),
                        preview.workspace_id.to_string(),
                        idempotency_key,
                        timestamp(requested_at),
                        plan_json,
                    ],
                )?;
                (id, true)
            };
            if !created {
                return Ok(attempt_id);
            }
            let layout_id = TerminalLayoutId::generate();
            transaction.execute(
                "INSERT INTO terminal_layouts (id, workspace_id, captured_at)
                 VALUES (?1, ?2, ?3)",
                params![
                    layout_id.to_string(),
                    preview.workspace_id.to_string(),
                    timestamp(requested_at),
                ],
            )?;
            for (position, entry) in plan
                .entries
                .iter()
                .filter(|entry| entry.feature_id.is_some())
                .enumerate()
            {
                transaction.execute(
                    "INSERT INTO terminal_tabs (
                         id, layout_id, feature_id, session_id, position
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        TerminalTabId::generate().to_string(),
                        layout_id.to_string(),
                        entry.feature_id.expect("filtered feature").to_string(),
                        entry.session_id.to_string(),
                        position as i64,
                    ],
                )?;
            }
            Ok(attempt_id)
        })
    }

    pub fn record_outcome(&mut self, outcome: RecordRecoveryOutcome) -> Result<(), AppError> {
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO recovery_entry_outcomes (
                     attempt_id, session_id, status, launch_intent_id,
                     code, message, observed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(attempt_id, session_id) DO UPDATE SET
                     status = excluded.status,
                     launch_intent_id = excluded.launch_intent_id,
                     code = excluded.code,
                     message = excluded.message,
                     observed_at = excluded.observed_at",
                params![
                    outcome.attempt_id.to_string(),
                    outcome.session_id.to_string(),
                    outcome.status.as_str(),
                    outcome.launch_intent_id.map(|id| id.to_string()),
                    outcome.code,
                    outcome.message,
                    timestamp(outcome.observed_at),
                ],
            )?;
            Ok(())
        })
    }

    pub fn recorded_outcome(
        &self,
        attempt_id: RecoveryAttemptId,
        session_id: ConversationId,
    ) -> Result<Option<RecoveryRecordedOutcome>, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT status, launch_intent_id, code, message
                     FROM recovery_entry_outcomes
                     WHERE attempt_id = ?1 AND session_id = ?2",
                    params![attempt_id.to_string(), session_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()?
                .map(|(status, launch_intent_id, code, message)| {
                    Ok(RecoveryRecordedOutcome {
                        status: parse_outcome_status(&status)?,
                        launch_intent_id: launch_intent_id.as_deref().map(parse_id).transpose()?,
                        code,
                        message,
                    })
                })
                .transpose()
        })
    }

    pub fn finish_attempt(
        &mut self,
        attempt_id: RecoveryAttemptId,
        completed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            let failures: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM recovery_entry_outcomes
                 WHERE attempt_id = ?1 AND status IN ('conflict', 'failed')",
                [attempt_id.to_string()],
                |row| row.get(0),
            )?;
            transaction.execute(
                "UPDATE recovery_attempts SET status = ?2, completed_at = ?3 WHERE id = ?1",
                params![
                    attempt_id.to_string(),
                    if failures == 0 {
                        "completed"
                    } else {
                        "partial"
                    },
                    timestamp(completed_at),
                ],
            )?;
            Ok(())
        })
    }

    pub fn recreate_checkout(
        &mut self,
        entry: &RecoveryEntry,
        observed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        if !matches!(entry.disposition, RecoveryDisposition::ReadyRecreate) {
            return Err(AppError::External {
                code: "recovery_entry_not_recreatable".to_owned(),
                message: "the recovery entry is not approved for checkout recreation".to_owned(),
            });
        }
        let branch = entry.branch.as_deref().ok_or_else(|| AppError::External {
            code: "missing_branch".to_owned(),
            message: "the historical checkout has no branch".to_owned(),
        })?;
        let resolved = GitCli.restore_missing_worktree(
            &entry.repository_path,
            &entry.checkout_path,
            branch,
        )?;
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE checkouts
                 SET git_worktree_identity = ?2, branch = ?3, head = ?4,
                     availability = 'available'
                 WHERE id = ?1",
                params![
                    entry.checkout_id.to_string(),
                    path_text(&resolved.git_dir)?,
                    resolved.branch.as_deref().map(short_branch),
                    resolved.head_oid,
                ],
            )?;
            let current = transaction
                .query_row(
                    "SELECT id, path FROM checkout_paths
                     WHERE checkout_id = ?1 AND observed_until IS NULL",
                    [entry.checkout_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if current
                .as_ref()
                .is_none_or(|(_, path)| !paths_equal(Path::new(path), &entry.checkout_path))
            {
                if let Some((id, _)) = current {
                    transaction.execute(
                        "UPDATE checkout_paths SET observed_until = ?2 WHERE id = ?1",
                        params![id, timestamp(observed_at)],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(),
                        entry.checkout_id.to_string(),
                        path_text(&entry.checkout_path)?,
                        timestamp(observed_at),
                    ],
                )?;
            }
            Ok(())
        })
    }

    fn classify(
        &mut self,
        row: RecoveryRow,
        observed_at: OffsetDateTime,
        availability: ProviderAvailability,
    ) -> Result<RecoveryEntry, AppError> {
        let session_id = parse_id(&row.session_id)?;
        let tool = parse_tool(&row.provider)?;
        let role = parse_role(&row.role)?;
        let profile = match (
            row.profile_schema,
            row.profile_model,
            row.profile_effort,
            row.profile_source,
        ) {
            (Some(schema_version), Some(model), Some(effort), Some(source)) => LaunchProfile {
                schema_version,
                tool,
                model: Some(model),
                effort: Some(parse_wire(&effort)?),
                role,
                source: parse_wire(&source)?,
            },
            _ => LaunchProfile::legacy_unknown(tool, role),
        };
        let owner = parse_owner(row.epic_id, row.feature_id, row.work_item_id)?;
        let feature_id = row.group_feature_id.as_deref().map(parse_id).transpose()?;
        let checkout_path = PathBuf::from(&row.checkout_path);
        let repository_path = PathBuf::from(&row.repository_path);
        let live = matches!(row.latest_live_status.as_deref(), Some("active" | "idle"))
            && row
                .latest_live_expires_at
                .as_deref()
                .is_some_and(|expires| expires > timestamp(observed_at).as_str());
        let launch_in_flight = row.latest_launch_status.as_deref() == Some("launched")
            || (row.latest_launch_status.as_deref() == Some("pending")
                && row
                    .latest_launch_expires_at
                    .as_deref()
                    .is_some_and(|expires| expires > timestamp(observed_at).as_str()));
        let disposition = if live {
            RecoveryDisposition::AlreadyLive
        } else if launch_in_flight {
            conflict(
                "launch_reconciliation_required",
                "an earlier exact resume launch has not reached a terminal state",
            )
        } else if !availability.supports(tool) {
            conflict(
                "provider_incompatible",
                format!("the {} executable is unavailable", row.provider),
            )
        } else {
            let conversation = workboard_core::ConversationRef::new(tool, row.native_id.clone())
                .map_err(|error| AppError::Domain(error.to_string()))?;
            let context = NativeSourceService::new(self.store).resume_context(
                session_id,
                checkout_path.clone(),
                row.owner_title.clone(),
            );
            match context.and_then(|context| {
                validate_native_source(&conversation, &context).map(|()| context)
            }) {
                Err(error) => conflict("unresumable", error.to_string()),
                Ok(_) => classify_checkout(
                    &repository_path,
                    &checkout_path,
                    &row.checkout_identity,
                    row.branch.as_deref(),
                ),
            }
        };
        let (derived_window_key, window_title) = match (feature_id, owner) {
            (Some(id), _) => (
                format!("workboard-feature-{id}"),
                row.feature_title.unwrap_or_else(|| row.owner_title.clone()),
            ),
            (_, HierarchyOwner::Epic(id)) => {
                (format!("workboard-epic-{id}"), row.owner_title.clone())
            }
            _ => (
                format!("workboard-session-{session_id}"),
                row.owner_title.clone(),
            ),
        };
        let window_key = row.terminal_window.unwrap_or(derived_window_key);
        Ok(RecoveryEntry {
            session_id,
            owner,
            role,
            tool,
            profile,
            native_id: row.native_id,
            checkout_id: parse_id(&row.checkout_id)?,
            repository_id: parse_id(&row.repository_id)?,
            repository_path,
            checkout_path,
            branch: row.branch,
            feature_id,
            window_key,
            window_title,
            tab_title: format!("{} — {}", row.owner_title, tool_title(tool)),
            disposition,
        })
    }
}

fn classify_checkout(
    repository_path: &Path,
    checkout_path: &Path,
    checkout_identity: &str,
    branch: Option<&str>,
) -> RecoveryDisposition {
    if checkout_path.exists() {
        if !checkout_path.is_dir() {
            return conflict(
                "checkout_collision",
                format!("{} is not a directory", checkout_path.display()),
            );
        }
        return match GitCli.resolve(checkout_path) {
            Ok(resolved)
                if paths_equal(&resolved.git_dir, Path::new(checkout_identity))
                    && branch.is_none_or(|expected| {
                        resolved.branch.as_deref().map(short_branch) == Some(expected)
                    }) =>
            {
                RecoveryDisposition::ReadyPresent
            }
            Ok(_) if git_dirty(checkout_path) => conflict(
                "dirty_checkout",
                format!(
                    "{} contains a different dirty checkout",
                    checkout_path.display()
                ),
            ),
            Ok(_) | Err(_) => conflict(
                "checkout_collision",
                format!(
                    "{} is occupied by a different checkout",
                    checkout_path.display()
                ),
            ),
        };
    }
    if !repository_path.is_dir() {
        return conflict(
            "repository_unreachable",
            format!("{} is unavailable", repository_path.display()),
        );
    }
    if checkout_path.parent().is_none_or(|parent| !parent.is_dir()) {
        return conflict(
            "checkout_parent_unreachable",
            format!("the parent of {} is unavailable", checkout_path.display()),
        );
    }
    let Some(branch) = branch else {
        return conflict("missing_branch", "the historical checkout has no branch");
    };
    let repository = match GitCli.discover(repository_path) {
        Ok(repository) => repository,
        Err(error) => return conflict("repository_unreachable", error.to_string()),
    };
    let full_branch = format!("refs/heads/{branch}");
    if !repository
        .branches
        .iter()
        .any(|candidate| candidate.full_name == full_branch)
    {
        return conflict(
            "missing_branch",
            format!("branch {branch} is not available in the repository"),
        );
    }
    if repository.worktrees.iter().any(|worktree| {
        worktree.present
            && worktree.branch.as_deref() == Some(full_branch.as_str())
            && !paths_equal(&worktree.path, checkout_path)
    }) {
        return conflict(
            "checkout_collision",
            format!("branch {branch} is already checked out elsewhere"),
        );
    }
    RecoveryDisposition::ReadyRecreate
}

fn conflict(code: impl Into<String>, message: impl Into<String>) -> RecoveryDisposition {
    RecoveryDisposition::Conflict {
        code: code.into(),
        message: message.into(),
    }
}

fn git_dirty(path: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
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
        _ => Err(AppError::Domain("recovery owner is invalid".to_owned())),
    }
}

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(AppError::Domain(format!(
            "unknown native provider: {value}"
        ))),
    }
}

fn parse_role(value: &str) -> Result<ManagedSessionRole, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_wire<T>(value: &str) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_outcome_status(value: &str) -> Result<RecoveryOutcomeStatus, AppError> {
    match value {
        "skipped" => Ok(RecoveryOutcomeStatus::Skipped),
        "launched" => Ok(RecoveryOutcomeStatus::Launched),
        "bound" => Ok(RecoveryOutcomeStatus::Bound),
        "conflict" => Ok(RecoveryOutcomeStatus::Conflict),
        "failed" => Ok(RecoveryOutcomeStatus::Failed),
        _ => Err(AppError::Domain(format!(
            "unknown recovery outcome status: {value}"
        ))),
    }
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

fn timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .expect("RFC 3339 timestamps always format")
}

fn tool_title(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "Claude",
        Tool::Codex => "Codex",
    }
}

fn short_branch(value: &str) -> &str {
    value.strip_prefix("refs/heads/").unwrap_or(value)
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

#[cfg(windows)]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .eq_ignore_ascii_case(
            right
                .as_os_str()
                .to_string_lossy()
                .trim_start_matches(r"\\?\"),
        )
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use rusqlite::params;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;
    use workboard_core::{
        AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId, EpicId, FeatureId,
        ManagedSessionId, RepositoryId, RepositoryPathId, WorkspaceId,
    };

    use super::{
        ProviderAvailability, RecoveryDisposition, RecoveryOutcomeStatus, RecoveryService,
        classify_checkout,
    };
    use crate::AppError;
    use crate::git::{GitCli, GitWorktreeResolver};
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        workspace_id: WorkspaceId,
        feature_id: FeatureId,
        ready_id: ConversationId,
        live_id: ConversationId,
        missing_source_id: ConversationId,
        target: std::path::PathBuf,
        repository: std::path::PathBuf,
        git_identity: std::path::PathBuf,
        observed_at: OffsetDateTime,
    }

    #[test]
    fn preview_is_exhaustive_grouped_and_explicit_about_live_and_unresumable_entries() {
        let mut fixture = fixture();
        let preview = RecoveryService::new(&mut fixture.store)
            .preview(
                fixture.workspace_id,
                None,
                fixture.observed_at,
                ProviderAvailability {
                    claude: true,
                    codex: true,
                },
            )
            .expect("recovery preview");
        assert_eq!(preview.entries.len(), 3);
        assert!(preview.entries.iter().all(|entry| {
            entry.feature_id == Some(fixture.feature_id)
                && entry.window_key == format!("workboard-feature-{}", fixture.feature_id)
        }));
        assert!(matches!(
            disposition(&preview, fixture.ready_id),
            RecoveryDisposition::ReadyPresent
        ));
        assert!(matches!(
            disposition(&preview, fixture.live_id),
            RecoveryDisposition::AlreadyLive
        ));
        assert!(matches!(
            disposition(&preview, fixture.missing_source_id),
            RecoveryDisposition::Conflict { code, .. } if code == "unresumable"
        ));

        let unavailable = RecoveryService::new(&mut fixture.store)
            .preview(
                fixture.workspace_id,
                Some(fixture.observed_at - time::Duration::days(1)),
                fixture.observed_at,
                ProviderAvailability {
                    claude: true,
                    codex: false,
                },
            )
            .expect("provider preview");
        assert!(matches!(
            disposition(&unavailable, fixture.ready_id),
            RecoveryDisposition::Conflict { code, .. } if code == "provider_incompatible"
        ));
        assert!(matches!(
            disposition(&unavailable, fixture.live_id),
            RecoveryDisposition::AlreadyLive
        ));

        RecoveryService::new(&mut fixture.store)
            .remove_from_restore(fixture.ready_id, "finished elsewhere", fixture.observed_at)
            .expect("remove restore entry");
        let removed = RecoveryService::new(&mut fixture.store)
            .preview(
                fixture.workspace_id,
                None,
                fixture.observed_at,
                ProviderAvailability {
                    claude: true,
                    codex: true,
                },
            )
            .expect("preview after removal");
        assert_eq!(removed.entries.len(), 2);
        assert!(
            !removed
                .entries
                .iter()
                .any(|entry| entry.session_id == fixture.ready_id)
        );
    }

    #[test]
    fn missing_checkout_recreation_is_exact_and_conflicts_are_typed() {
        let mut fixture = fixture();
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target,
                fixture.git_identity.to_str().unwrap(),
                Some("feature/recovery"),
            ),
            RecoveryDisposition::ReadyPresent
        ));
        fs::write(fixture.target.join("dirty.txt"), "dirty").expect("dirty fixture");
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target,
                "different-checkout",
                Some("feature/recovery"),
            ),
            RecoveryDisposition::Conflict { code, .. } if code == "dirty_checkout"
        ));
        fs::remove_file(fixture.target.join("dirty.txt")).expect("remove dirty fixture");
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target,
                "different-checkout",
                Some("feature/recovery"),
            ),
            RecoveryDisposition::Conflict { code, .. } if code == "checkout_collision"
        ));
        fs::remove_dir_all(&fixture.target).expect("remove fixture worktree");
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target,
                fixture.git_identity.to_str().unwrap(),
                Some("feature/recovery"),
            ),
            RecoveryDisposition::ReadyRecreate
        ));
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target,
                fixture.git_identity.to_str().unwrap(),
                Some("feature/missing"),
            ),
            RecoveryDisposition::Conflict { code, .. } if code == "missing_branch"
        ));
        assert!(matches!(
            classify_checkout(
                &fixture.repository.join("missing"),
                &fixture.target,
                fixture.git_identity.to_str().unwrap(),
                Some("feature/recovery"),
            ),
            RecoveryDisposition::Conflict { code, .. } if code == "repository_unreachable"
        ));
        assert!(matches!(
            classify_checkout(
                &fixture.repository,
                &fixture.target.join("missing-parent").join("checkout"),
                fixture.git_identity.to_str().unwrap(),
                Some("feature/recovery"),
            ),
            RecoveryDisposition::Conflict { code, .. } if code == "checkout_parent_unreachable"
        ));
        let preview = RecoveryService::new(&mut fixture.store)
            .preview(
                fixture.workspace_id,
                None,
                fixture.observed_at,
                ProviderAvailability {
                    claude: true,
                    codex: true,
                },
            )
            .expect("missing checkout preview");
        let entry = preview
            .entries
            .iter()
            .find(|entry| entry.session_id == fixture.ready_id)
            .expect("ready recovery entry")
            .clone();
        assert!(matches!(
            entry.disposition,
            RecoveryDisposition::ReadyRecreate
        ));
        RecoveryService::new(&mut fixture.store)
            .recreate_checkout(&entry, fixture.observed_at)
            .expect("restore exact missing worktree");
        assert_eq!(
            fixture.target.canonicalize().unwrap(),
            GitCli.resolve(&fixture.target).unwrap().path
        );
    }

    #[test]
    fn recovery_attempt_retry_reuses_identity_layout_and_outcomes() {
        let mut fixture = fixture();
        let preview = RecoveryService::new(&mut fixture.store)
            .preview(
                fixture.workspace_id,
                None,
                fixture.observed_at,
                ProviderAvailability {
                    claude: true,
                    codex: true,
                },
            )
            .expect("preview");
        let selected = preview
            .entries
            .iter()
            .map(|entry| entry.session_id)
            .collect::<Vec<_>>();
        let attempt = RecoveryService::new(&mut fixture.store)
            .begin_attempt(&preview, &selected, "restart-attempt", fixture.observed_at)
            .expect("begin attempt");
        RecoveryService::new(&mut fixture.store)
            .record_outcome(super::RecordRecoveryOutcome {
                attempt_id: attempt,
                session_id: fixture.ready_id,
                status: RecoveryOutcomeStatus::Bound,
                launch_intent_id: None,
                code: None,
                message: Some("bound".to_owned()),
                observed_at: fixture.observed_at,
            })
            .expect("record bound");
        RecoveryService::new(&mut fixture.store)
            .record_outcome(super::RecordRecoveryOutcome {
                attempt_id: attempt,
                session_id: fixture.missing_source_id,
                status: RecoveryOutcomeStatus::Conflict,
                launch_intent_id: None,
                code: Some("unresumable".to_owned()),
                message: Some("source missing".to_owned()),
                observed_at: fixture.observed_at,
            })
            .expect("record conflict");
        RecoveryService::new(&mut fixture.store)
            .finish_attempt(attempt, fixture.observed_at)
            .expect("finish partial attempt");
        let mut later_preview = preview.clone();
        later_preview.observed_at = "2026-08-28T12:05:00Z".to_owned();
        let retried = RecoveryService::new(&mut fixture.store)
            .begin_attempt(
                &later_preview,
                &selected,
                "restart-attempt",
                fixture.observed_at + time::Duration::minutes(5),
            )
            .expect("retry attempt");
        assert_eq!(retried, attempt);
        let (attempts, layouts, tabs, status): (i64, i64, i64, String) = fixture
            .store
            .read(|connection| {
                Ok((
                    connection.query_row("SELECT COUNT(*) FROM recovery_attempts", [], |row| {
                        row.get(0)
                    })?,
                    connection.query_row("SELECT COUNT(*) FROM terminal_layouts", [], |row| {
                        row.get(0)
                    })?,
                    connection
                        .query_row("SELECT COUNT(*) FROM terminal_tabs", [], |row| row.get(0))?,
                    connection.query_row(
                        "SELECT status FROM recovery_attempts WHERE id = ?1",
                        [attempt.to_string()],
                        |row| row.get(0),
                    )?,
                ))
            })
            .expect("attempt counts");
        assert_eq!((attempts, layouts, tabs), (1, 1, 3));
        assert_eq!(status, "running");
        let error = RecoveryService::new(&mut fixture.store)
            .begin_attempt(
                &later_preview,
                &[fixture.ready_id],
                "restart-attempt",
                fixture.observed_at,
            )
            .unwrap_err();
        assert!(matches!(error, AppError::IdempotencyConflict));
    }

    #[test]
    fn stopped_liveness_and_restore_intent_survive_database_reopen() {
        let fixture = fixture();
        let Fixture {
            _directory,
            mut store,
            workspace_id,
            live_id,
            observed_at,
            ..
        } = fixture;
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO live_observations (
                         id, session_id, source, status, observed_at, expires_at
                     ) VALUES (?1, ?2, 'hook', 'stopped', ?3, ?4)",
                    params![
                        workboard_core::LiveObservationId::generate().to_string(),
                        live_id.to_string(),
                        super::timestamp(observed_at + time::Duration::minutes(1)),
                        super::timestamp(observed_at + time::Duration::minutes(2)),
                    ],
                )?;
                Ok(())
            })
            .expect("record terminal closure");
        let database = store.path().to_path_buf();
        drop(store);
        let mut reopened = SqliteStore::open(database).expect("reopen after restart");
        let preview = RecoveryService::new(&mut reopened)
            .preview(
                workspace_id,
                Some(observed_at - time::Duration::days(1)),
                observed_at + time::Duration::minutes(3),
                ProviderAvailability {
                    claude: true,
                    codex: true,
                },
            )
            .expect("preview after restart");
        assert!(matches!(
            disposition(&preview, live_id),
            RecoveryDisposition::ReadyPresent
        ));
        drop(_directory);
    }

    fn disposition(preview: &super::RecoveryPreview, id: ConversationId) -> &RecoveryDisposition {
        &preview
            .entries
            .iter()
            .find(|entry| entry.session_id == id)
            .expect("session entry")
            .disposition
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let repository = directory.path().join("repository");
        let planning = directory.path().join("planning");
        let target_parent = directory.path().join("worktrees");
        let target = target_parent.join("feature-recovery");
        fs::create_dir(&repository).expect("repository");
        fs::create_dir(&planning).expect("planning");
        fs::create_dir(&target_parent).expect("worktree parent");
        git(&repository, &["init", "-b", "main"]);
        git(
            &repository,
            &["config", "user.email", "workboard@example.invalid"],
        );
        git(&repository, &["config", "user.name", "Agent Workboard"]);
        fs::write(repository.join("README.md"), "fixture\n").expect("fixture file");
        git(&repository, &["add", "README.md"]);
        git(&repository, &["commit", "-m", "Initial fixture"]);
        git_path(
            &repository,
            &["worktree", "add", "-b", "feature/recovery"],
            &target,
            &["main"],
        );
        let resolved = GitCli.resolve(&target).expect("resolve fixture worktree");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let checkout_id = CheckoutId::generate();
        let ready_id = ConversationId::generate();
        let live_id = ConversationId::generate();
        let missing_source_id = ConversationId::generate();
        let observed_at = at("2026-08-28T12:00:00Z");
        let at_text = super::timestamp(observed_at);
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
                     VALUES (?1, 'fixture', 'Fixture', ?2, ?3)",
                    params![workspace_id.to_string(), planning_repository_id.to_string(), at_text],
                )?;
                for (id, slug, title, common, planning_store) in [
                    (
                        planning_repository_id,
                        "planning",
                        "Planning",
                        planning.join(".git"),
                        1,
                    ),
                    (
                        repository_id,
                        "code",
                        "Code",
                        resolved.common_dir.clone(),
                        0,
                    ),
                ] {
                    transaction.execute(
                        "INSERT INTO repositories (
                             id, workspace_id, slug, title, git_common_directory,
                             default_branch, is_planning_store, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'main', ?6, ?7)",
                        params![
                            id.to_string(), workspace_id.to_string(), slug, title,
                            common.to_string_lossy(), planning_store, at_text,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO repository_paths (
                             id, repository_id, path, observed_from, observed_until
                         ) VALUES (?1, ?2, ?3, ?4, NULL)",
                        params![
                            RepositoryPathId::generate().to_string(), id.to_string(),
                            if planning_store == 1 { planning.to_string_lossy() } else { repository.to_string_lossy() },
                            at_text,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'recovery', 'Recovery', ?3)",
                    params![epic_id.to_string(), workspace_id.to_string(), at_text],
                )?;
                transaction.execute(
                    "INSERT INTO features (
                         id, epic_id, slug, title, workflow_state, created_at
                     ) VALUES (?1, ?2, 'restart', 'Restart recovery', 'planned', ?3)",
                    params![feature_id.to_string(), epic_id.to_string(), at_text],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, head,
                         availability, created_at
                     ) VALUES (?1, ?2, ?3, 'feature/recovery', ?4, 'available', ?5)",
                    params![
                        checkout_id.to_string(), repository_id.to_string(),
                        resolved.git_dir.to_string_lossy(), resolved.head_oid, at_text,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(), checkout_id.to_string(),
                        target.to_string_lossy(), at_text,
                    ],
                )?;
                for (session_id, native_id, live, has_source) in [
                    (ready_id, "ready-session", false, true),
                    (live_id, "live-session", true, true),
                    (missing_source_id, "missing-source", false, false),
                ] {
                    transaction.execute(
                        "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                         VALUES (?1, 'codex', ?2, ?3)",
                        params![session_id.to_string(), native_id, at_text],
                    )?;
                    transaction.execute(
                        "INSERT INTO native_session_associations (
                             id, session_id, feature_id, role, associated_from
                         ) VALUES (?1, ?2, ?3, 'feature_planning', ?4)",
                        params![
                            AssociationIntervalId::generate().to_string(), session_id.to_string(),
                            feature_id.to_string(), at_text,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO managed_sessions (
                             id, session_id, checkout_id, role, status, managed_from
                         ) VALUES (?1, ?2, ?3, 'feature_planning', 'bound', ?4)",
                        params![
                            ManagedSessionId::generate().to_string(), session_id.to_string(),
                            checkout_id.to_string(), at_text,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO restore_entries (session_id, feature_id, added_at)
                         VALUES (?1, ?2, ?3)",
                        params![session_id.to_string(), feature_id.to_string(), at_text],
                    )?;
                    if has_source {
                        let source = directory.path().join(format!("{native_id}.jsonl"));
                        fs::write(
                            &source,
                            format!(
                                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{native_id}\",\"cwd\":\"{}\"}}}}\n",
                                target.to_string_lossy().replace('\\', "\\\\")
                            ),
                        )
                        .expect("native source");
                        let snapshot = workboard_native::NativeConversation::new(
                            native_id,
                            workboard_native::ConversationKind::TopLevel,
                        );
                        transaction.execute(
                            "INSERT INTO native_session_sources (
                                 session_id, path, adapter_version, snapshot_json, missing, observed_at
                             ) VALUES (?1, ?2, 'codex-v1', ?3, 0, ?4)",
                            params![
                                session_id.to_string(), source.to_string_lossy(),
                                serde_json::to_string(&snapshot)?, at_text,
                            ],
                        )?;
                    }
                    if live {
                        transaction.execute(
                            "INSERT INTO live_observations (
                                 id, session_id, source, status, observed_at, expires_at
                             ) VALUES (?1, ?2, 'hook', 'active', ?3, ?4)",
                            params![
                                workboard_core::LiveObservationId::generate().to_string(),
                                session_id.to_string(), at_text,
                                super::timestamp(observed_at + time::Duration::minutes(5)),
                            ],
                        )?;
                    }
                }
                Ok(())
            })
            .expect("seed recovery fixture");
        Fixture {
            _directory: directory,
            store,
            workspace_id,
            feature_id,
            ready_id,
            live_id,
            missing_source_id,
            target,
            repository,
            git_identity: resolved.git_dir,
            observed_at,
        }
    }

    fn at(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).expect("timestamp")
    }

    fn git(repository: &std::path::Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(arguments)
            .status()
            .expect("run Git");
        assert!(status.success(), "Git failed: {arguments:?}");
    }

    fn git_path(
        repository: &std::path::Path,
        before: &[&str],
        path: &std::path::Path,
        after: &[&str],
    ) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(before)
            .arg(path)
            .args(after)
            .status()
            .expect("run Git with path");
        assert!(status.success(), "Git failed: {before:?}");
    }
}
