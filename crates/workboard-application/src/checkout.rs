use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{
    CHECKOUT_READINESS_SCHEMA_VERSION, CheckoutAccessMode, CheckoutAvailability,
    CheckoutEvidenceKind, CheckoutId, CheckoutPathId, CheckoutPurpose, CheckoutReadiness,
    CheckoutReconciliationEvidence, FeatureId, HierarchyOwner, OperationIntentId, RepositoryId,
    WorkItemId,
};

use crate::AppError;
use crate::git::{GitCli, GitWorktreeCreator, GitWorktreeResolver, ResolvedWorktree};
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareFeatureCheckout {
    pub feature_id: FeatureId,
    pub repository_id: RepositoryId,
    pub target: PathBuf,
    pub branch: String,
    pub create_branch: bool,
    pub start_point: String,
    pub idempotency_key: String,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCheckoutOutcome {
    pub checkout_id: CheckoutId,
    pub feature_id: FeatureId,
    pub repository_id: RepositoryId,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: String,
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptFeatureCheckout {
    pub feature_id: FeatureId,
    pub checkout_id: CheckoutId,
    pub idempotency_key: String,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareWorkItemCheckout {
    pub work_item_id: WorkItemId,
    pub repository_id: RepositoryId,
    pub idempotency_key: String,
    pub observed_at: OffsetDateTime,
}

struct WorkItemCheckoutContext {
    repository_path: PathBuf,
    repository_common_dir: PathBuf,
    parent_checkout_id: CheckoutId,
    parent_path: PathBuf,
    parent_identity: PathBuf,
    base_revision: String,
    source_revision: String,
    target: PathBuf,
    branch: String,
}

pub struct CheckoutService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> CheckoutService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn prepare_work_item(
        &mut self,
        request: PrepareWorkItemCheckout,
    ) -> Result<CheckoutReadiness, AppError> {
        self.prepare_work_item_with(request, &GitCli)
    }

    pub fn prepare_work_item_with(
        &mut self,
        request: PrepareWorkItemCheckout,
        git: &(impl GitWorktreeCreator + GitWorktreeResolver),
    ) -> Result<CheckoutReadiness, AppError> {
        validate_work_item_request(&request)?;
        let context = self.work_item_context(&request)?;
        let parent = git.resolve(&context.parent_path)?;
        validate_parent_checkout(&context, &parent)?;
        let current = read_work_item_readiness(self.store, &request)?;
        if let Some(readiness) = current.as_ref() {
            validate_recorded_allocation(readiness, &context)?;
            reject_active_writer(self.store, readiness.checkout_id)?;
        }
        let intent_id = ensure_work_item_intent(self.store, &request, &context)?;
        let (resolved, evidence_kind) = if context.target.is_dir() {
            match git.resolve(&context.target) {
                Ok(resolved) => (resolved, CheckoutEvidenceKind::GitResolved),
                Err(_error) if directory_is_empty(&context.target)? => {
                    ensure_target_parent(&context.target)?;
                    let resolved = git.materialize(
                        &context.repository_path,
                        &context.target,
                        &context.branch,
                        &context.source_revision,
                    )?;
                    (resolved, CheckoutEvidenceKind::Restored)
                }
                Err(error) => {
                    return Err(checkout_conflict(
                        "checkout_target_occupied",
                        format!(
                            "the derived checkout target is occupied and did not resolve: {error}"
                        ),
                    ));
                }
            }
        } else if context.target.exists() {
            return Err(checkout_conflict(
                "checkout_target_occupied",
                format!(
                    "the derived checkout target is not a directory: {}",
                    context.target.display()
                ),
            ));
        } else {
            ensure_target_parent(&context.target)?;
            let resolved = git.materialize(
                &context.repository_path,
                &context.target,
                &context.branch,
                &context.source_revision,
            )?;
            let kind = if current.is_some() {
                CheckoutEvidenceKind::Restored
            } else {
                CheckoutEvidenceKind::Materialized
            };
            (resolved, kind)
        };
        if let Err(error) = validate_work_item_resolved(&context, &resolved) {
            if let Some(readiness) = current.as_ref() {
                record_unavailable(
                    self.store,
                    readiness,
                    request.observed_at,
                    error.to_string(),
                )?;
            }
            return Err(error);
        }
        if let Some(readiness) = current.as_ref()
            && let Err(error) = validate_reconciled_identity(readiness, &resolved)
        {
            record_unavailable(
                self.store,
                readiness,
                request.observed_at,
                error.to_string(),
            )?;
            return Err(error);
        }
        reject_allocation_collision(self.store, &request, &context, &resolved)?;
        persist_work_item_readiness(
            self.store,
            &request,
            &context,
            intent_id,
            &resolved,
            current.as_ref(),
            evidence_kind,
        )
    }

    pub fn readiness_for_checkout(
        &self,
        checkout_id: CheckoutId,
    ) -> Result<Option<CheckoutReadiness>, AppError> {
        read_readiness_by_checkout(self.store, checkout_id)
    }

    pub fn reconcile_registered_checkout(
        &mut self,
        checkout_id: CheckoutId,
        observed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.reconcile_registered_checkout_with(checkout_id, observed_at, &GitCli)
    }

    pub fn reconcile_registered_checkout_with(
        &mut self,
        checkout_id: CheckoutId,
        observed_at: OffsetDateTime,
        git: &impl GitWorktreeResolver,
    ) -> Result<(), AppError> {
        let checkout = read_registered_checkout(self.store, checkout_id)?;
        let readiness = read_readiness_by_checkout(self.store, checkout_id)?;
        let resolved = match git.resolve(&checkout.path) {
            Ok(resolved) => resolved,
            Err(error) => {
                mark_registered_checkout_missing(
                    self.store,
                    &checkout,
                    readiness.as_ref(),
                    observed_at,
                    error.to_string(),
                )?;
                return Err(error);
            }
        };
        let validation = if !paths_equal(&resolved.git_dir, &checkout.git_worktree_identity) {
            Err(checkout_conflict(
                "checkout_identity_drift",
                "the checkout resolved to a different Git worktree identity".to_owned(),
            ))
        } else if checkout.branch.as_deref() != resolved.branch.as_deref().map(short_branch) {
            Err(checkout_conflict(
                "checkout_branch_drift",
                "the checkout resolved to a different branch".to_owned(),
            ))
        } else {
            Ok(())
        };
        if let Err(error) = validation {
            mark_registered_checkout_missing(
                self.store,
                &checkout,
                readiness.as_ref(),
                observed_at,
                error.to_string(),
            )?;
            return Err(error);
        }
        record_registered_checkout_reconciliation(
            self.store,
            &checkout,
            readiness.as_ref(),
            &resolved,
            observed_at,
        )
    }

    fn work_item_context(
        &self,
        request: &PrepareWorkItemCheckout,
    ) -> Result<WorkItemCheckoutContext, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT repository_path.path, repository.git_common_directory,
                            feature_checkout.checkout_id,
                            checkout_path.path, checkout.git_worktree_identity,
                            checkout.branch, checkout.head
                     FROM work_items item
                     JOIN work_item_repositories target
                       ON target.work_item_id = item.id AND target.repository_id = ?2
                     JOIN repositories repository ON repository.id = target.repository_id
                     JOIN repository_paths repository_path
                       ON repository_path.repository_id = repository.id
                      AND repository_path.observed_until IS NULL
                     JOIN feature_checkouts feature_checkout
                       ON feature_checkout.feature_id = item.feature_id
                      AND feature_checkout.repository_id = repository.id
                     JOIN checkouts checkout
                       ON checkout.id = feature_checkout.checkout_id
                      AND checkout.availability = 'available'
                     JOIN checkout_paths checkout_path
                       ON checkout_path.checkout_id = checkout.id
                      AND checkout_path.observed_until IS NULL
                     WHERE item.id = ?1",
                    params![
                        request.work_item_id.to_string(),
                        request.repository_id.to_string()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                repository_path,
                repository_common_dir,
                parent_checkout_id,
                parent_path,
                parent_identity,
                parent_branch,
                parent_head,
            )) = row
            else {
                return Err(AppError::ResumeCheckoutRequired);
            };
            let source_revision =
                parent_head
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        checkout_conflict(
                            "feature_checkout_head_missing",
                            "the Feature integration checkout has no recorded head".to_owned(),
                        )
                    })?;
            let repository_path = PathBuf::from(repository_path);
            Ok(WorkItemCheckoutContext {
                repository_common_dir: PathBuf::from(repository_common_dir),
                parent_checkout_id: parse_id(&parent_checkout_id)?,
                parent_path: PathBuf::from(parent_path),
                parent_identity: PathBuf::from(parent_identity),
                base_revision: parent_branch.unwrap_or_else(|| source_revision.clone()),
                target: work_item_target(&repository_path, request.work_item_id)?,
                branch: format!("work-item/{}", request.work_item_id),
                repository_path,
                source_revision,
            })
        })
    }

    pub fn prepare_feature(
        &mut self,
        request: PrepareFeatureCheckout,
    ) -> Result<FeatureCheckoutOutcome, AppError> {
        self.prepare_feature_with(request, &GitCli)
    }

    pub fn adopt_feature_checkout(
        &mut self,
        request: AdoptFeatureCheckout,
    ) -> Result<FeatureCheckoutOutcome, AppError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(AppError::EmptyIdempotencyKey);
        }
        let existing = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT feature_id, payload_json FROM operation_intents
                     WHERE idempotency_key = ?1 AND kind = 'feature_checkout_adoption'",
                    [request.idempotency_key.as_str()],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(Into::into)
        })?;
        let payload = serde_json::json!({
            "checkoutId": request.checkout_id,
            "featureId": request.feature_id,
        })
        .to_string();
        if let Some((feature_id, existing_payload)) = existing.as_ref()
            && (feature_id.as_deref() != Some(request.feature_id.to_string().as_str())
                || existing_payload != &payload)
        {
            return Err(AppError::IdempotencyConflict);
        }
        let checkout = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT checkout.repository_id, path.path, checkout.branch, checkout.head
                     FROM checkouts checkout
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     JOIN repositories repository ON repository.id = checkout.repository_id
                     JOIN epics epic ON epic.workspace_id = repository.workspace_id
                     JOIN features feature ON feature.epic_id = epic.id
                     WHERE checkout.id = ?1 AND feature.id = ?2
                       AND checkout.availability = 'available'
                       AND repository.is_planning_store = 0",
                    params![
                        request.checkout_id.to_string(),
                        request.feature_id.to_string()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let (repository_id, path, branch, head) =
            checkout.ok_or(AppError::ResumeCheckoutNotScanned)?;
        let repository_id = parse_id::<RepositoryId>(&repository_id)?;
        let at = timestamp(request.observed_at);
        self.store.write(|transaction| {
            if existing.is_none() {
                transaction.execute(
                    "INSERT INTO operation_intents (
                         id, feature_id, idempotency_key, kind, status, payload_json,
                         created_at, completed_at
                     ) VALUES (?1, ?2, ?3, 'feature_checkout_adoption', 'completed', ?4, ?5, ?5)",
                    params![
                        OperationIntentId::generate().to_string(),
                        request.feature_id.to_string(),
                        request.idempotency_key,
                        payload,
                        at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(feature_id, repository_id) DO UPDATE SET
                         checkout_id = excluded.checkout_id,
                         assigned_at = excluded.assigned_at",
                    params![
                        request.feature_id.to_string(),
                        repository_id.to_string(),
                        request.checkout_id.to_string(),
                        at,
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(FeatureCheckoutOutcome {
            checkout_id: request.checkout_id,
            feature_id: request.feature_id,
            repository_id,
            path: PathBuf::from(path),
            branch,
            head: head.unwrap_or_default(),
            reused: existing.is_some(),
        })
    }

    pub fn prepare_feature_with(
        &mut self,
        request: PrepareFeatureCheckout,
        git: &(impl GitWorktreeCreator + GitWorktreeResolver),
    ) -> Result<FeatureCheckoutOutcome, AppError> {
        validate_request(&request)?;
        let repository_path = self.repository_path(request.feature_id, request.repository_id)?;
        if let Some(existing) = self.current_feature_checkout(&request)?
            && git.resolve(&existing.path).is_ok()
        {
            self.record_reuse(&request)?;
            return Ok(existing);
        }
        let intent_id = self.ensure_pending_intent(&request)?;
        let resolved = if request.target.is_dir() {
            git.resolve(&request.target)?
        } else {
            git.recreate(
                &repository_path,
                &request.target,
                &request.branch,
                request.create_branch,
                &request.start_point,
            )?
        };
        validate_resolved(&request, &resolved)?;
        self.persist_checkout(&request, intent_id, &resolved)
    }

    fn repository_path(
        &self,
        feature_id: FeatureId,
        repository_id: RepositoryId,
    ) -> Result<PathBuf, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT path.path
                     FROM features feature
                     JOIN epics epic ON epic.id = feature.epic_id
                     JOIN repositories repository
                       ON repository.workspace_id = epic.workspace_id
                      AND repository.id = ?2
                      AND repository.is_planning_store = 0
                     JOIN repository_paths path
                       ON path.repository_id = repository.id AND path.observed_until IS NULL
                     WHERE feature.id = ?1",
                    params![feature_id.to_string(), repository_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(PathBuf::from)
                .ok_or(AppError::ResumeRepositoryMismatch)
        })
    }

    fn current_feature_checkout(
        &self,
        request: &PrepareFeatureCheckout,
    ) -> Result<Option<FeatureCheckoutOutcome>, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT checkout.id, path.path, checkout.branch, checkout.head
                     FROM feature_checkouts feature
                     JOIN checkouts checkout
                       ON checkout.id = feature.checkout_id
                      AND checkout.availability = 'available'
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     WHERE feature.feature_id = ?1 AND feature.repository_id = ?2",
                    params![
                        request.feature_id.to_string(),
                        request.repository_id.to_string()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()?;
            row.map(|(checkout_id, path, branch, head)| {
                Ok(FeatureCheckoutOutcome {
                    checkout_id: parse_id(&checkout_id)?,
                    feature_id: request.feature_id,
                    repository_id: request.repository_id,
                    path: PathBuf::from(path),
                    branch,
                    head: head.unwrap_or_default(),
                    reused: true,
                })
            })
            .transpose()
        })
    }

    fn record_reuse(&mut self, request: &PrepareFeatureCheckout) -> Result<(), AppError> {
        let now = timestamp(request.observed_at);
        let payload = request_payload(request)?;
        self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT feature_id, kind FROM operation_intents WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((feature_id, kind)) = existing {
                if feature_id.as_deref() != Some(request.feature_id.to_string().as_str())
                    || kind != "feature_checkout"
                {
                    return Err(AppError::IdempotencyConflict);
                }
                return Ok(());
            }
            transaction.execute(
                "INSERT INTO operation_intents (
                     id, feature_id, idempotency_key, kind, status, payload_json,
                     created_at, completed_at
                 ) VALUES (?1, ?2, ?3, 'feature_checkout', 'completed', ?4, ?5, ?5)",
                params![
                    OperationIntentId::generate().to_string(),
                    request.feature_id.to_string(),
                    request.idempotency_key,
                    payload,
                    now,
                ],
            )?;
            Ok(())
        })
    }

    fn ensure_pending_intent(
        &mut self,
        request: &PrepareFeatureCheckout,
    ) -> Result<OperationIntentId, AppError> {
        let now = timestamp(request.observed_at);
        let payload = request_payload(request)?;
        self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT id, feature_id, kind, payload_json
                     FROM operation_intents WHERE idempotency_key = ?1",
                    [request.idempotency_key.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((id, feature_id, kind, existing_payload)) = existing {
                if feature_id.as_deref() != Some(request.feature_id.to_string().as_str())
                    || kind != "feature_checkout"
                    || existing_payload != payload
                {
                    return Err(AppError::IdempotencyConflict);
                }
                return parse_id(&id);
            }
            let intent_id = OperationIntentId::generate();
            transaction.execute(
                "INSERT INTO operation_intents (
                     id, feature_id, idempotency_key, kind, status, payload_json, created_at
                 ) VALUES (?1, ?2, ?3, 'feature_checkout', 'pending', ?4, ?5)",
                params![
                    intent_id.to_string(),
                    request.feature_id.to_string(),
                    request.idempotency_key,
                    payload,
                    now,
                ],
            )?;
            Ok(intent_id)
        })
    }

    fn persist_checkout(
        &mut self,
        request: &PrepareFeatureCheckout,
        intent_id: OperationIntentId,
        resolved: &ResolvedWorktree,
    ) -> Result<FeatureCheckoutOutcome, AppError> {
        let now = timestamp(request.observed_at);
        let identity = path_text(&resolved.git_dir)?;
        let path = path_text(&resolved.path)?;
        let branch = resolved.branch.as_deref().map(short_branch);
        let checkout_id = self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT id FROM checkouts
                     WHERE repository_id = ?1 AND git_worktree_identity = ?2",
                    params![request.repository_id.to_string(), identity],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let checkout_id = existing
                .as_deref()
                .map(parse_id)
                .transpose()?
                .unwrap_or_else(CheckoutId::generate);
            transaction.execute(
                "INSERT INTO checkouts (
                     id, repository_id, git_worktree_identity, branch, head,
                     availability, created_intent_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'available', ?6, ?7)
                 ON CONFLICT(repository_id, git_worktree_identity) DO UPDATE SET
                     branch = excluded.branch,
                     head = excluded.head,
                     availability = 'available'",
                params![
                    checkout_id.to_string(),
                    request.repository_id.to_string(),
                    identity,
                    branch,
                    resolved.head_oid,
                    intent_id.to_string(),
                    now,
                ],
            )?;
            let current_path = transaction
                .query_row(
                    "SELECT id, path FROM checkout_paths
                     WHERE checkout_id = ?1 AND observed_until IS NULL",
                    [checkout_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if current_path
                .as_ref()
                .is_none_or(|(_, current)| !paths_equal(Path::new(current), &resolved.path))
            {
                if let Some((path_id, _)) = current_path {
                    transaction.execute(
                        "UPDATE checkout_paths SET observed_until = ?2 WHERE id = ?1",
                        params![path_id, now],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(),
                        checkout_id.to_string(),
                        path,
                        now,
                    ],
                )?;
            }
            transaction.execute(
                "INSERT INTO feature_checkouts (
                     feature_id, repository_id, checkout_id, assigned_at
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(feature_id, repository_id) DO UPDATE SET
                     checkout_id = excluded.checkout_id,
                     assigned_at = excluded.assigned_at",
                params![
                    request.feature_id.to_string(),
                    request.repository_id.to_string(),
                    checkout_id.to_string(),
                    now,
                ],
            )?;
            transaction.execute(
                "UPDATE operation_intents
                 SET status = 'completed', completed_at = ?2
                 WHERE id = ?1 AND status IN ('pending', 'completed')",
                params![intent_id.to_string(), now],
            )?;
            Ok(checkout_id)
        })?;
        Ok(FeatureCheckoutOutcome {
            checkout_id,
            feature_id: request.feature_id,
            repository_id: request.repository_id,
            path: resolved.path.clone(),
            branch: branch.map(str::to_owned),
            head: resolved.head_oid.clone(),
            reused: false,
        })
    }
}

struct RawCheckoutReadiness {
    schema_version: i64,
    repository_id: String,
    checkout_id: String,
    checkout_path_id: String,
    purpose: String,
    access_mode: String,
    owner_kind: String,
    owner_id: String,
    session_id: Option<String>,
    parent_feature_checkout_id: Option<String>,
    base_revision: String,
    source_revision: String,
    path: String,
    git_worktree_identity: String,
    branch: Option<String>,
    head: String,
    availability: String,
    isolation_generation: i64,
    reconciliation_generation: i64,
    evidence_json: String,
}

struct RegisteredCheckout {
    checkout_id: CheckoutId,
    path: PathBuf,
    git_worktree_identity: PathBuf,
    branch: Option<String>,
}

fn read_registered_checkout(
    store: &SqliteStore,
    checkout_id: CheckoutId,
) -> Result<RegisteredCheckout, AppError> {
    store.read(|connection| {
        connection
            .query_row(
                "SELECT checkout.id, path.path, checkout.git_worktree_identity,
                        checkout.branch
                 FROM checkouts checkout
                 JOIN checkout_paths path
                   ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                 WHERE checkout.id = ?1",
                [checkout_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .map(|(id, path, identity, branch)| {
                Ok::<_, AppError>(RegisteredCheckout {
                    checkout_id: parse_id(&id)?,
                    path: PathBuf::from(path),
                    git_worktree_identity: PathBuf::from(identity),
                    branch,
                })
            })
            .transpose()?
            .ok_or(AppError::ResumeCheckoutRequired)
    })
}

fn mark_registered_checkout_missing(
    store: &mut SqliteStore,
    checkout: &RegisteredCheckout,
    readiness: Option<&CheckoutReadiness>,
    observed_at: OffsetDateTime,
    detail: String,
) -> Result<(), AppError> {
    if let Some(readiness) = readiness {
        return record_unavailable(store, readiness, observed_at, detail);
    }
    store.write(|transaction| {
        transaction.execute(
            "UPDATE checkouts SET availability = 'missing' WHERE id = ?1",
            [checkout.checkout_id.to_string()],
        )?;
        Ok(())
    })
}

fn record_registered_checkout_reconciliation(
    store: &mut SqliteStore,
    checkout: &RegisteredCheckout,
    readiness: Option<&CheckoutReadiness>,
    resolved: &ResolvedWorktree,
    observed_at: OffsetDateTime,
) -> Result<(), AppError> {
    let at = timestamp(observed_at);
    if let Some(readiness) = readiness {
        let generation = readiness.reconciliation_generation + 1;
        let evidence = vec![
            CheckoutReconciliationEvidence {
                kind: CheckoutEvidenceKind::GitResolved,
                observed_at,
                detail: checkout.path.display().to_string(),
            },
            CheckoutReconciliationEvidence {
                kind: CheckoutEvidenceKind::IdentityVerified,
                observed_at,
                detail: checkout.git_worktree_identity.display().to_string(),
            },
        ];
        let evidence_json = serde_json::to_string(&evidence)?;
        store.write(|transaction| {
            transaction.execute(
                "UPDATE checkouts SET head = ?2, availability = 'available' WHERE id = ?1",
                params![checkout.checkout_id.to_string(), resolved.head_oid],
            )?;
            transaction.execute(
                "UPDATE checkout_readiness
                 SET head = ?2, availability = 'available',
                     reconciliation_generation = ?3, evidence_json = ?4, observed_at = ?5
                 WHERE checkout_id = ?1",
                params![
                    checkout.checkout_id.to_string(),
                    resolved.head_oid,
                    i64::try_from(generation)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    evidence_json,
                    at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO checkout_reconciliation_events (
                     checkout_id, generation, availability, head, evidence_json, observed_at
                 ) VALUES (?1, ?2, 'available', ?3, ?4, ?5)",
                params![
                    checkout.checkout_id.to_string(),
                    i64::try_from(generation)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    resolved.head_oid,
                    evidence_json,
                    at,
                ],
            )?;
            Ok(())
        })
    } else {
        store.write(|transaction| {
            transaction.execute(
                "UPDATE checkouts SET head = ?2, availability = 'available' WHERE id = ?1",
                params![checkout.checkout_id.to_string(), resolved.head_oid],
            )?;
            Ok(())
        })
    }
}

fn validate_work_item_request(request: &PrepareWorkItemCheckout) -> Result<(), AppError> {
    if request.idempotency_key.trim().is_empty() {
        return Err(AppError::EmptyIdempotencyKey);
    }
    Ok(())
}

fn validate_parent_checkout(
    context: &WorkItemCheckoutContext,
    resolved: &ResolvedWorktree,
) -> Result<(), AppError> {
    if !paths_equal(&resolved.path, &context.parent_path)
        || !paths_equal(&resolved.git_dir, &context.parent_identity)
        || !paths_equal(&resolved.common_dir, &context.repository_common_dir)
    {
        return Err(checkout_conflict(
            "feature_checkout_identity_drift",
            "the Feature integration checkout no longer matches its recorded Git identity"
                .to_owned(),
        ));
    }
    if resolved.head_oid != context.source_revision {
        return Err(checkout_conflict(
            "feature_checkout_head_drift",
            format!(
                "the Feature integration checkout head changed from {} to {}",
                context.source_revision, resolved.head_oid
            ),
        ));
    }
    Ok(())
}

fn validate_recorded_allocation(
    readiness: &CheckoutReadiness,
    context: &WorkItemCheckoutContext,
) -> Result<(), AppError> {
    if readiness.schema_version != CHECKOUT_READINESS_SCHEMA_VERSION
        || readiness.purpose != CheckoutPurpose::WorkItemWrite
        || readiness.access_mode != CheckoutAccessMode::WriteIsolated
        || readiness.parent_feature_checkout_id != Some(context.parent_checkout_id)
        || !paths_equal(&readiness.path, &context.target)
        || readiness.branch.as_deref() != Some(context.branch.as_str())
    {
        return Err(checkout_conflict(
            "checkout_allocation_drift",
            "the recorded Work-item checkout allocation no longer matches its derived boundary"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_work_item_resolved(
    context: &WorkItemCheckoutContext,
    resolved: &ResolvedWorktree,
) -> Result<(), AppError> {
    if !paths_equal(&resolved.path, &context.target) {
        return Err(checkout_conflict(
            "checkout_path_mismatch",
            "Git resolved a different worktree path than the derived target".to_owned(),
        ));
    }
    if !paths_equal(&resolved.common_dir, &context.repository_common_dir) {
        return Err(checkout_conflict(
            "checkout_repository_mismatch",
            "the derived target belongs to a different Git repository".to_owned(),
        ));
    }
    if paths_equal(&resolved.git_dir, &context.parent_identity) {
        return Err(checkout_conflict(
            "checkout_not_isolated",
            "the Work-item checkout shares the Feature integration Git identity".to_owned(),
        ));
    }
    if resolved.branch.as_deref().map(short_branch) != Some(context.branch.as_str()) {
        return Err(checkout_conflict(
            "checkout_branch_mismatch",
            format!("the Work-item checkout is not on {}", context.branch),
        ));
    }
    if resolved.head_oid.is_empty() {
        return Err(checkout_conflict(
            "checkout_head_missing",
            "the Work-item checkout has no resolved head".to_owned(),
        ));
    }
    Ok(())
}

fn validate_reconciled_identity(
    readiness: &CheckoutReadiness,
    resolved: &ResolvedWorktree,
) -> Result<(), AppError> {
    if !paths_equal(&readiness.git_worktree_identity, &resolved.git_dir) {
        return Err(checkout_conflict(
            "checkout_identity_drift",
            "the Work-item checkout resolved to a different Git worktree identity".to_owned(),
        ));
    }
    Ok(())
}

fn work_item_target(repository_path: &Path, work_item_id: WorkItemId) -> Result<PathBuf, AppError> {
    if !repository_path.is_absolute() {
        return Err(AppError::WorktreePathNotAbsolute(
            repository_path.to_path_buf(),
        ));
    }
    let parent = repository_path.parent().ok_or_else(|| {
        checkout_conflict(
            "checkout_parent_missing",
            "the repository path has no parent for isolated worktrees".to_owned(),
        )
    })?;
    let name = repository_path.file_name().ok_or_else(|| {
        checkout_conflict(
            "checkout_repository_name_missing",
            "the repository path has no stable directory name".to_owned(),
        )
    })?;
    Ok(parent
        .join(format!("{}.worktrees", name.to_string_lossy()))
        .join(format!("WorkItem-{work_item_id}")))
}

fn ensure_target_parent(target: &Path) -> Result<(), AppError> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::RecreateCheckoutParentMissing(target.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(AppError::GitIo)
}

fn directory_is_empty(path: &Path) -> Result<bool, AppError> {
    let mut entries = fs::read_dir(path).map_err(AppError::GitIo)?;
    Ok(entries
        .next()
        .transpose()
        .map_err(AppError::GitIo)?
        .is_none())
}

fn ensure_work_item_intent(
    store: &mut SqliteStore,
    request: &PrepareWorkItemCheckout,
    context: &WorkItemCheckoutContext,
) -> Result<OperationIntentId, AppError> {
    let payload = serde_json::to_string(&serde_json::json!({
        "schema_version": CHECKOUT_READINESS_SCHEMA_VERSION,
        "work_item_id": request.work_item_id,
        "repository_id": request.repository_id,
        "parent_feature_checkout_id": context.parent_checkout_id,
        "target": context.target,
        "branch": context.branch,
        "base_revision": context.base_revision,
        "source_revision": context.source_revision,
    }))?;
    let at = timestamp(request.observed_at);
    store.write(|transaction| {
        let existing = transaction
            .query_row(
                "SELECT id, work_item_id, kind, payload_json
                 FROM operation_intents WHERE idempotency_key = ?1",
                [request.idempotency_key.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((id, work_item_id, kind, existing_payload)) = existing {
            if work_item_id.as_deref() != Some(request.work_item_id.to_string().as_str())
                || kind != "work_item_checkout"
                || existing_payload != payload
            {
                return Err(AppError::IdempotencyConflict);
            }
            return parse_id(&id);
        }
        let intent_id = OperationIntentId::generate();
        transaction.execute(
            "INSERT INTO operation_intents (
                 id, work_item_id, idempotency_key, kind, status, payload_json, created_at
             ) VALUES (?1, ?2, ?3, 'work_item_checkout', 'pending', ?4, ?5)",
            params![
                intent_id.to_string(),
                request.work_item_id.to_string(),
                request.idempotency_key,
                payload,
                at,
            ],
        )?;
        Ok(intent_id)
    })
}

fn read_work_item_readiness(
    store: &SqliteStore,
    request: &PrepareWorkItemCheckout,
) -> Result<Option<CheckoutReadiness>, AppError> {
    store.read(|connection| {
        let raw = connection
            .query_row(
                &readiness_select(
                    "WHERE repository_id = ?1 AND purpose = 'work_item_write'\
                     AND owner_kind = 'work_item' AND owner_id = ?2 AND session_key = ''",
                ),
                params![
                    request.repository_id.to_string(),
                    request.work_item_id.to_string()
                ],
                raw_readiness,
            )
            .optional()?;
        raw.map(parse_readiness).transpose()
    })
}

fn read_readiness_by_checkout(
    store: &SqliteStore,
    checkout_id: CheckoutId,
) -> Result<Option<CheckoutReadiness>, AppError> {
    store.read(|connection| {
        let raw = connection
            .query_row(
                &readiness_select("WHERE checkout_id = ?1"),
                [checkout_id.to_string()],
                raw_readiness,
            )
            .optional()?;
        raw.map(parse_readiness).transpose()
    })
}

fn readiness_select(filter: &str) -> String {
    format!(
        "SELECT schema_version, repository_id, checkout_id, checkout_path_id,
                purpose, access_mode, owner_kind, owner_id, session_id,
                parent_feature_checkout_id, base_revision, source_revision,
                path, git_worktree_identity, branch, head, availability,
                isolation_generation, reconciliation_generation, evidence_json
         FROM checkout_readiness {filter}"
    )
}

fn raw_readiness(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawCheckoutReadiness> {
    Ok(RawCheckoutReadiness {
        schema_version: row.get(0)?,
        repository_id: row.get(1)?,
        checkout_id: row.get(2)?,
        checkout_path_id: row.get(3)?,
        purpose: row.get(4)?,
        access_mode: row.get(5)?,
        owner_kind: row.get(6)?,
        owner_id: row.get(7)?,
        session_id: row.get(8)?,
        parent_feature_checkout_id: row.get(9)?,
        base_revision: row.get(10)?,
        source_revision: row.get(11)?,
        path: row.get(12)?,
        git_worktree_identity: row.get(13)?,
        branch: row.get(14)?,
        head: row.get(15)?,
        availability: row.get(16)?,
        isolation_generation: row.get(17)?,
        reconciliation_generation: row.get(18)?,
        evidence_json: row.get(19)?,
    })
}

fn parse_readiness(raw: RawCheckoutReadiness) -> Result<CheckoutReadiness, AppError> {
    Ok(CheckoutReadiness {
        schema_version: u32::try_from(raw.schema_version)
            .map_err(|error| AppError::Domain(error.to_string()))?,
        repository_id: parse_id(&raw.repository_id)?,
        checkout_id: parse_id(&raw.checkout_id)?,
        checkout_path_id: parse_id(&raw.checkout_path_id)?,
        purpose: parse_checkout_purpose(&raw.purpose)?,
        access_mode: parse_checkout_access_mode(&raw.access_mode)?,
        owner: parse_readiness_owner(&raw.owner_kind, &raw.owner_id)?,
        session_id: raw.session_id.as_deref().map(parse_id).transpose()?,
        parent_feature_checkout_id: raw
            .parent_feature_checkout_id
            .as_deref()
            .map(parse_id)
            .transpose()?,
        base_revision: raw.base_revision,
        source_revision: raw.source_revision,
        path: PathBuf::from(raw.path),
        git_worktree_identity: PathBuf::from(raw.git_worktree_identity),
        branch: raw.branch,
        head: raw.head,
        availability: parse_checkout_availability(&raw.availability)?,
        isolation_generation: u64::try_from(raw.isolation_generation)
            .map_err(|error| AppError::Domain(error.to_string()))?,
        reconciliation_generation: u64::try_from(raw.reconciliation_generation)
            .map_err(|error| AppError::Domain(error.to_string()))?,
        evidence: serde_json::from_str(&raw.evidence_json)?,
    })
}

fn reject_active_writer(store: &SqliteStore, checkout_id: CheckoutId) -> Result<(), AppError> {
    let active = store.read(|connection| {
        connection
            .query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM managed_sessions
                     WHERE checkout_id = ?1 AND managed_until IS NULL
                       AND status IN ('bound', 'adopted')
                 )",
                [checkout_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .map_err(Into::into)
    })?;
    if active {
        Err(checkout_conflict(
            "checkout_writer_active",
            "the Work-item checkout already has a current writer".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn reject_allocation_collision(
    store: &SqliteStore,
    request: &PrepareWorkItemCheckout,
    context: &WorkItemCheckoutContext,
    resolved: &ResolvedWorktree,
) -> Result<(), AppError> {
    let collision = store.read(|connection| {
        connection
            .query_row(
                "SELECT owner_kind, owner_id FROM checkout_readiness
                 WHERE repository_id = ?1
                   AND (path = ?2 OR git_worktree_identity = ?3 OR branch = ?4)
                   AND NOT (owner_kind = 'work_item' AND owner_id = ?5
                            AND purpose = 'work_item_write' AND session_key = '')
                 LIMIT 1",
                params![
                    request.repository_id.to_string(),
                    path_text(&context.target)?,
                    path_text(&resolved.git_dir)?,
                    context.branch,
                    request.work_item_id.to_string(),
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(Into::into)
    })?;
    if let Some((kind, id)) = collision {
        Err(checkout_conflict(
            "checkout_allocation_conflict",
            format!("the derived checkout boundary is already owned by {kind} {id}"),
        ))
    } else {
        Ok(())
    }
}

fn persist_work_item_readiness(
    store: &mut SqliteStore,
    request: &PrepareWorkItemCheckout,
    context: &WorkItemCheckoutContext,
    intent_id: OperationIntentId,
    resolved: &ResolvedWorktree,
    current: Option<&CheckoutReadiness>,
    evidence_kind: CheckoutEvidenceKind,
) -> Result<CheckoutReadiness, AppError> {
    let at = timestamp(request.observed_at);
    let identity = path_text(&resolved.git_dir)?;
    let path = path_text(&resolved.path)?;
    let branch = resolved.branch.as_deref().map(short_branch);
    let isolation_generation = current.map_or(1, |value| value.isolation_generation);
    let reconciliation_generation = current.map_or(1, |value| value.reconciliation_generation + 1);
    let evidence = vec![
        CheckoutReconciliationEvidence {
            kind: CheckoutEvidenceKind::IntentRecorded,
            observed_at: request.observed_at,
            detail: intent_id.to_string(),
        },
        CheckoutReconciliationEvidence {
            kind: evidence_kind,
            observed_at: request.observed_at,
            detail: resolved.path.display().to_string(),
        },
        CheckoutReconciliationEvidence {
            kind: CheckoutEvidenceKind::IdentityVerified,
            observed_at: request.observed_at,
            detail: resolved.git_dir.display().to_string(),
        },
    ];
    let evidence_json = serde_json::to_string(&evidence)?;
    let (checkout_id, checkout_path_id) = store.write(|transaction| {
        let existing_checkout = transaction
            .query_row(
                "SELECT id FROM checkouts
                 WHERE repository_id = ?1 AND git_worktree_identity = ?2",
                params![request.repository_id.to_string(), identity],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let checkout_id = existing_checkout
            .as_deref()
            .map(parse_id)
            .transpose()?
            .unwrap_or_else(CheckoutId::generate);
        if current.is_some_and(|value| value.checkout_id != checkout_id) {
            return Err(checkout_conflict(
                "checkout_identity_reassigned",
                "the recorded checkout identity now resolves to a different checkout".to_owned(),
            ));
        }
        transaction.execute(
            "INSERT INTO checkouts (
                 id, repository_id, git_worktree_identity, branch, head,
                 availability, created_intent_id, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'available', ?6, ?7)
             ON CONFLICT(repository_id, git_worktree_identity) DO UPDATE SET
                 branch = excluded.branch,
                 head = excluded.head,
                 availability = 'available'",
            params![
                checkout_id.to_string(),
                request.repository_id.to_string(),
                identity,
                branch,
                resolved.head_oid,
                intent_id.to_string(),
                at,
            ],
        )?;
        let current_path = transaction
            .query_row(
                "SELECT id, path FROM checkout_paths
                 WHERE checkout_id = ?1 AND observed_until IS NULL",
                [checkout_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let checkout_path_id = if let Some((path_id, current_path)) = current_path {
            if !paths_equal(Path::new(&current_path), &resolved.path) {
                return Err(checkout_conflict(
                    "checkout_path_reassigned",
                    "the Git worktree identity is already recorded at another path".to_owned(),
                ));
            }
            parse_id(&path_id)?
        } else {
            let path_id = CheckoutPathId::generate();
            transaction.execute(
                "INSERT INTO checkout_paths (
                     id, checkout_id, path, observed_from, observed_until
                 ) VALUES (?1, ?2, ?3, ?4, NULL)",
                params![path_id.to_string(), checkout_id.to_string(), path, at],
            )?;
            path_id
        };
        transaction.execute(
            "INSERT INTO work_item_checkout_overrides (
                 work_item_id, repository_id, checkout_id, assigned_at
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(work_item_id, repository_id) DO UPDATE SET
                 checkout_id = excluded.checkout_id,
                 assigned_at = excluded.assigned_at",
            params![
                request.work_item_id.to_string(),
                request.repository_id.to_string(),
                checkout_id.to_string(),
                at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO checkout_readiness (
                 checkout_id, schema_version, repository_id, checkout_path_id,
                 purpose, access_mode, owner_kind, owner_id, session_id, session_key,
                 parent_feature_checkout_id, base_revision, source_revision, path,
                 git_worktree_identity, branch, head, availability,
                 isolation_generation, reconciliation_generation, evidence_json, observed_at
             ) VALUES (?1, ?2, ?3, ?4, 'work_item_write', 'write_isolated',
                       'work_item', ?5, NULL, '', ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       'available', ?13, ?14, ?15, ?16)
             ON CONFLICT(checkout_id) DO UPDATE SET
                 checkout_path_id = excluded.checkout_path_id,
                 parent_feature_checkout_id = excluded.parent_feature_checkout_id,
                 base_revision = excluded.base_revision,
                 source_revision = excluded.source_revision,
                 path = excluded.path,
                 git_worktree_identity = excluded.git_worktree_identity,
                 branch = excluded.branch,
                 head = excluded.head,
                 availability = excluded.availability,
                 isolation_generation = excluded.isolation_generation,
                 reconciliation_generation = excluded.reconciliation_generation,
                 evidence_json = excluded.evidence_json,
                 observed_at = excluded.observed_at",
            params![
                checkout_id.to_string(),
                CHECKOUT_READINESS_SCHEMA_VERSION,
                request.repository_id.to_string(),
                checkout_path_id.to_string(),
                request.work_item_id.to_string(),
                context.parent_checkout_id.to_string(),
                context.base_revision,
                context.source_revision,
                path,
                identity,
                branch,
                resolved.head_oid,
                i64::try_from(isolation_generation)
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                i64::try_from(reconciliation_generation)
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                evidence_json,
                at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO checkout_reconciliation_events (
                 checkout_id, generation, availability, head, evidence_json, observed_at
             ) VALUES (?1, ?2, 'available', ?3, ?4, ?5)",
            params![
                checkout_id.to_string(),
                i64::try_from(reconciliation_generation)
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                resolved.head_oid,
                evidence_json,
                at,
            ],
        )?;
        transaction.execute(
            "UPDATE operation_intents
             SET status = 'completed', completed_at = ?2
             WHERE id = ?1 AND status IN ('pending', 'completed')",
            params![intent_id.to_string(), at],
        )?;
        Ok((checkout_id, checkout_path_id))
    })?;
    Ok(CheckoutReadiness {
        schema_version: CHECKOUT_READINESS_SCHEMA_VERSION,
        repository_id: request.repository_id,
        checkout_id,
        checkout_path_id,
        purpose: CheckoutPurpose::WorkItemWrite,
        access_mode: CheckoutAccessMode::WriteIsolated,
        owner: HierarchyOwner::WorkItem(request.work_item_id),
        session_id: None,
        parent_feature_checkout_id: Some(context.parent_checkout_id),
        base_revision: context.base_revision.clone(),
        source_revision: context.source_revision.clone(),
        path: resolved.path.clone(),
        git_worktree_identity: resolved.git_dir.clone(),
        branch: branch.map(str::to_owned),
        head: resolved.head_oid.clone(),
        availability: CheckoutAvailability::Available,
        isolation_generation,
        reconciliation_generation,
        evidence,
    })
}

fn record_unavailable(
    store: &mut SqliteStore,
    readiness: &CheckoutReadiness,
    observed_at: OffsetDateTime,
    detail: String,
) -> Result<(), AppError> {
    let generation = readiness.reconciliation_generation + 1;
    let evidence = vec![CheckoutReconciliationEvidence {
        kind: CheckoutEvidenceKind::AvailabilityCorrected,
        observed_at,
        detail,
    }];
    let evidence_json = serde_json::to_string(&evidence)?;
    let at = timestamp(observed_at);
    store.write(|transaction| {
        transaction.execute(
            "UPDATE checkouts SET availability = 'missing' WHERE id = ?1",
            [readiness.checkout_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE checkout_readiness
             SET availability = 'missing', reconciliation_generation = ?2,
                 evidence_json = ?3, observed_at = ?4
             WHERE checkout_id = ?1",
            params![
                readiness.checkout_id.to_string(),
                i64::try_from(generation).map_err(|error| AppError::Domain(error.to_string()))?,
                evidence_json,
                at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO checkout_reconciliation_events (
                 checkout_id, generation, availability, head, evidence_json, observed_at
             ) VALUES (?1, ?2, 'missing', ?3, ?4, ?5)",
            params![
                readiness.checkout_id.to_string(),
                i64::try_from(generation).map_err(|error| AppError::Domain(error.to_string()))?,
                readiness.head,
                evidence_json,
                at,
            ],
        )?;
        Ok(())
    })
}

fn parse_checkout_purpose(value: &str) -> Result<CheckoutPurpose, AppError> {
    match value {
        "feature_integration" => Ok(CheckoutPurpose::FeatureIntegration),
        "work_item_write" => Ok(CheckoutPurpose::WorkItemWrite),
        "writer_session" => Ok(CheckoutPurpose::WriterSession),
        "read_only_shared" => Ok(CheckoutPurpose::ReadOnlyShared),
        _ => Err(AppError::Domain(format!(
            "unknown checkout purpose: {value}"
        ))),
    }
}

fn parse_checkout_access_mode(value: &str) -> Result<CheckoutAccessMode, AppError> {
    match value {
        "write_isolated" => Ok(CheckoutAccessMode::WriteIsolated),
        "read_only_shared" => Ok(CheckoutAccessMode::ReadOnlyShared),
        _ => Err(AppError::Domain(format!(
            "unknown checkout access mode: {value}"
        ))),
    }
}

fn parse_readiness_owner(kind: &str, id: &str) -> Result<HierarchyOwner, AppError> {
    match kind {
        "epic" => Ok(HierarchyOwner::Epic(parse_id(id)?)),
        "feature" => Ok(HierarchyOwner::Feature(parse_id(id)?)),
        "work_item" => Ok(HierarchyOwner::WorkItem(parse_id(id)?)),
        _ => Err(AppError::Domain(format!(
            "unknown checkout owner kind: {kind}"
        ))),
    }
}

fn parse_checkout_availability(value: &str) -> Result<CheckoutAvailability, AppError> {
    match value {
        "available" => Ok(CheckoutAvailability::Available),
        "missing" => Ok(CheckoutAvailability::Missing),
        "deleted" => Ok(CheckoutAvailability::Deleted),
        "replaced" => Ok(CheckoutAvailability::Replaced),
        _ => Err(AppError::Domain(format!(
            "unknown checkout availability: {value}"
        ))),
    }
}

fn checkout_conflict(code: &str, message: String) -> AppError {
    AppError::CheckoutReconciliation {
        code: code.to_owned(),
        message,
    }
}

fn validate_request(request: &PrepareFeatureCheckout) -> Result<(), AppError> {
    if request.idempotency_key.trim().is_empty() {
        return Err(AppError::EmptyIdempotencyKey);
    }
    if !request.target.is_absolute() {
        return Err(AppError::WorktreePathNotAbsolute(request.target.clone()));
    }
    if request.branch.trim().is_empty() || request.start_point.trim().is_empty() {
        return Err(AppError::GitCommand {
            message: "branch and start point cannot be blank".to_owned(),
        });
    }
    Ok(())
}

fn validate_resolved(
    request: &PrepareFeatureCheckout,
    resolved: &ResolvedWorktree,
) -> Result<(), AppError> {
    let target = request
        .target
        .canonicalize()
        .unwrap_or_else(|_| request.target.clone());
    if !paths_equal(&target, &resolved.path) {
        return Err(AppError::CallerIdentityMismatch);
    }
    if resolved.branch.as_deref().map(short_branch) != Some(request.branch.as_str()) {
        return Err(AppError::GitCommand {
            message: format!("worktree is not on requested branch {}", request.branch),
        });
    }
    Ok(())
}

fn request_payload(request: &PrepareFeatureCheckout) -> Result<String, AppError> {
    serde_json::to_string(&serde_json::json!({
        "feature_id": request.feature_id,
        "repository_id": request.repository_id,
        "target": request.target,
        "branch": request.branch,
        "create_branch": request.create_branch,
        "start_point": request.start_point,
    }))
    .map_err(Into::into)
}

fn short_branch(value: &str) -> &str {
    value.strip_prefix("refs/heads/").unwrap_or(value)
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339 timestamps always format")
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
    use std::cell::Cell;
    use std::fs;
    use std::path::Path;

    use rusqlite::params;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        CheckoutAvailability, CheckoutPurpose, FeatureId, OperationIntentId, RepositoryId,
        WorkItemId, WorkspaceId,
    };

    use super::{
        AdoptFeatureCheckout, CheckoutService, PrepareFeatureCheckout, PrepareWorkItemCheckout,
        request_payload, timestamp, work_item_target,
    };
    use crate::AppError;
    use crate::git::{GitWorktreeCreator, GitWorktreeResolver, ResolvedWorktree};
    use crate::storage::SqliteStore;

    struct FakeGit {
        resolved: ResolvedWorktree,
        creates: Cell<usize>,
    }

    impl GitWorktreeResolver for FakeGit {
        fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError> {
            if super::paths_equal(path, &self.resolved.path) {
                Ok(self.resolved.clone())
            } else {
                Err(AppError::WorktreePathInvalid(path.to_path_buf()))
            }
        }
    }

    impl GitWorktreeCreator for FakeGit {
        fn recreate(
            &self,
            _repository: &Path,
            target: &Path,
            _branch: &str,
            _create_branch: bool,
            _start_point: &str,
        ) -> Result<ResolvedWorktree, AppError> {
            if !super::paths_equal(target, &self.resolved.path) {
                return Err(AppError::WorktreePathInvalid(target.to_path_buf()));
            }
            self.creates.set(self.creates.get() + 1);
            Ok(self.resolved.clone())
        }
    }

    struct WorkItemGit {
        parent: ResolvedWorktree,
        child: ResolvedWorktree,
        child_resolves: bool,
        creates: Cell<usize>,
    }

    impl GitWorktreeResolver for WorkItemGit {
        fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError> {
            if super::paths_equal(path, &self.parent.path) {
                Ok(self.parent.clone())
            } else if self.child_resolves && super::paths_equal(path, &self.child.path) {
                Ok(self.child.clone())
            } else {
                Err(AppError::WorktreePathInvalid(path.to_path_buf()))
            }
        }
    }

    impl GitWorktreeCreator for WorkItemGit {
        fn recreate(
            &self,
            _repository: &Path,
            target: &Path,
            _branch: &str,
            _create_branch: bool,
            _start_point: &str,
        ) -> Result<ResolvedWorktree, AppError> {
            if !super::paths_equal(target, &self.child.path) {
                return Err(AppError::WorktreePathInvalid(target.to_path_buf()));
            }
            fs::create_dir_all(target).map_err(AppError::GitIo)?;
            self.creates.set(self.creates.get() + 1);
            Ok(self.child.clone())
        }
    }

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        feature_id: FeatureId,
        repository_id: RepositoryId,
        repository_path: std::path::PathBuf,
        repository_common_dir: std::path::PathBuf,
        target: std::path::PathBuf,
        observed_at: OffsetDateTime,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let repository_path = directory.path().join("repository");
        let repository_common_dir = repository_path.join(".git");
        let target = directory.path().join("worktrees").join("feature-one");
        fs::create_dir(&repository_path).expect("repository path");
        fs::create_dir(directory.path().join("worktrees")).expect("worktree parent");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = workboard_core::EpicId::generate();
        let feature_id = FeatureId::generate();
        let now = "2026-08-27T12:00:00Z";
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
                     VALUES (?1, 'demo', 'Demo', ?2, ?3)",
                    params![workspace_id.to_string(), planning_repository_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory, default_branch,
                         is_planning_store, created_at
                     ) VALUES (?1, ?2, 'planning', 'Planning', 'planning.git', 'main', 1, ?4),
                              (?3, ?2, 'code', 'Code', ?5, 'main', 0, ?4)",
                    params![
                        planning_repository_id.to_string(),
                        workspace_id.to_string(),
                        repository_id.to_string(),
                        now,
                        repository_common_dir.to_string_lossy(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO repository_paths (
                         id, repository_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        workboard_core::RepositoryPathId::generate().to_string(),
                        repository_id.to_string(),
                        repository_path.to_string_lossy(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'launch', 'Launch', ?3)",
                    params![epic_id.to_string(), workspace_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'feature-one', 'Feature one', 'worktree_pending', ?3)",
                    params![feature_id.to_string(), epic_id.to_string(), now],
                )?;
                Ok(())
            })
            .expect("seed checkout fixture");
        Fixture {
            _directory: directory,
            store,
            feature_id,
            repository_id,
            repository_path,
            repository_common_dir,
            target,
            observed_at: OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)
                .expect("timestamp"),
        }
    }

    fn request(fixture: &Fixture, idempotency_key: &str) -> PrepareFeatureCheckout {
        PrepareFeatureCheckout {
            feature_id: fixture.feature_id,
            repository_id: fixture.repository_id,
            target: fixture.target.clone(),
            branch: "feature/one".to_owned(),
            create_branch: true,
            start_point: "main".to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            observed_at: fixture.observed_at,
        }
    }

    fn fake_git(fixture: &Fixture) -> FakeGit {
        FakeGit {
            resolved: ResolvedWorktree {
                path: fixture.target.clone(),
                common_dir: fixture.repository_common_dir.clone(),
                git_dir: fixture.target.join(".git-worktrees-feature-one"),
                branch: Some("refs/heads/feature/one".to_owned()),
                head_oid: "0123456789abcdef".to_owned(),
            },
            creates: Cell::new(0),
        }
    }

    fn seed_work_item(fixture: &mut Fixture) -> WorkItemId {
        let work_item_id = WorkItemId::generate();
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at
                     ) VALUES (?1, ?2, 'feature-one/checkout', 'checkout',
                               'Checkout', 'ready', '2026-08-27T12:00:00Z')",
                    params![work_item_id.to_string(), fixture.feature_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![work_item_id.to_string(), fixture.repository_id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed Work item");
        work_item_id
    }

    fn work_item_git(
        fixture: &Fixture,
        work_item_id: WorkItemId,
        child_identity: &str,
    ) -> WorkItemGit {
        let parent = fake_git(fixture).resolved;
        let child_path = work_item_target(&fixture.repository_path, work_item_id)
            .expect("derived Work-item target");
        WorkItemGit {
            parent,
            child: ResolvedWorktree {
                path: child_path.clone(),
                common_dir: fixture.repository_common_dir.clone(),
                git_dir: child_path.join(child_identity),
                branch: Some(format!("refs/heads/work-item/{work_item_id}")),
                head_oid: "0123456789abcdef".to_owned(),
            },
            child_resolves: true,
            creates: Cell::new(0),
        }
    }

    #[test]
    fn creates_once_and_reuses_the_feature_checkout() {
        let mut fixture = fixture();
        let git = fake_git(&fixture);
        let first_request = request(&fixture, "feature-checkout-one");
        let repeated_request = request(&fixture, "feature-checkout-two");
        let first = CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(first_request, &git)
            .expect("prepare checkout");
        let repeated = CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(repeated_request, &git)
            .expect("reuse checkout");

        assert!(!first.reused);
        assert!(repeated.reused);
        assert_eq!(first.checkout_id, repeated.checkout_id);
        assert_eq!(git.creates.get(), 1);
    }

    #[test]
    fn materializes_and_reconciles_one_isolated_work_item_checkout() {
        let mut fixture = fixture();
        let feature_git = fake_git(&fixture);
        let feature_request = request(&fixture, "feature-checkout");
        CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(feature_request, &feature_git)
            .expect("prepare Feature checkout");
        let work_item_id = seed_work_item(&mut fixture);
        let git = work_item_git(&fixture, work_item_id, ".git-worktrees-work-item");
        let first = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "work-item-checkout-one".to_owned(),
                    observed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("prepare Work-item checkout");
        let repeated = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "work-item-checkout-two".to_owned(),
                    observed_at: fixture.observed_at + time::Duration::seconds(1),
                },
                &git,
            )
            .expect("reconcile Work-item checkout");
        let inherited = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT inherited FROM effective_work_item_checkouts
                         WHERE work_item_id = ?1 AND repository_id = ?2",
                        params![work_item_id.to_string(), fixture.repository_id.to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("effective isolated checkout");

        assert_eq!(first.purpose, CheckoutPurpose::WorkItemWrite);
        assert_eq!(first.availability, CheckoutAvailability::Available);
        assert_ne!(first.checkout_id, first.parent_feature_checkout_id.unwrap());
        assert_eq!(first.isolation_generation, 1);
        assert_eq!(repeated.isolation_generation, 1);
        assert_eq!(repeated.reconciliation_generation, 2);
        assert_eq!(git.creates.get(), 1);
        assert_eq!(inherited, 0);
    }

    #[test]
    fn materializes_a_truly_empty_derived_target() {
        let mut fixture = fixture();
        let feature_git = fake_git(&fixture);
        let feature_request = request(&fixture, "feature-checkout");
        CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(feature_request, &feature_git)
            .expect("prepare Feature checkout");
        let work_item_id = seed_work_item(&mut fixture);
        let mut git = work_item_git(&fixture, work_item_id, ".git-worktrees-work-item");
        git.child_resolves = false;
        fs::create_dir_all(&git.child.path).expect("empty derived target");

        let readiness = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "empty-work-item-checkout".to_owned(),
                    observed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("materialize empty target");

        assert_eq!(readiness.path, git.child.path);
        assert_eq!(git.creates.get(), 1);
    }

    #[test]
    fn occupied_derived_target_creates_no_checkout() {
        let mut fixture = fixture();
        let feature_git = fake_git(&fixture);
        let feature_request = request(&fixture, "feature-checkout");
        CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(feature_request, &feature_git)
            .expect("prepare Feature checkout");
        let work_item_id = seed_work_item(&mut fixture);
        let mut git = work_item_git(&fixture, work_item_id, ".git-worktrees-work-item");
        git.child_resolves = false;
        fs::create_dir_all(&git.child.path).expect("occupied derived target");
        fs::write(git.child.path.join("occupied.txt"), "occupied").expect("occupied marker");

        let error = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "occupied-work-item-checkout".to_owned(),
                    observed_at: fixture.observed_at,
                },
                &git,
            )
            .expect_err("occupied target must fail closed");
        let overrides = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM work_item_checkout_overrides
                         WHERE work_item_id = ?1",
                        [work_item_id.to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("override count");

        assert_eq!(error.code(), "checkout_target_occupied");
        assert_eq!(git.creates.get(), 0);
        assert_eq!(overrides, 0);
    }

    #[test]
    fn corrects_false_availability_when_git_identity_drifts() {
        let mut fixture = fixture();
        let feature_git = fake_git(&fixture);
        let feature_request = request(&fixture, "feature-checkout");
        CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(feature_request, &feature_git)
            .expect("prepare Feature checkout");
        let work_item_id = seed_work_item(&mut fixture);
        let git = work_item_git(&fixture, work_item_id, ".git-worktrees-work-item");
        let readiness = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "work-item-checkout".to_owned(),
                    observed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("prepare Work-item checkout");
        let drifted = work_item_git(&fixture, work_item_id, ".git-worktrees-drifted");
        let error = CheckoutService::new(&mut fixture.store)
            .prepare_work_item_with(
                PrepareWorkItemCheckout {
                    work_item_id,
                    repository_id: fixture.repository_id,
                    idempotency_key: "work-item-checkout-reconcile".to_owned(),
                    observed_at: fixture.observed_at + time::Duration::seconds(1),
                },
                &drifted,
            )
            .expect_err("identity drift must fail closed");
        let availability = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT availability FROM checkouts WHERE id = ?1",
                        [readiness.checkout_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("corrected availability");

        assert_eq!(error.code(), "checkout_identity_drift");
        assert_eq!(availability, "missing");
    }

    #[test]
    fn explicitly_adopts_an_existing_checkout_for_an_imported_feature_once() {
        let mut fixture = fixture();
        let git = fake_git(&fixture);
        let prepare_request = request(&fixture, "feature-checkout");
        let prepared = CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(prepare_request, &git)
            .expect("prepare source checkout");
        let imported_feature_id = FeatureId::generate();
        let epic_id = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT epic_id FROM features WHERE id = ?1",
                        [fixture.feature_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("Epic ID");
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'imported', 'Imported', 'planned', '2026-08-27T12:00:00Z')",
                    params![imported_feature_id.to_string(), epic_id],
                )?;
                Ok(())
            })
            .expect("seed imported Feature");
        let adoption = AdoptFeatureCheckout {
            feature_id: imported_feature_id,
            checkout_id: prepared.checkout_id,
            idempotency_key: "adopt-imported-checkout".to_owned(),
            observed_at: fixture.observed_at,
        };
        let first = CheckoutService::new(&mut fixture.store)
            .adopt_feature_checkout(adoption.clone())
            .expect("adopt checkout");
        let repeated = CheckoutService::new(&mut fixture.store)
            .adopt_feature_checkout(adoption)
            .expect("repeat adoption");

        assert_eq!(first.checkout_id, prepared.checkout_id);
        assert!(!first.reused);
        assert!(repeated.reused);
    }

    #[test]
    fn reconciles_a_worktree_created_before_database_completion() {
        let mut fixture = fixture();
        fs::create_dir(&fixture.target).expect("external worktree boundary");
        let git = fake_git(&fixture);
        let request = request(&fixture, "interrupted-checkout");
        let intent_id = OperationIntentId::generate();
        fixture
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO operation_intents (
                         id, feature_id, idempotency_key, kind, status, payload_json, created_at
                     ) VALUES (?1, ?2, ?3, 'feature_checkout', 'pending', ?4, ?5)",
                    params![
                        intent_id.to_string(),
                        request.feature_id.to_string(),
                        request.idempotency_key,
                        request_payload(&request)?,
                        timestamp(request.observed_at),
                    ],
                )?;
                Ok(())
            })
            .expect("seed interrupted intent");
        let outcome = CheckoutService::new(&mut fixture.store)
            .prepare_feature_with(request, &git)
            .expect("reconcile checkout");
        let status = fixture
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT status FROM operation_intents WHERE id = ?1",
                        [intent_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("intent status");

        assert!(!outcome.reused);
        assert_eq!(git.creates.get(), 0);
        assert_eq!(status, "completed");
    }
}
