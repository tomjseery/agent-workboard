use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{CheckoutId, CheckoutPathId, FeatureId, OperationIntentId, RepositoryId};

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

pub struct CheckoutService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> CheckoutService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn prepare_feature(
        &mut self,
        request: PrepareFeatureCheckout,
    ) -> Result<FeatureCheckoutOutcome, AppError> {
        self.prepare_feature_with(request, &GitCli)
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
    if !paths_equal(&request.target, &resolved.path) {
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
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
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
    use workboard_core::{FeatureId, OperationIntentId, RepositoryId, WorkspaceId};

    use super::{CheckoutService, PrepareFeatureCheckout, request_payload, timestamp};
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

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        feature_id: FeatureId,
        repository_id: RepositoryId,
        repository_path: std::path::PathBuf,
        target: std::path::PathBuf,
        observed_at: OffsetDateTime,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let repository_path = directory.path().join("repository");
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
                common_dir: fixture.repository_path.join(".git"),
                git_dir: fixture.target.join(".git-worktrees-feature-one"),
                branch: Some("refs/heads/feature/one".to_owned()),
                head_oid: "0123456789abcdef".to_owned(),
            },
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
