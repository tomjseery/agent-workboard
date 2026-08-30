use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_core::{
    CheckoutId, ConversationId, FeatureId, LaunchProfile, RepositoryId, Tool, WorkItemId,
};

use crate::AppError;
use crate::checkout::{CheckoutService, PrepareWorkItemCheckout};
use crate::storage::SqliteStore;
use crate::work_projection::WorkProjectionService;

const CONFIRMATION_TTL_MINUTES: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewManagedLaunchBatch {
    pub feature_id: FeatureId,
    pub work_item_ids: Vec<WorkItemId>,
    pub tool: Tool,
    pub profile: LaunchProfile,
    pub idempotency_key: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLaunchBatchChild {
    pub position: u32,
    pub work_item_id: WorkItemId,
    pub repository_id: RepositoryId,
    pub dependency_layer: u32,
    pub tool: Tool,
    pub profile: LaunchProfile,
    pub checkout_id: Option<CheckoutId>,
    pub launch_intent_id: Option<workboard_core::LaunchIntentId>,
    pub session_id: Option<ConversationId>,
    pub status: String,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLaunchBatchPreview {
    pub batch_id: String,
    pub feature_id: FeatureId,
    pub confirmation_token: String,
    pub confirmation_expires_at: OffsetDateTime,
    pub children: Vec<ManagedLaunchBatchChild>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmManagedLaunchBatch {
    pub batch_id: String,
    pub confirmation_token: String,
    pub confirmed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLaunchBatchReservation {
    pub batch_id: String,
    pub feature_id: FeatureId,
    pub status: String,
    pub children: Vec<ManagedLaunchBatchChild>,
}

pub struct BatchLaunchService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> BatchLaunchService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn preview(
        &mut self,
        request: PreviewManagedLaunchBatch,
    ) -> Result<ManagedLaunchBatchPreview, AppError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(AppError::EmptyIdempotencyKey);
        }
        request
            .profile
            .validate_for_launch(request.tool, request.profile.role)
            .map_err(|error| AppError::Domain(error.to_string()))?;
        let existing = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT 1 FROM managed_launch_batches WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(Into::into)
        })?;
        if existing.is_some() {
            return Err(AppError::DuplicateConfirmed);
        }
        let all_ready = request.work_item_ids.is_empty();
        let feature_work_items = self.feature_work_items(request.feature_id)?;
        let selected = request
            .work_item_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let work_item_ids = if all_ready {
            feature_work_items
        } else {
            let work_item_ids = feature_work_items
                .into_iter()
                .filter(|work_item_id| selected.contains(work_item_id))
                .collect::<Vec<_>>();
            if work_item_ids.len() != selected.len() {
                return Err(AppError::External {
                    code: "batch_selection_mismatch".to_owned(),
                    message: "every selected Work item must belong to the requested Feature"
                        .to_owned(),
                });
            }
            work_item_ids
        };
        if work_item_ids.is_empty() {
            return Err(AppError::External {
                code: "batch_selection_empty".to_owned(),
                message: "the batch contains no Work items".to_owned(),
            });
        }
        let mut children = Vec::new();
        for work_item_id in work_item_ids {
            let projection = WorkProjectionService::new(self.store).project(work_item_id)?;
            if projection.work_item.feature_id != request.feature_id {
                return Err(AppError::External {
                    code: "batch_selection_mismatch".to_owned(),
                    message: "every selected Work item must belong to the requested Feature"
                        .to_owned(),
                });
            }
            if !projection.readiness.ready {
                if all_ready {
                    continue;
                }
                return Err(AppError::External {
                    code: "batch_work_item_blocked".to_owned(),
                    message: format!("Work item {work_item_id} is not ready"),
                });
            }
            if projection
                .sessions
                .iter()
                .any(|session| session.primary_writer)
            {
                return Err(AppError::External {
                    code: "batch_work_item_active".to_owned(),
                    message: format!("Work item {work_item_id} already has a primary writer"),
                });
            }
            for repository_id in projection.work_item.repository_ids {
                children.push(ManagedLaunchBatchChild {
                    position: 0,
                    work_item_id,
                    repository_id,
                    dependency_layer: projection.readiness.layer,
                    tool: request.tool,
                    profile: request.profile.clone(),
                    checkout_id: None,
                    launch_intent_id: None,
                    session_id: None,
                    status: "selected".to_owned(),
                    failure: None,
                });
            }
        }
        if children.is_empty() {
            return Err(AppError::External {
                code: "batch_selection_empty".to_owned(),
                message: "the batch contains no ready Work items".to_owned(),
            });
        }
        children.sort_by_key(|child| child.dependency_layer);
        for (position, child) in children.iter_mut().enumerate() {
            child.position =
                u32::try_from(position).map_err(|error| AppError::Domain(error.to_string()))?;
        }
        let batch_id = Uuid::new_v4().to_string();
        let confirmation_token = Uuid::new_v4().to_string();
        let confirmation_expires_at =
            request.created_at + time::Duration::minutes(CONFIRMATION_TTL_MINUTES);
        let selection_hash = selection_hash(request.feature_id, &children)?;
        let created_at = timestamp(request.created_at);
        let expires_at = timestamp(confirmation_expires_at);
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO managed_launch_batches (
                     id, feature_id, idempotency_key, selection_hash,
                     confirmation_token_hash, status, created_at, confirmation_expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'previewed', ?6, ?7)",
                params![
                    batch_id,
                    request.feature_id.to_string(),
                    request.idempotency_key,
                    selection_hash,
                    token_hash(&confirmation_token),
                    created_at,
                    expires_at,
                ],
            )?;
            for child in &children {
                transaction.execute(
                    "INSERT INTO managed_launch_batch_children (
                         batch_id, position, work_item_id, repository_id, dependency_layer,
                         provider, profile_json, status, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'selected', ?8)",
                    params![
                        batch_id,
                        i64::from(child.position),
                        child.work_item_id.to_string(),
                        child.repository_id.to_string(),
                        i64::from(child.dependency_layer),
                        tool_name(child.tool),
                        serde_json::to_string(&child.profile)?,
                        created_at,
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(ManagedLaunchBatchPreview {
            batch_id,
            feature_id: request.feature_id,
            confirmation_token,
            confirmation_expires_at,
            children,
        })
    }

    pub fn reserve(
        &mut self,
        request: ConfirmManagedLaunchBatch,
    ) -> Result<ManagedLaunchBatchReservation, AppError> {
        self.reserve_with(request, |store, batch_id, child, confirmed_at| {
            CheckoutService::new(store)
                .prepare_work_item(PrepareWorkItemCheckout {
                    work_item_id: child.work_item_id,
                    repository_id: child.repository_id,
                    idempotency_key: format!(
                        "batch:{batch_id}:{}:{}:checkout",
                        child.work_item_id, child.repository_id
                    ),
                    observed_at: confirmed_at,
                })
                .map(|readiness| readiness.checkout_id)
        })
    }

    fn reserve_with(
        &mut self,
        request: ConfirmManagedLaunchBatch,
        mut preflight: impl FnMut(
            &mut SqliteStore,
            &str,
            &ManagedLaunchBatchChild,
            OffsetDateTime,
        ) -> Result<CheckoutId, AppError>,
    ) -> Result<ManagedLaunchBatchReservation, AppError> {
        self.validate_confirmation(&request)?;
        let preview = self.read_reservation(&request.batch_id)?;
        let mut prepared = Vec::with_capacity(preview.children.len());
        for child in &preview.children {
            match preflight(self.store, &request.batch_id, child, request.confirmed_at) {
                Ok(checkout_id) => prepared.push((child.clone(), checkout_id)),
                Err(error) => {
                    self.fail_preflight(
                        &request.batch_id,
                        child.position,
                        &error,
                        request.confirmed_at,
                    )?;
                    return Err(error);
                }
            }
        }
        let confirmed_at = timestamp(request.confirmed_at);
        self.store.write(|transaction| {
            for (child, checkout_id) in &prepared {
                let valid = transaction.query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM work_items item
                         JOIN checkout_readiness readiness
                           ON readiness.owner_kind = 'work_item'
                          AND readiness.owner_id = item.id
                          AND readiness.repository_id = ?3
                          AND readiness.checkout_id = ?4
                          AND readiness.purpose = 'work_item_write'
                          AND readiness.availability = 'available'
                         WHERE item.id = ?1 AND item.feature_id = ?2
                           AND item.status IN ('ready', 'in_progress', 'review')
                           AND NOT EXISTS (
                               SELECT 1 FROM work_item_dependencies edge
                               JOIN work_items dependency ON dependency.id = edge.dependency_work_item_id
                               WHERE edge.work_item_id = item.id
                                 AND dependency.status NOT IN ('review', 'done')
                           )
                           AND NOT EXISTS (
                               SELECT 1 FROM native_session_associations association
                               JOIN managed_sessions managed ON managed.session_id = association.session_id
                               JOIN checkout_readiness active ON active.checkout_id = managed.checkout_id
                               WHERE association.work_item_id = item.id
                                 AND association.associated_until IS NULL
                                 AND managed.managed_until IS NULL
                                 AND managed.status IN ('bound', 'adopted')
                                 AND active.purpose = 'work_item_write'
                           )
                     )",
                    params![
                        child.work_item_id.to_string(),
                        preview.feature_id.to_string(),
                        child.repository_id.to_string(),
                        checkout_id.to_string(),
                    ],
                    |row| row.get::<_, i64>(0),
                )?;
                if valid == 0 {
                    return Err(AppError::External {
                        code: "batch_revalidation_failed".to_owned(),
                        message: format!(
                            "Work item {} changed after batch preview",
                            child.work_item_id
                        ),
                    });
                }
            }
            let updated = transaction.execute(
                "UPDATE managed_launch_batches
                 SET status = 'reserved', confirmed_at = ?3
                 WHERE id = ?1 AND confirmation_token_hash = ?2 AND status = 'previewed'",
                params![
                    request.batch_id,
                    token_hash(&request.confirmation_token),
                    confirmed_at,
                ],
            )?;
            if updated != 1 {
                return Err(AppError::LaunchLeaseLost);
            }
            for (child, checkout_id) in &prepared {
                transaction.execute(
                    "UPDATE managed_launch_batch_children
                     SET checkout_id = ?3, status = 'reserved', updated_at = ?4
                     WHERE batch_id = ?1 AND position = ?2 AND status = 'selected'",
                    params![
                        request.batch_id,
                        i64::from(child.position),
                        checkout_id.to_string(),
                        confirmed_at,
                    ],
                )?;
            }
            Ok(())
        })?;
        self.read_reservation(&request.batch_id)
    }

    pub fn mark_launched(
        &mut self,
        batch_id: &str,
        position: u32,
        intent_id: workboard_core::LaunchIntentId,
        updated_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.update_child(
            batch_id,
            position,
            "launched",
            Some(intent_id),
            None,
            None,
            updated_at,
        )
    }

    pub fn mark_bound(
        &mut self,
        batch_id: &str,
        position: u32,
        session_id: ConversationId,
        updated_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.update_child(
            batch_id,
            position,
            "bound",
            None,
            Some(session_id),
            None,
            updated_at,
        )
    }

    pub fn mark_failed(
        &mut self,
        batch_id: &str,
        position: u32,
        failure: &str,
        updated_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.update_child(
            batch_id,
            position,
            "failed",
            None,
            None,
            Some(failure),
            updated_at,
        )
    }

    pub fn reservation(&self, batch_id: &str) -> Result<ManagedLaunchBatchReservation, AppError> {
        self.read_reservation(batch_id)
    }

    pub fn reconcile(
        &mut self,
        batch_id: &str,
        reconciled_at: OffsetDateTime,
    ) -> Result<ManagedLaunchBatchReservation, AppError> {
        let candidates = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT child.position, child.launch_intent_id, intent.status, managed.session_id
                 FROM managed_launch_batch_children child
                 JOIN launch_intents intent ON intent.id = child.launch_intent_id
                 LEFT JOIN managed_sessions managed ON managed.launch_intent_id = intent.id
                 WHERE child.batch_id = ?1
                   AND child.status IN ('launched', 'failed')
                 ORDER BY child.position",
            )?;
            statement
                .query_map([batch_id], |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        for (position, _intent_id, status, session_id) in candidates {
            if let Some(session_id) = session_id {
                self.mark_bound(batch_id, position, parse_id(&session_id)?, reconciled_at)?;
            } else if matches!(status.as_str(), "failed" | "cancelled" | "expired") {
                self.mark_failed(
                    batch_id,
                    position,
                    &format!("launch intent is {status}"),
                    reconciled_at,
                )?;
            }
        }
        self.read_reservation(batch_id)
    }

    fn feature_work_items(&self, feature_id: FeatureId) -> Result<Vec<WorkItemId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id FROM work_items WHERE feature_id = ?1 ORDER BY proposal_order, id",
            )?;
            statement
                .query_map([feature_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| parse_id(&row?))
                .collect()
        })
    }

    fn validate_confirmation(&self, request: &ConfirmManagedLaunchBatch) -> Result<(), AppError> {
        let valid = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM managed_launch_batches
                         WHERE id = ?1 AND confirmation_token_hash = ?2
                           AND status = 'previewed' AND confirmation_expires_at > ?3
                     )",
                    params![
                        request.batch_id,
                        token_hash(&request.confirmation_token),
                        timestamp(request.confirmed_at),
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(Into::into)
        })?;
        if valid == 1 {
            Ok(())
        } else {
            Err(AppError::External {
                code: "batch_confirmation_invalid".to_owned(),
                message: "the batch confirmation is missing, expired, consumed, or changed"
                    .to_owned(),
            })
        }
    }

    fn fail_preflight(
        &mut self,
        batch_id: &str,
        position: u32,
        error: &AppError,
        failed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        let failed_at = timestamp(failed_at);
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE managed_launch_batches SET status = 'failed', completed_at = ?2
                 WHERE id = ?1 AND status = 'previewed'",
                params![batch_id, failed_at],
            )?;
            transaction.execute(
                "UPDATE managed_launch_batch_children
                 SET status = CASE WHEN position = ?2 THEN 'failed' ELSE 'skipped' END,
                     failure = CASE WHEN position = ?2 THEN ?3 ELSE 'batch preflight failed' END,
                     updated_at = ?4
                 WHERE batch_id = ?1 AND status = 'selected'",
                params![batch_id, i64::from(position), error.to_string(), failed_at],
            )?;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn update_child(
        &mut self,
        batch_id: &str,
        position: u32,
        status: &str,
        intent_id: Option<workboard_core::LaunchIntentId>,
        session_id: Option<ConversationId>,
        failure: Option<&str>,
        updated_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        let updated_at = timestamp(updated_at);
        self.store.write(|transaction| {
            let updated = transaction.execute(
                "UPDATE managed_launch_batch_children
                 SET status = ?3,
                     launch_intent_id = COALESCE(?4, launch_intent_id),
                     session_id = COALESCE(?5, session_id), failure = ?6, updated_at = ?7
                 WHERE batch_id = ?1 AND position = ?2",
                params![
                    batch_id,
                    i64::from(position),
                    status,
                    intent_id.map(|id| id.to_string()),
                    session_id.map(|id| id.to_string()),
                    failure,
                    updated_at,
                ],
            )?;
            if updated != 1 {
                return Err(AppError::LaunchLeaseLost);
            }
            let (pending, bound, failed) = transaction.query_row(
                "SELECT
                     SUM(status IN ('selected', 'reserved', 'launched')),
                     SUM(status = 'bound'), SUM(status IN ('failed', 'skipped'))
                 FROM managed_launch_batch_children WHERE batch_id = ?1",
                [batch_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?;
            let batch_status = if pending > 0 {
                "launching"
            } else if failed == 0 {
                "completed"
            } else if bound == 0 {
                "failed"
            } else {
                "partial"
            };
            transaction.execute(
                "UPDATE managed_launch_batches
                 SET status = ?2, completed_at = CASE
                     WHEN ?2 IN ('completed', 'partial', 'failed') THEN ?3 ELSE NULL END
                 WHERE id = ?1",
                params![batch_id, batch_status, updated_at],
            )?;
            Ok(())
        })
    }

    fn read_reservation(&self, batch_id: &str) -> Result<ManagedLaunchBatchReservation, AppError> {
        self.store.read(|connection| {
            let (feature_id, status) = connection
                .query_row(
                    "SELECT feature_id, status FROM managed_launch_batches WHERE id = ?1",
                    [batch_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::External {
                    code: "batch_not_found".to_owned(),
                    message: "the managed launch batch does not exist".to_owned(),
                })?;
            let mut statement = connection.prepare(
                "SELECT position, work_item_id, repository_id, dependency_layer,
                        provider, profile_json, checkout_id, launch_intent_id, session_id,
                        status, failure
                 FROM managed_launch_batch_children WHERE batch_id = ?1 ORDER BY position",
            )?;
            let children = statement
                .query_map([batch_id], |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?,
                    ))
                })?
                .map(|row| {
                    let (
                        position,
                        work_item_id,
                        repository_id,
                        dependency_layer,
                        tool,
                        profile,
                        checkout_id,
                        launch_intent_id,
                        session_id,
                        status,
                        failure,
                    ) = row?;
                    Ok(ManagedLaunchBatchChild {
                        position,
                        work_item_id: parse_id(&work_item_id)?,
                        repository_id: parse_id(&repository_id)?,
                        dependency_layer,
                        tool: parse_tool(&tool)?,
                        profile: serde_json::from_str(&profile)?,
                        checkout_id: checkout_id.as_deref().map(parse_id).transpose()?,
                        launch_intent_id: launch_intent_id.as_deref().map(parse_id).transpose()?,
                        session_id: session_id.as_deref().map(parse_id).transpose()?,
                        status,
                        failure,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(ManagedLaunchBatchReservation {
                batch_id: batch_id.to_owned(),
                feature_id: parse_id(&feature_id)?,
                status,
                children,
            })
        })
    }
}

fn selection_hash(
    feature_id: FeatureId,
    children: &[ManagedLaunchBatchChild],
) -> Result<String, AppError> {
    let value = serde_json::to_vec(&(feature_id, children))?;
    Ok(format!("{:x}", Sha256::digest(value)))
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
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
        _ => Err(AppError::Domain(
            "managed batch provider is invalid".to_owned(),
        )),
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

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        CheckoutId, CheckoutPathId, DocumentId, EpicId, FeatureId, LaunchProfile,
        ManagedSessionRole, RepositoryId, Tool, WorkItemId, WorkspaceId,
    };

    use super::{BatchLaunchService, ConfirmManagedLaunchBatch, PreviewManagedLaunchBatch};
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        feature_id: FeatureId,
        root_id: WorkItemId,
        parallel_id: WorkItemId,
        blocked_id: WorkItemId,
        observed_at: OffsetDateTime,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let code_repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let root_id = WorkItemId::generate();
        let parallel_id = WorkItemId::generate();
        let blocked_id = WorkItemId::generate();
        let observed_at = OffsetDateTime::parse(
            "2026-08-30T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("timestamp");
        let now = observed_at.unix_timestamp_nanos().to_string();
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (
                         id, slug, title, planning_store_repository_id, created_at
                     ) VALUES (?1, 'batch', 'Batch', ?2, ?3)",
                    params![
                        workspace_id.to_string(),
                        planning_repository_id.to_string(),
                        now
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory, default_branch,
                         is_planning_store, created_at
                     ) VALUES (?1, ?2, 'planning', 'Planning', 'planning.git', 'main', 1, ?4),
                              (?3, ?2, 'code', 'Code', 'code.git', 'main', 0, ?4)",
                    params![
                        planning_repository_id.to_string(),
                        workspace_id.to_string(),
                        code_repository_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'managed', 'Managed', ?3)",
                    params![epic_id.to_string(), workspace_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'batch', 'Batch', 'planned', ?3)",
                    params![feature_id.to_string(), epic_id.to_string(), now],
                )?;
                for (position, work_item_id, slug, status) in [
                    (0, root_id, "root", "ready"),
                    (1, parallel_id, "parallel", "ready"),
                    (2, blocked_id, "blocked", "ready"),
                ] {
                    transaction.execute(
                        "INSERT INTO work_items (
                             id, feature_id, key, slug, title, status, created_at, proposal_order
                         ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)",
                        params![
                            work_item_id.to_string(),
                            feature_id.to_string(),
                            format!("batch/{slug}"),
                            slug,
                            status,
                            now,
                            position,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO work_item_repositories (work_item_id, repository_id)
                         VALUES (?1, ?2)",
                        params![work_item_id.to_string(), code_repository_id.to_string()],
                    )?;
                    transaction.execute(
                        "INSERT INTO documents (
                             id, repository_id, work_item_id, kind, relative_path,
                             content_hash, observed_at
                         ) VALUES (?1, ?2, ?3, 'work_item', ?4, ?5, ?6)",
                        params![
                            DocumentId::generate().to_string(),
                            planning_repository_id.to_string(),
                            work_item_id.to_string(),
                            format!("work-items/{slug}.md"),
                            "0".repeat(64),
                            now,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO work_item_dependencies (
                         work_item_id, dependency_work_item_id, dependency_order
                     ) VALUES (?1, ?2, 0)",
                    params![blocked_id.to_string(), root_id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed batch fixture");
        Fixture {
            _directory: directory,
            store,
            feature_id,
            root_id,
            parallel_id,
            blocked_id,
            observed_at,
        }
    }

    #[test]
    fn all_ready_preview_is_dependency_aware_stable_and_requires_confirmation() {
        let mut fixture = fixture();
        let preview = BatchLaunchService::new(&mut fixture.store)
            .preview(PreviewManagedLaunchBatch {
                feature_id: fixture.feature_id,
                work_item_ids: Vec::new(),
                tool: Tool::Codex,
                profile: LaunchProfile::suggested(
                    Tool::Codex,
                    ManagedSessionRole::WorkItemExecution,
                ),
                idempotency_key: "batch-preview".to_owned(),
                created_at: fixture.observed_at,
            })
            .expect("preview ready batch");

        assert_eq!(preview.children.len(), 2);
        assert_eq!(preview.children[0].work_item_id, fixture.root_id);
        assert_eq!(preview.children[1].work_item_id, fixture.parallel_id);
        assert!(
            !preview
                .children
                .iter()
                .any(|child| child.work_item_id == fixture.blocked_id)
        );

        let error = BatchLaunchService::new(&mut fixture.store)
            .reserve(ConfirmManagedLaunchBatch {
                batch_id: preview.batch_id,
                confirmation_token: "wrong-token".to_owned(),
                confirmed_at: fixture.observed_at,
            })
            .expect_err("wrong confirmation must fail before checkout preflight");
        let launch_intents = fixture
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM launch_intents", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("launch intent count");

        assert_eq!(error.code(), "batch_confirmation_invalid");
        assert_eq!(launch_intents, 0);
    }

    #[test]
    fn explicit_selection_rejects_a_blocked_child_without_persisting_a_batch() {
        let mut fixture = fixture();
        let error = BatchLaunchService::new(&mut fixture.store)
            .preview(PreviewManagedLaunchBatch {
                feature_id: fixture.feature_id,
                work_item_ids: vec![fixture.blocked_id],
                tool: Tool::Claude,
                profile: LaunchProfile::suggested(
                    Tool::Claude,
                    ManagedSessionRole::WorkItemExecution,
                ),
                idempotency_key: "blocked-preview".to_owned(),
                created_at: fixture.observed_at,
            })
            .expect_err("blocked selection must fail");
        let batches = fixture
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM managed_launch_batches", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("batch count");

        assert_eq!(error.code(), "batch_work_item_blocked");
        assert_eq!(batches, 0);
    }

    #[test]
    fn checkout_preflight_failure_records_zero_launch_and_skips_the_remaining_batch() {
        let mut fixture = fixture();
        let preview = BatchLaunchService::new(&mut fixture.store)
            .preview(PreviewManagedLaunchBatch {
                feature_id: fixture.feature_id,
                work_item_ids: Vec::new(),
                tool: Tool::Codex,
                profile: LaunchProfile::suggested(
                    Tool::Codex,
                    ManagedSessionRole::WorkItemExecution,
                ),
                idempotency_key: "preflight-failure".to_owned(),
                created_at: fixture.observed_at,
            })
            .expect("preview batch");
        let batch_id = preview.batch_id.clone();
        let error = BatchLaunchService::new(&mut fixture.store)
            .reserve_with(
                ConfirmManagedLaunchBatch {
                    batch_id: preview.batch_id,
                    confirmation_token: preview.confirmation_token,
                    confirmed_at: fixture.observed_at,
                },
                |_store, _batch_id, _child, _confirmed_at| {
                    Err(crate::AppError::CheckoutReconciliation {
                        code: "checkout_target_occupied".to_owned(),
                        message: "occupied".to_owned(),
                    })
                },
            )
            .expect_err("preflight must fail before launch");
        let reservation = BatchLaunchService::new(&mut fixture.store)
            .reservation(&batch_id)
            .expect("failed reservation");
        let launch_intents = fixture
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM launch_intents", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("launch intent count");

        assert_eq!(error.code(), "checkout_target_occupied");
        assert_eq!(reservation.status, "failed");
        assert_eq!(reservation.children[0].status, "failed");
        assert_eq!(reservation.children[1].status, "skipped");
        assert_eq!(launch_intents, 0);
    }

    #[test]
    fn successful_preflight_atomically_reserves_every_child_and_consumes_confirmation() {
        let mut fixture = fixture();
        let preview = BatchLaunchService::new(&mut fixture.store)
            .preview(PreviewManagedLaunchBatch {
                feature_id: fixture.feature_id,
                work_item_ids: Vec::new(),
                tool: Tool::Codex,
                profile: LaunchProfile::suggested(
                    Tool::Codex,
                    ManagedSessionRole::WorkItemExecution,
                ),
                idempotency_key: "successful-preflight".to_owned(),
                created_at: fixture.observed_at,
            })
            .expect("preview batch");
        let confirmation = ConfirmManagedLaunchBatch {
            batch_id: preview.batch_id.clone(),
            confirmation_token: preview.confirmation_token,
            confirmed_at: fixture.observed_at,
        };
        let reservation = BatchLaunchService::new(&mut fixture.store)
            .reserve_with(
                confirmation.clone(),
                |store, _batch_id, child, confirmed_at| {
                    let checkout_id = CheckoutId::generate();
                    let checkout_path_id = CheckoutPathId::generate();
                    let path = format!("C:/batch/{}", child.work_item_id);
                    let identity = format!("identity-{}", child.work_item_id);
                    let branch = format!("work-item/{}", child.work_item_id);
                    let at = confirmed_at.unix_timestamp_nanos().to_string();
                    store.write(|transaction| {
                        transaction.execute(
                            "INSERT INTO checkouts (
                                 id, repository_id, git_worktree_identity, branch, head,
                                 availability, created_at
                             ) VALUES (?1, ?2, ?3, ?4, 'head', 'available', ?5)",
                            params![
                                checkout_id.to_string(),
                                child.repository_id.to_string(),
                                identity,
                                branch,
                                at,
                            ],
                        )?;
                        transaction.execute(
                            "INSERT INTO checkout_paths (
                                 id, checkout_id, path, observed_from, observed_until
                             ) VALUES (?1, ?2, ?3, ?4, NULL)",
                            params![
                                checkout_path_id.to_string(),
                                checkout_id.to_string(),
                                path,
                                at,
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
                             ) VALUES (?1, 2, ?2, ?3, 'work_item_write', 'write_isolated',
                                       'work_item', ?4, NULL, '', NULL, 'main', 'head', ?5,
                                       ?6, ?7, 'head', 'available', 1, 1, '[]', ?8)",
                            params![
                                checkout_id.to_string(),
                                child.repository_id.to_string(),
                                checkout_path_id.to_string(),
                                child.work_item_id.to_string(),
                                path,
                                identity,
                                branch,
                                at,
                            ],
                        )?;
                        Ok(())
                    })?;
                    Ok(checkout_id)
                },
            )
            .expect("reserve batch");

        assert_eq!(reservation.status, "reserved");
        assert!(
            reservation
                .children
                .iter()
                .all(|child| child.status == "reserved" && child.checkout_id.is_some())
        );
        let repeated = BatchLaunchService::new(&mut fixture.store)
            .reserve_with(confirmation, |_store, _batch_id, _child, _confirmed_at| {
                unreachable!("consumed confirmation must fail before preflight")
            })
            .expect_err("confirmation can be consumed only once");
        assert_eq!(repeated.code(), "batch_confirmation_invalid");
    }
}
