use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_core::{CheckoutId, FeatureId, RepositoryId, WorkItemId};

use crate::AppError;
use crate::git::{GitCli, GitWorktreeResolver, ResolvedWorktree};
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrateFeatureBranches {
    pub feature_id: FeatureId,
    pub repository_id: RepositoryId,
    pub idempotency_key: String,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmFeatureIntegration {
    pub run_id: String,
    pub confirmation_token: String,
    pub confirmed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureIntegrationPreview {
    pub run: FeatureIntegrationOutcome,
    pub confirmation_token: String,
    pub confirmation_expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureIntegrationStep {
    pub position: u32,
    pub work_item_id: WorkItemId,
    pub source_checkout_id: CheckoutId,
    pub dependency_layer: u32,
    pub expected_target_head: String,
    pub source_head: String,
    pub result_head: Option<String>,
    pub status: String,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureIntegrationOutcome {
    pub run_id: String,
    pub feature_id: FeatureId,
    pub repository_id: RepositoryId,
    pub feature_checkout_id: CheckoutId,
    pub status: String,
    pub expected_target_head: String,
    pub result_head: Option<String>,
    pub failure: Option<String>,
    pub steps: Vec<FeatureIntegrationStep>,
}

struct IntegrationCandidate {
    work_item_id: WorkItemId,
    source_checkout_id: CheckoutId,
    source_path: PathBuf,
    source_head: String,
    dependency_layer: u32,
    proposal_order: u32,
    slug: String,
}

struct IntegrationTarget {
    checkout_id: CheckoutId,
    path: PathBuf,
    head: String,
}

type IntegrationGraph = (HashSet<WorkItemId>, Vec<(WorkItemId, WorkItemId)>);

trait IntegrationGit {
    fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError>;
    fn contains(&self, target: &Path, source_head: &str) -> Result<bool, AppError>;
    fn merge(&self, target: &Path, source_head: &str) -> Result<ResolvedWorktree, AppError>;
}

struct SystemIntegrationGit;

impl IntegrationGit for SystemIntegrationGit {
    fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError> {
        GitCli.resolve(path)
    }

    fn contains(&self, target: &Path, source_head: &str) -> Result<bool, AppError> {
        let status = Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["merge-base", "--is-ancestor", source_head, "HEAD"])
            .status()
            .map_err(AppError::GitIo)?;
        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(AppError::GitCommand {
                message: format!("Git ancestry verification exited with {status}"),
            }),
        }
    }

    fn merge(&self, target: &Path, source_head: &str) -> Result<ResolvedWorktree, AppError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["merge", "--no-ff", "--no-edit", "--", source_head])
            .output()
            .map_err(AppError::GitIo)?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(AppError::GitCommand {
                message: if message.is_empty() {
                    format!("Git merge exited with {}", output.status)
                } else {
                    message
                },
            });
        }
        GitCli.resolve(target)
    }
}

pub struct FeatureIntegrationService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> FeatureIntegrationService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn preview(
        &mut self,
        request: IntegrateFeatureBranches,
    ) -> Result<FeatureIntegrationPreview, AppError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(AppError::EmptyIdempotencyKey);
        }
        if let Some(existing) = self.run_for_idempotency(&request.idempotency_key)? {
            if existing.feature_id != request.feature_id
                || existing.repository_id != request.repository_id
            {
                return Err(AppError::IdempotencyConflict);
            }
            return Err(AppError::DuplicateConfirmed);
        }
        let target = self.target(request.feature_id, request.repository_id)?;
        let candidates = self.candidates(request.feature_id, request.repository_id)?;
        if candidates.is_empty() {
            return Err(AppError::External {
                code: "integration_queue_empty".to_owned(),
                message: "the Feature has no pending accepted branches for this repository"
                    .to_owned(),
            });
        }
        let run_id = Uuid::new_v4().to_string();
        let confirmation_token = Uuid::new_v4().to_string();
        let confirmation_expires_at = request.observed_at + time::Duration::minutes(5);
        let at = timestamp(request.observed_at);
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO feature_integration_runs (
                     id, feature_id, repository_id, feature_checkout_id, idempotency_key,
                     status, expected_target_head, confirmation_token_hash,
                     confirmation_expires_at, started_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'previewed', ?6, ?7, ?8, ?9)",
                params![
                    run_id,
                    request.feature_id.to_string(),
                    request.repository_id.to_string(),
                    target.checkout_id.to_string(),
                    request.idempotency_key,
                    target.head,
                    token_hash(&confirmation_token),
                    timestamp(confirmation_expires_at),
                    at,
                ],
            )?;
            for (position, candidate) in candidates.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO feature_integration_steps (
                         run_id, position, work_item_id, source_checkout_id, dependency_layer,
                         expected_target_head, source_head, status, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
                    params![
                        run_id,
                        i64::try_from(position)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                        candidate.work_item_id.to_string(),
                        candidate.source_checkout_id.to_string(),
                        i64::from(candidate.dependency_layer),
                        target.head,
                        candidate.source_head,
                        at,
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(FeatureIntegrationPreview {
            run: self.read_run(&run_id)?,
            confirmation_token,
            confirmation_expires_at,
        })
    }

    pub fn confirm(
        &mut self,
        request: ConfirmFeatureIntegration,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        self.confirm_with(request, &SystemIntegrationGit)
    }

    fn confirm_with(
        &mut self,
        request: ConfirmFeatureIntegration,
        git: &impl IntegrationGit,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        let confirmed_at = timestamp(request.confirmed_at);
        let updated = self.store.write(|transaction| {
            let run = transaction
                .query_row(
                    "SELECT feature_id, repository_id FROM feature_integration_runs
                     WHERE id = ?1 AND status = 'previewed'
                       AND confirmation_token_hash = ?2 AND confirmation_expires_at > ?3",
                    params![
                        request.run_id,
                        token_hash(&request.confirmation_token),
                        confirmed_at,
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((feature_id, repository_id)) = run else {
                return Ok(0);
            };
            let active = transaction.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM feature_integration_runs
                     WHERE feature_id = ?1 AND repository_id = ?2 AND status = 'running'
                 )",
                params![feature_id, repository_id],
                |row| row.get::<_, i64>(0),
            )?;
            if active != 0 {
                return Err(AppError::External {
                    code: "feature_integration_lease_held".to_owned(),
                    message: "another integration run holds the Feature checkout lease".to_owned(),
                });
            }
            transaction
                .execute(
                    "UPDATE feature_integration_runs
                     SET status = 'running', confirmed_at = ?2 WHERE id = ?1",
                    params![request.run_id, confirmed_at],
                )
                .map_err(Into::into)
        })?;
        if updated != 1 {
            return Err(AppError::External {
                code: "integration_confirmation_invalid".to_owned(),
                message: "the integration confirmation is missing, expired, consumed, or changed"
                    .to_owned(),
            });
        }
        self.execute_run(request.run_id, request.confirmed_at, git)
    }

    pub fn resume(
        &mut self,
        run_id: &str,
        observed_at: OffsetDateTime,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        let outcome = self.read_run(run_id)?;
        if outcome.status == "running" {
            self.execute_run(run_id.to_owned(), observed_at, &SystemIntegrationGit)
        } else {
            Ok(outcome)
        }
    }

    pub fn outcome(&self, run_id: &str) -> Result<FeatureIntegrationOutcome, AppError> {
        self.read_run(run_id)
    }

    fn execute_run(
        &mut self,
        run_id: String,
        observed_at: OffsetDateTime,
        git: &impl IntegrationGit,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        let outcome = self.read_run(&run_id)?;
        let target = self.target(outcome.feature_id, outcome.repository_id)?;
        let resolved_target = git.resolve(&target.path)?;
        if resolved_target.head_oid != target.head {
            let pending_source = outcome
                .steps
                .iter()
                .find(|step| step.status == "pending")
                .map(|step| step.source_head.as_str());
            if pending_source.is_none()
                || !git.contains(&target.path, pending_source.expect("pending source"))?
            {
                return self.fail_run(
                    &run_id,
                    "feature integration checkout head drifted outside the recorded run",
                    observed_at,
                );
            }
        }
        for step in outcome.steps.iter().filter(|step| step.status == "pending") {
            let candidate = self.candidate(step.work_item_id, outcome.repository_id)?;
            let source = git.resolve(&candidate.source_path)?;
            if source.head_oid != step.source_head {
                return self.fail_step(
                    &run_id,
                    step.position,
                    step.work_item_id,
                    outcome.repository_id,
                    "source checkout head changed after integration was queued",
                    observed_at,
                );
            }
            let current = git.resolve(&target.path)?;
            self.record_expected_target(&run_id, step.position, &current.head_oid, observed_at)?;
            if git.contains(&target.path, &step.source_head)? {
                self.complete_step(
                    &run_id,
                    step.position,
                    step.work_item_id,
                    outcome.repository_id,
                    &current.head_oid,
                    observed_at,
                )?;
                continue;
            }
            match git.merge(&target.path, &step.source_head) {
                Ok(merged) => self.complete_step(
                    &run_id,
                    step.position,
                    step.work_item_id,
                    outcome.repository_id,
                    &merged.head_oid,
                    observed_at,
                )?,
                Err(error) => {
                    return self.fail_step(
                        &run_id,
                        step.position,
                        step.work_item_id,
                        outcome.repository_id,
                        &error.to_string(),
                        observed_at,
                    );
                }
            }
        }
        let result_head = git.resolve(&target.path)?.head_oid;
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE feature_integration_runs
                 SET status = 'completed', result_head = ?2, completed_at = ?3
                 WHERE id = ?1 AND status = 'running'",
                params![run_id, result_head, timestamp(observed_at)],
            )?;
            Ok(())
        })?;
        self.read_run(&run_id)
    }

    fn complete_step(
        &mut self,
        run_id: &str,
        position: u32,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
        result_head: &str,
        observed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        let at = timestamp(observed_at);
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE feature_integration_steps
                 SET status = 'integrated', result_head = ?3, conflict = NULL, updated_at = ?4
                 WHERE run_id = ?1 AND position = ?2 AND status = 'pending'",
                params![run_id, i64::from(position), result_head, at],
            )?;
            transaction.execute(
                "UPDATE work_item_integrations
                 SET status = 'integrated', integration_run_id = ?3,
                     expected_target_head = (
                         SELECT expected_target_head FROM feature_integration_steps
                         WHERE run_id = ?3 AND position = ?4
                     ), result_head = ?5, conflict = NULL, updated_at = ?6
                 WHERE work_item_id = ?1 AND repository_id = ?2",
                params![
                    work_item_id.to_string(),
                    repository_id.to_string(),
                    run_id,
                    i64::from(position),
                    result_head,
                    at,
                ],
            )?;
            transaction.execute(
                "UPDATE checkouts SET head = ?2 WHERE id = (
                     SELECT feature_checkout_id FROM feature_integration_runs WHERE id = ?1
                 )",
                params![run_id, result_head],
            )?;
            transaction.execute(
                "UPDATE checkout_readiness
                 SET head = ?2, reconciliation_generation = reconciliation_generation + 1,
                     observed_at = ?3
                 WHERE checkout_id = (
                     SELECT feature_checkout_id FROM feature_integration_runs WHERE id = ?1
                 )",
                params![run_id, result_head, at],
            )?;
            transaction.execute(
                "UPDATE work_items
                 SET status = 'done'
                 WHERE id = ?1 AND status = 'review'
                   AND NOT EXISTS (
                       SELECT 1 FROM work_item_integrations
                       WHERE work_item_id = ?1 AND status <> 'integrated'
                   )",
                [work_item_id.to_string()],
            )?;
            Ok(())
        })
    }

    fn record_expected_target(
        &mut self,
        run_id: &str,
        position: u32,
        head: &str,
        observed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE feature_integration_steps
                 SET expected_target_head = ?3, updated_at = ?4
                 WHERE run_id = ?1 AND position = ?2 AND status = 'pending'",
                params![run_id, i64::from(position), head, timestamp(observed_at)],
            )?;
            Ok(())
        })
    }

    fn fail_step(
        &mut self,
        run_id: &str,
        position: u32,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
        failure: &str,
        observed_at: OffsetDateTime,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        let at = timestamp(observed_at);
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE feature_integration_steps
                 SET status = 'conflict', conflict = ?3, updated_at = ?4
                 WHERE run_id = ?1 AND position = ?2 AND status = 'pending'",
                params![run_id, i64::from(position), failure, at],
            )?;
            transaction.execute(
                "UPDATE feature_integration_steps
                 SET status = 'skipped', conflict = 'earlier integration step conflicted',
                     updated_at = ?3
                 WHERE run_id = ?1 AND position > ?2 AND status = 'pending'",
                params![run_id, i64::from(position), at],
            )?;
            transaction.execute(
                "UPDATE work_item_integrations
                 SET status = 'conflict', integration_run_id = ?2, conflict = ?3, updated_at = ?4
                 WHERE work_item_id = ?1 AND repository_id = ?5",
                params![
                    work_item_id.to_string(),
                    run_id,
                    failure,
                    at,
                    repository_id.to_string(),
                ],
            )?;
            transaction.execute(
                "UPDATE feature_integration_runs
                 SET status = 'conflict', failure = ?2, completed_at = ?3 WHERE id = ?1",
                params![run_id, failure, at],
            )?;
            Ok(())
        })?;
        self.read_run(run_id)
    }

    fn fail_run(
        &mut self,
        run_id: &str,
        failure: &str,
        observed_at: OffsetDateTime,
    ) -> Result<FeatureIntegrationOutcome, AppError> {
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE feature_integration_runs
                 SET status = 'failed', failure = ?2, completed_at = ?3 WHERE id = ?1",
                params![run_id, failure, timestamp(observed_at)],
            )?;
            Ok(())
        })?;
        self.read_run(run_id)
    }

    fn target(
        &self,
        feature_id: FeatureId,
        repository_id: RepositoryId,
    ) -> Result<IntegrationTarget, AppError> {
        self.store.read(|connection| {
            let target = connection
                .query_row(
                    "SELECT checkout.id, path.path, checkout.head
                     FROM feature_checkouts feature_checkout
                     JOIN checkouts checkout ON checkout.id = feature_checkout.checkout_id
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     WHERE feature_checkout.feature_id = ?1
                       AND feature_checkout.repository_id = ?2
                       AND checkout.availability = 'available' AND checkout.head IS NOT NULL",
                    params![feature_id.to_string(), repository_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(AppError::ResumeCheckoutRequired)?;
            Ok(IntegrationTarget {
                checkout_id: parse_id(&target.0)?,
                path: PathBuf::from(target.1),
                head: target.2,
            })
        })
    }

    fn candidates(
        &self,
        feature_id: FeatureId,
        repository_id: RepositoryId,
    ) -> Result<Vec<IntegrationCandidate>, AppError> {
        let (items, edges) = self.integration_graph(feature_id)?;
        let mut memo = HashMap::new();
        let mut candidates = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT integration.work_item_id, integration.source_checkout_id,
                        path.path, integration.source_head, item.proposal_order, item.slug
                 FROM work_item_integrations integration
                 JOIN work_items item ON item.id = integration.work_item_id
                 JOIN checkout_paths path
                   ON path.checkout_id = integration.source_checkout_id
                  AND path.observed_until IS NULL
                 WHERE item.feature_id = ?1 AND integration.repository_id = ?2
                   AND integration.status IN ('pending', 'conflict')
                   AND item.status IN ('review', 'done')",
            )?;
            statement
                .query_map(
                    params![feature_id.to_string(), repository_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, u32>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )?
                .map(|row| {
                    let (work_item_id, checkout_id, path, head, proposal_order, slug) = row?;
                    let work_item_id = parse_id(&work_item_id)?;
                    Ok(IntegrationCandidate {
                        work_item_id,
                        source_checkout_id: parse_id(&checkout_id)?,
                        source_path: PathBuf::from(path),
                        source_head: head,
                        dependency_layer: dependency_layer(
                            work_item_id,
                            &edges,
                            &mut memo,
                            &mut HashSet::new(),
                        )?,
                        proposal_order,
                        slug,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()
        })?;
        candidates.retain(|candidate| items.contains(&candidate.work_item_id));
        candidates.sort_by(|left, right| {
            (
                left.dependency_layer,
                left.proposal_order,
                left.slug.as_str(),
            )
                .cmp(&(
                    right.dependency_layer,
                    right.proposal_order,
                    right.slug.as_str(),
                ))
        });
        Ok(candidates)
    }

    fn candidate(
        &self,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
    ) -> Result<IntegrationCandidate, AppError> {
        let feature_id = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT feature_id FROM work_items WHERE id = ?1",
                    [work_item_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
        })?;
        self.candidates(parse_id(&feature_id)?, repository_id)?
            .into_iter()
            .find(|candidate| candidate.work_item_id == work_item_id)
            .ok_or(AppError::WorkItemNotFound)
    }

    fn integration_graph(&self, feature_id: FeatureId) -> Result<IntegrationGraph, AppError> {
        self.store.read(|connection| {
            let mut item_statement =
                connection.prepare("SELECT id FROM work_items WHERE feature_id = ?1")?;
            let items = item_statement
                .query_map([feature_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| parse_id(&row?))
                .collect::<Result<HashSet<_>, AppError>>()?;
            let mut edge_statement = connection.prepare(
                "SELECT edge.work_item_id, edge.dependency_work_item_id
                 FROM work_item_dependencies edge
                 JOIN work_items item ON item.id = edge.work_item_id
                 WHERE item.feature_id = ?1",
            )?;
            let edges = edge_statement
                .query_map([feature_id.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .map(|row| {
                    let (work_item, dependency) = row?;
                    Ok((parse_id(&work_item)?, parse_id(&dependency)?))
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok((items, edges))
        })
    }

    fn run_for_idempotency(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<FeatureIntegrationOutcome>, AppError> {
        let run_id = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT id FROM feature_integration_runs WHERE idempotency_key = ?1",
                    [idempotency_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(Into::into)
        })?;
        run_id
            .as_deref()
            .map(|run_id| self.read_run(run_id))
            .transpose()
    }

    fn read_run(&self, run_id: &str) -> Result<FeatureIntegrationOutcome, AppError> {
        self.store.read(|connection| {
            let (feature_id, repository_id, checkout_id, status, expected, result, failure) =
                connection
                    .query_row(
                        "SELECT feature_id, repository_id, feature_checkout_id, status,
                                expected_target_head, result_head, failure
                         FROM feature_integration_runs WHERE id = ?1",
                        [run_id],
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
                    .optional()?
                    .ok_or_else(|| AppError::External {
                        code: "integration_run_not_found".to_owned(),
                        message: "the Feature integration run does not exist".to_owned(),
                    })?;
            let mut statement = connection.prepare(
                "SELECT position, work_item_id, source_checkout_id, dependency_layer,
                        expected_target_head, source_head, result_head, status, conflict
                 FROM feature_integration_steps WHERE run_id = ?1 ORDER BY position",
            )?;
            let steps = statement
                .query_map([run_id], |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                })?
                .map(|row| {
                    let (
                        position,
                        work_item_id,
                        checkout_id,
                        dependency_layer,
                        expected_target_head,
                        source_head,
                        result_head,
                        status,
                        conflict,
                    ) = row?;
                    Ok(FeatureIntegrationStep {
                        position,
                        work_item_id: parse_id(&work_item_id)?,
                        source_checkout_id: parse_id(&checkout_id)?,
                        dependency_layer,
                        expected_target_head,
                        source_head,
                        result_head,
                        status,
                        conflict,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(FeatureIntegrationOutcome {
                run_id: run_id.to_owned(),
                feature_id: parse_id(&feature_id)?,
                repository_id: parse_id(&repository_id)?,
                feature_checkout_id: parse_id(&checkout_id)?,
                status,
                expected_target_head: expected,
                result_head: result,
                failure,
                steps,
            })
        })
    }
}

fn dependency_layer(
    work_item_id: WorkItemId,
    edges: &[(WorkItemId, WorkItemId)],
    memo: &mut HashMap<WorkItemId, u32>,
    visiting: &mut HashSet<WorkItemId>,
) -> Result<u32, AppError> {
    if let Some(layer) = memo.get(&work_item_id) {
        return Ok(*layer);
    }
    if !visiting.insert(work_item_id) {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item dependencies must be acyclic".to_owned(),
        ));
    }
    let mut layer = 0;
    for (_, dependency) in edges.iter().filter(|(item, _)| *item == work_item_id) {
        layer = layer.max(dependency_layer(*dependency, edges, memo, visiting)?.saturating_add(1));
    }
    visiting.remove(&work_item_id);
    memo.insert(work_item_id, layer);
    Ok(layer)
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
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
    use std::cell::RefCell;

    use rusqlite::params;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        CheckoutId, DocumentId, EpicId, FeatureId, RepositoryId, WorkItemId, WorkItemStatus,
        WorkspaceId,
    };

    use super::{
        ConfirmFeatureIntegration, FeatureIntegrationService, IntegrateFeatureBranches,
        IntegrationGit, ResolvedWorktree,
    };
    use crate::AppError;
    use crate::storage::SqliteStore;
    use crate::work_projection::WorkProjectionService;

    struct FakeGit {
        target_path: std::path::PathBuf,
        target_head: RefCell<String>,
        source_heads: Vec<(std::path::PathBuf, String)>,
        merged: RefCell<Vec<String>>,
        fail_source: Option<String>,
    }

    impl IntegrationGit for FakeGit {
        fn resolve(&self, path: &std::path::Path) -> Result<ResolvedWorktree, AppError> {
            let head = if path == self.target_path {
                self.target_head.borrow().clone()
            } else {
                self.source_heads
                    .iter()
                    .find(|(candidate, _)| candidate == path)
                    .map(|(_, head)| head.clone())
                    .ok_or_else(|| AppError::WorktreePathInvalid(path.to_path_buf()))?
            };
            Ok(ResolvedWorktree {
                path: path.to_path_buf(),
                common_dir: "C:/repo/.git".into(),
                git_dir: path.join(".git-worktree"),
                branch: Some("refs/heads/test".to_owned()),
                head_oid: head,
            })
        }

        fn contains(&self, _target: &std::path::Path, source_head: &str) -> Result<bool, AppError> {
            Ok(self.merged.borrow().iter().any(|head| head == source_head))
        }

        fn merge(
            &self,
            target: &std::path::Path,
            source_head: &str,
        ) -> Result<ResolvedWorktree, AppError> {
            if self.fail_source.as_deref() == Some(source_head) {
                return Err(AppError::GitCommand {
                    message: format!("conflict while merging {source_head}"),
                });
            }
            self.merged.borrow_mut().push(source_head.to_owned());
            let result = format!("{}+{source_head}", self.target_head.borrow());
            *self.target_head.borrow_mut() = result;
            self.resolve(target)
        }
    }

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        feature_id: FeatureId,
        repository_id: RepositoryId,
        root_id: WorkItemId,
        middle_id: WorkItemId,
        leaf_id: WorkItemId,
        target_path: std::path::PathBuf,
        root_path: std::path::PathBuf,
        middle_path: std::path::PathBuf,
        observed_at: OffsetDateTime,
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let root_id = WorkItemId::generate();
        let middle_id = WorkItemId::generate();
        let leaf_id = WorkItemId::generate();
        let feature_checkout_id = CheckoutId::generate();
        let root_checkout_id = CheckoutId::generate();
        let middle_checkout_id = CheckoutId::generate();
        let target_path = directory.path().join("feature");
        let root_path = directory.path().join("root");
        let middle_path = directory.path().join("middle");
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
                     ) VALUES (?1, 'integration', 'Integration', ?2, ?3)",
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
                        repository_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'integration', 'Integration', ?3)",
                    params![epic_id.to_string(), workspace_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'integration', 'Integration', 'planned', ?3)",
                    params![feature_id.to_string(), epic_id.to_string(), now],
                )?;
                for (position, work_item_id, slug, status) in [
                    (0, root_id, "root", "review"),
                    (1, middle_id, "middle", "review"),
                    (2, leaf_id, "leaf", "ready"),
                ] {
                    transaction.execute(
                        "INSERT INTO work_items (
                             id, feature_id, key, slug, title, status, created_at, proposal_order
                         ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)",
                        params![
                            work_item_id.to_string(),
                            feature_id.to_string(),
                            format!("integration/{slug}"),
                            slug,
                            status,
                            now,
                            position,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO work_item_repositories (work_item_id, repository_id)
                         VALUES (?1, ?2)",
                        params![work_item_id.to_string(), repository_id.to_string()],
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
                     ) VALUES (?1, ?2, 0), (?3, ?1, 0)",
                    params![
                        middle_id.to_string(),
                        root_id.to_string(),
                        leaf_id.to_string(),
                    ],
                )?;
                for (checkout_id, identity, branch, head, path) in [
                    (
                        feature_checkout_id,
                        "feature-identity",
                        "feature/integration",
                        "base",
                        &target_path,
                    ),
                    (
                        root_checkout_id,
                        "root-identity",
                        "work-item/root",
                        "root-head",
                        &root_path,
                    ),
                    (
                        middle_checkout_id,
                        "middle-identity",
                        "work-item/middle",
                        "middle-head",
                        &middle_path,
                    ),
                ] {
                    transaction.execute(
                        "INSERT INTO checkouts (
                             id, repository_id, git_worktree_identity, branch, head,
                             availability, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'available', ?6)",
                        params![
                            checkout_id.to_string(),
                            repository_id.to_string(),
                            identity,
                            branch,
                            head,
                            now,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO checkout_paths (
                             id, checkout_id, path, observed_from, observed_until
                         ) VALUES (?1, ?2, ?3, ?4, NULL)",
                        params![
                            workboard_core::CheckoutPathId::generate().to_string(),
                            checkout_id.to_string(),
                            path.to_string_lossy(),
                            now,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        feature_id.to_string(),
                        repository_id.to_string(),
                        feature_checkout_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_integrations (
                         work_item_id, repository_id, source_checkout_id, source_head,
                         status, updated_at
                     ) VALUES (?1, ?3, ?4, 'root-head', 'pending', ?6),
                              (?2, ?3, ?5, 'middle-head', 'pending', ?6)",
                    params![
                        root_id.to_string(),
                        middle_id.to_string(),
                        repository_id.to_string(),
                        root_checkout_id.to_string(),
                        middle_checkout_id.to_string(),
                        now,
                    ],
                )?;
                Ok(())
            })
            .expect("seed integration fixture");
        Fixture {
            _directory: directory,
            store,
            feature_id,
            repository_id,
            root_id,
            middle_id,
            leaf_id,
            target_path,
            root_path,
            middle_path,
            observed_at,
        }
    }

    fn fake_git(fixture: &Fixture, fail_source: Option<&str>) -> FakeGit {
        FakeGit {
            target_path: fixture.target_path.clone(),
            target_head: RefCell::new("base".to_owned()),
            source_heads: vec![
                (fixture.root_path.clone(), "root-head".to_owned()),
                (fixture.middle_path.clone(), "middle-head".to_owned()),
            ],
            merged: RefCell::new(Vec::new()),
            fail_source: fail_source.map(str::to_owned),
        }
    }

    #[test]
    fn integrates_dependency_layers_in_stable_order_and_records_every_head() {
        let mut fixture = fixture();
        let git = fake_git(&fixture, None);
        let preview = FeatureIntegrationService::new(&mut fixture.store)
            .preview(IntegrateFeatureBranches {
                feature_id: fixture.feature_id,
                repository_id: fixture.repository_id,
                idempotency_key: "integrate-success".to_owned(),
                observed_at: fixture.observed_at,
            })
            .expect("preview accepted branches");
        let outcome = FeatureIntegrationService::new(&mut fixture.store)
            .confirm_with(
                ConfirmFeatureIntegration {
                    run_id: preview.run.run_id,
                    confirmation_token: preview.confirmation_token,
                    confirmed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("integrate accepted branches");

        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.steps[0].work_item_id, fixture.root_id);
        assert_eq!(outcome.steps[0].expected_target_head, "base");
        assert_eq!(
            outcome.steps[0].result_head.as_deref(),
            Some("base+root-head")
        );
        assert_eq!(outcome.steps[1].work_item_id, fixture.middle_id);
        assert_eq!(outcome.steps[1].expected_target_head, "base+root-head");
        assert_eq!(
            outcome.steps[1].result_head.as_deref(),
            Some("base+root-head+middle-head")
        );
        assert_eq!(
            git.merged.borrow().as_slice(),
            ["root-head".to_owned(), "middle-head".to_owned()]
        );
        let root = WorkProjectionService::new(&fixture.store)
            .project(fixture.root_id)
            .expect("root projection");
        let middle = WorkProjectionService::new(&fixture.store)
            .project(fixture.middle_id)
            .expect("middle projection");
        assert_eq!(root.work_item.status, WorkItemStatus::Done);
        assert_eq!(middle.work_item.status, WorkItemStatus::Done);
        let leaf = WorkProjectionService::new(&fixture.store)
            .project(fixture.leaf_id)
            .expect("leaf projection");
        assert!(leaf.readiness.ready);
    }

    #[test]
    fn conflict_stops_ordered_integration_and_keeps_dependants_blocked() {
        let mut fixture = fixture();
        let git = fake_git(&fixture, Some("middle-head"));
        let preview = FeatureIntegrationService::new(&mut fixture.store)
            .preview(IntegrateFeatureBranches {
                feature_id: fixture.feature_id,
                repository_id: fixture.repository_id,
                idempotency_key: "integrate-conflict".to_owned(),
                observed_at: fixture.observed_at,
            })
            .expect("preview accepted branches");
        let outcome = FeatureIntegrationService::new(&mut fixture.store)
            .confirm_with(
                ConfirmFeatureIntegration {
                    run_id: preview.run.run_id,
                    confirmation_token: preview.confirmation_token,
                    confirmed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("record integration conflict");

        assert_eq!(outcome.status, "conflict");
        assert_eq!(outcome.steps[0].status, "integrated");
        assert_eq!(outcome.steps[1].status, "conflict");
        let root = WorkProjectionService::new(&fixture.store)
            .project(fixture.root_id)
            .expect("root projection");
        let middle = WorkProjectionService::new(&fixture.store)
            .project(fixture.middle_id)
            .expect("middle projection");
        assert_eq!(root.work_item.status, WorkItemStatus::Done);
        assert_eq!(middle.work_item.status, WorkItemStatus::Review);
        assert!(
            outcome.steps[1]
                .conflict
                .as_deref()
                .is_some_and(|message| message.contains("middle-head"))
        );
        let leaf = WorkProjectionService::new(&fixture.store)
            .project(fixture.leaf_id)
            .expect("leaf projection");
        assert!(!leaf.readiness.ready);
        assert_eq!(leaf.readiness.blocked_by, vec![fixture.middle_id]);
    }

    #[test]
    fn preview_requires_one_exact_confirmation_before_any_merge() {
        let mut fixture = fixture();
        let git = fake_git(&fixture, None);
        let preview = FeatureIntegrationService::new(&mut fixture.store)
            .preview(IntegrateFeatureBranches {
                feature_id: fixture.feature_id,
                repository_id: fixture.repository_id,
                idempotency_key: "integration-confirmation".to_owned(),
                observed_at: fixture.observed_at,
            })
            .expect("preview integration");
        assert_eq!(preview.run.status, "previewed");
        assert!(git.merged.borrow().is_empty());

        let error = FeatureIntegrationService::new(&mut fixture.store)
            .confirm_with(
                ConfirmFeatureIntegration {
                    run_id: preview.run.run_id.clone(),
                    confirmation_token: "wrong-token".to_owned(),
                    confirmed_at: fixture.observed_at,
                },
                &git,
            )
            .expect_err("wrong confirmation must not merge");
        assert_eq!(error.code(), "integration_confirmation_invalid");
        assert!(git.merged.borrow().is_empty());

        FeatureIntegrationService::new(&mut fixture.store)
            .confirm_with(
                ConfirmFeatureIntegration {
                    run_id: preview.run.run_id.clone(),
                    confirmation_token: preview.confirmation_token,
                    confirmed_at: fixture.observed_at,
                },
                &git,
            )
            .expect("confirm integration");
        let repeated = FeatureIntegrationService::new(&mut fixture.store)
            .confirm_with(
                ConfirmFeatureIntegration {
                    run_id: preview.run.run_id,
                    confirmation_token: "already-consumed".to_owned(),
                    confirmed_at: fixture.observed_at,
                },
                &git,
            )
            .expect_err("confirmation is one-shot");
        assert_eq!(repeated.code(), "integration_confirmation_invalid");
    }
}
