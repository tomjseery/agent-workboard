use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    CheckoutId, DocumentId, DocumentKind, EpicId, FeatureId, HierarchyOwner, ManagedSessionRole,
    RepositoryId, Slug, WorkItemId, WorkItemKey, WorkItemStatus, WorkflowActor, WorkflowEventId,
    WorkflowRunId, WorkflowState,
};

use crate::AppError;
use crate::git::{GitCli, GitWorktreeResolver};
use crate::planning_store::{DocumentFrontMatter, NewPlanningDocument, PlanningStore};
use crate::storage::SqliteStore;

const MAX_PROPOSAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFeaturePlanning {
    pub epic_id: EpicId,
    pub repository_id: RepositoryId,
    pub slug: Slug,
    pub title: String,
    pub idempotency_key: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturePlanningDraft {
    pub feature_id: FeatureId,
    pub workflow_run_id: WorkflowRunId,
    pub epic_id: EpicId,
    pub repository_id: RepositoryId,
    pub slug: Slug,
    pub title: String,
    pub epic_content_hash: String,
    pub repository_head: String,
    pub state: WorkflowState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureProposal {
    pub feature_body: String,
    pub work_items: Vec<ProposedWorkItem>,
    pub expected_epic_content_hash: String,
    pub expected_repository_head: String,
    pub verification: Vec<String>,
    pub first_work_item_slug: Option<Slug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedWorkItem {
    pub slug: Slug,
    pub title: String,
    pub body: String,
    pub repository_ids: Vec<RepositoryId>,
    #[serde(default)]
    pub dependencies: Vec<Slug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredFeatureProposal {
    feature_document_id: DocumentId,
    proposal: FeatureProposal,
    work_items: Vec<StoredProposedWorkItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredProposedWorkItem {
    work_item_id: WorkItemId,
    document_id: DocumentId,
    proposal: ProposedWorkItem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureProposalOutcome {
    pub feature_id: FeatureId,
    pub workflow_run_id: WorkflowRunId,
    pub state: WorkflowState,
    pub work_item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturePublicationOutcome {
    pub feature_id: FeatureId,
    pub workflow_run_id: WorkflowRunId,
    pub commit: String,
    pub work_item_ids: Vec<WorkItemId>,
    pub first_work_item_id: Option<WorkItemId>,
}

pub struct PlanningWorkflowService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> PlanningWorkflowService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn create_feature(
        &mut self,
        request: CreateFeaturePlanning,
    ) -> Result<FeaturePlanningDraft, AppError> {
        validate_title(&request.title)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if let Some(existing) = self.draft_for_idempotency(&request.idempotency_key)? {
            if existing.epic_id != request.epic_id
                || existing.repository_id != request.repository_id
                || existing.slug != request.slug
                || existing.title != request.title
            {
                return Err(AppError::IdempotencyConflict);
            }
            return Ok(existing);
        }
        let source = self.source_context(request.epic_id, request.repository_id)?;
        let repository_head = GitCli.resolve(&source.repository_path)?.head_oid;
        let feature_id = FeatureId::generate();
        let workflow_run_id = WorkflowRunId::generate();
        let at = timestamp(request.created_at);
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'worktree_pending', ?5)",
                params![
                    feature_id.to_string(),
                    request.epic_id.to_string(),
                    request.slug.as_str(),
                    request.title,
                    at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO workflow_runs (id, feature_id, current_state, started_at)
                 VALUES (?1, ?2, 'worktree_pending', ?3)",
                params![workflow_run_id.to_string(), feature_id.to_string(), at],
            )?;
            append_event(
                transaction,
                workflow_run_id,
                WorkflowState::Draft,
                WorkflowState::WorktreePending,
                WorkflowActor::Application,
                request.created_at,
                "{}",
                Some(&request.idempotency_key),
            )?;
            transaction.execute(
                "INSERT INTO feature_planning_contexts (
                     feature_id, workflow_run_id, idempotency_key, repository_id,
                     epic_content_hash, repository_head, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    feature_id.to_string(),
                    workflow_run_id.to_string(),
                    request.idempotency_key,
                    request.repository_id.to_string(),
                    source.epic_content_hash,
                    repository_head,
                    at,
                ],
            )?;
            Ok(())
        })?;
        Ok(FeaturePlanningDraft {
            feature_id,
            workflow_run_id,
            epic_id: request.epic_id,
            repository_id: request.repository_id,
            slug: request.slug,
            title: request.title,
            epic_content_hash: source.epic_content_hash,
            repository_head,
            state: WorkflowState::WorktreePending,
        })
    }

    pub fn mark_launch_pending(
        &mut self,
        feature_id: FeatureId,
        checkout_id: CheckoutId,
        occurred_at: OffsetDateTime,
    ) -> Result<FeaturePlanningDraft, AppError> {
        self.store.write(|transaction| {
            let draft = draft_by_feature(transaction, feature_id)?;
            if draft.state == WorkflowState::PlanningLaunchPending {
                return Ok(draft);
            }
            if draft.state != WorkflowState::WorktreePending {
                return Err(AppError::Domain(
                    "Feature is not waiting for its planning checkout".to_owned(),
                ));
            }
            let valid_checkout: i64 = transaction.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM feature_checkouts
                     WHERE feature_id = ?1 AND repository_id = ?2 AND checkout_id = ?3
                 )",
                params![
                    feature_id.to_string(),
                    draft.repository_id.to_string(),
                    checkout_id.to_string(),
                ],
                |row| row.get(0),
            )?;
            if valid_checkout == 0 {
                return Err(AppError::ResumeCheckoutNotScanned);
            }
            transition_feature(
                transaction,
                &draft,
                WorkflowState::PlanningLaunchPending,
                WorkflowActor::Application,
                occurred_at,
                &serde_json::json!({ "checkout_id": checkout_id }).to_string(),
                None,
            )?;
            draft_by_feature(transaction, feature_id)
        })
    }

    pub fn submit_proposal(
        &mut self,
        feature_id: FeatureId,
        launch_token: &str,
        proposal: FeatureProposal,
        idempotency_key: &str,
        submitted_at: OffsetDateTime,
    ) -> Result<FeatureProposalOutcome, AppError> {
        validate_idempotency_key(idempotency_key)?;
        validate_proposal(&proposal)?;
        let stored = StoredFeatureProposal {
            feature_document_id: DocumentId::generate(),
            work_items: proposal
                .work_items
                .iter()
                .cloned()
                .map(|proposal| StoredProposedWorkItem {
                    work_item_id: WorkItemId::generate(),
                    document_id: DocumentId::generate(),
                    proposal,
                })
                .collect(),
            proposal,
        };
        let proposal_json = serde_json::to_string(&stored)?;
        self.store.write(|transaction| {
            let draft =
                authenticated_planning_draft(transaction, feature_id, launch_token, submitted_at)?;
            if stored.proposal.expected_epic_content_hash != draft.epic_content_hash
                || stored.proposal.expected_repository_head != draft.repository_head
            {
                return Err(AppError::WorkflowDocumentChanged);
            }
            if let Some((existing_key, existing_json, status)) = transaction
                .query_row(
                    "SELECT idempotency_key, proposal_json, status
                     FROM feature_planning_proposals WHERE feature_id = ?1",
                    [feature_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
            {
                let existing_proposal: StoredFeatureProposal =
                    serde_json::from_str(&existing_json)?;
                if existing_key == idempotency_key && existing_proposal.proposal == stored.proposal
                {
                    return Ok(FeatureProposalOutcome {
                        feature_id,
                        workflow_run_id: draft.workflow_run_id,
                        state: proposal_state(&status)?,
                        work_item_count: existing_proposal.work_items.len(),
                    });
                }
                if status != "rejected" {
                    return Err(AppError::IdempotencyConflict);
                }
            }
            if draft.state != WorkflowState::PlanningActive {
                return Err(AppError::WorkflowOperationUnauthorized);
            }
            transition_feature(
                transaction,
                &draft,
                WorkflowState::ProposalReady,
                WorkflowActor::Integration,
                submitted_at,
                &proposal_json,
                Some(idempotency_key),
            )?;
            let ready = FeaturePlanningDraft {
                state: WorkflowState::ProposalReady,
                ..draft.clone()
            };
            transition_feature(
                transaction,
                &ready,
                WorkflowState::AwaitingApproval,
                WorkflowActor::Application,
                submitted_at,
                "{}",
                None,
            )?;
            transaction.execute(
                "INSERT INTO feature_planning_proposals (
                     feature_id, workflow_run_id, idempotency_key, proposal_json,
                     status, submitted_at
                 ) VALUES (?1, ?2, ?3, ?4, 'awaiting_approval', ?5)
                 ON CONFLICT(feature_id) DO UPDATE SET
                     workflow_run_id = excluded.workflow_run_id,
                     idempotency_key = excluded.idempotency_key,
                     proposal_json = excluded.proposal_json,
                     status = excluded.status,
                     submitted_at = excluded.submitted_at,
                     approved_at = NULL,
                     published_commit = NULL",
                params![
                    feature_id.to_string(),
                    draft.workflow_run_id.to_string(),
                    idempotency_key,
                    proposal_json,
                    timestamp(submitted_at),
                ],
            )?;
            Ok(FeatureProposalOutcome {
                feature_id,
                workflow_run_id: draft.workflow_run_id,
                state: WorkflowState::AwaitingApproval,
                work_item_count: stored.work_items.len(),
            })
        })
    }

    pub fn reject_proposal(
        &mut self,
        feature_id: FeatureId,
        rejected_at: OffsetDateTime,
    ) -> Result<FeatureProposalOutcome, AppError> {
        self.store.write(|transaction| {
            let draft = draft_by_feature(transaction, feature_id)?;
            if draft.state != WorkflowState::AwaitingApproval {
                return Err(AppError::Domain(
                    "Feature has no proposal awaiting approval".to_owned(),
                ));
            }
            transition_feature(
                transaction,
                &draft,
                WorkflowState::PlanningActive,
                WorkflowActor::User,
                rejected_at,
                "{}",
                None,
            )?;
            transaction.execute(
                "UPDATE feature_planning_proposals SET status = 'rejected'
                 WHERE feature_id = ?1 AND status = 'awaiting_approval'",
                [feature_id.to_string()],
            )?;
            let count = proposal_work_item_count(transaction, feature_id)?;
            Ok(FeatureProposalOutcome {
                feature_id,
                workflow_run_id: draft.workflow_run_id,
                state: WorkflowState::PlanningActive,
                work_item_count: count,
            })
        })
    }

    pub fn approve_proposal(
        &mut self,
        feature_id: FeatureId,
        approved_at: OffsetDateTime,
    ) -> Result<FeatureProposalOutcome, AppError> {
        self.store.write(|transaction| {
            let draft = draft_by_feature(transaction, feature_id)?;
            if draft.state != WorkflowState::AwaitingApproval {
                return Err(AppError::Domain(
                    "Feature has no proposal awaiting approval".to_owned(),
                ));
            }
            transition_feature(
                transaction,
                &draft,
                WorkflowState::Publishing,
                WorkflowActor::User,
                approved_at,
                "{}",
                None,
            )?;
            transaction.execute(
                "UPDATE feature_planning_proposals
                 SET status = 'publishing', approved_at = ?2
                 WHERE feature_id = ?1 AND status = 'awaiting_approval'",
                params![feature_id.to_string(), timestamp(approved_at)],
            )?;
            let count = proposal_work_item_count(transaction, feature_id)?;
            Ok(FeatureProposalOutcome {
                feature_id,
                workflow_run_id: draft.workflow_run_id,
                state: WorkflowState::Publishing,
                work_item_count: count,
            })
        })
    }

    pub fn publish_approved(
        &mut self,
        feature_id: FeatureId,
        published_at: OffsetDateTime,
    ) -> Result<FeaturePublicationOutcome, AppError> {
        if let Some(existing) = self.published_outcome(feature_id)? {
            return Ok(existing);
        }
        let publication = self.publication_context(feature_id)?;
        if publication.draft.state != WorkflowState::Publishing {
            return Err(AppError::WorkflowOperationUnauthorized);
        }
        let planning_store = PlanningStore::create_or_link(&publication.planning_store_path)?;
        let current_epic = planning_store.read_document(&publication.epic_path)?;
        let current_head = GitCli.resolve(&publication.repository_path)?.head_oid;
        if current_epic.content_hash != publication.draft.epic_content_hash
            || current_head != publication.draft.repository_head
        {
            self.mark_reconciliation(feature_id, published_at)?;
            return Err(AppError::WorkflowDocumentChanged);
        }
        let documents = publication.documents()?;
        let stored = planning_store.publish_batch_new(
            &documents,
            &format!("Publish {} feature plan", publication.draft.title),
        )?;
        let commit = stored
            .first()
            .and_then(|document| document.observed_commit.clone())
            .ok_or_else(|| AppError::PlanningGit {
                message: "planning publication produced no commit".to_owned(),
            })?;
        self.store.write(|transaction| {
            let current = draft_by_feature(transaction, feature_id)?;
            if current.state != WorkflowState::Publishing {
                return Err(AppError::IdempotencyConflict);
            }
            let at = timestamp(published_at);
            insert_document(
                transaction,
                publication.planning_repository_id,
                HierarchyOwner::Feature(feature_id),
                &stored[0],
                &at,
            )?;
            for (proposal_order, item) in publication.proposal.work_items.iter().enumerate() {
                let key = WorkItemKey::new(format!(
                    "{}/{}/{}",
                    publication.epic_slug, publication.draft.slug, item.proposal.slug
                ))
                .map_err(|error| AppError::Domain(error.to_string()))?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at,
                         proposal_order
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7)",
                    params![
                        item.work_item_id.to_string(),
                        feature_id.to_string(),
                        key.as_str(),
                        item.proposal.slug.as_str(),
                        item.proposal.title,
                        at,
                        i64::try_from(proposal_order)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                    ],
                )?;
                for repository_id in &item.proposal.repository_ids {
                    transaction.execute(
                        "INSERT INTO work_item_repositories (work_item_id, repository_id)
                         VALUES (?1, ?2)",
                        params![item.work_item_id.to_string(), repository_id.to_string()],
                    )?;
                }
            }
            for item in &publication.proposal.work_items {
                for (dependency_order, dependency) in item.proposal.dependencies.iter().enumerate()
                {
                    let dependency_id = publication
                        .proposal
                        .work_items
                        .iter()
                        .find(|candidate| &candidate.proposal.slug == dependency)
                        .map(|candidate| candidate.work_item_id)
                        .ok_or_else(|| {
                            AppError::PlanningDocumentInvalid(format!(
                                "Work-item dependency {dependency} is missing"
                            ))
                        })?;
                    transaction.execute(
                        "INSERT INTO work_item_dependencies (
                             work_item_id, dependency_work_item_id, dependency_order
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            item.work_item_id.to_string(),
                            dependency_id.to_string(),
                            i64::try_from(dependency_order)
                                .map_err(|error| AppError::Domain(error.to_string()))?,
                        ],
                    )?;
                }
            }
            for (document, item) in stored
                .iter()
                .skip(1)
                .zip(publication.proposal.work_items.iter())
            {
                insert_document(
                    transaction,
                    publication.planning_repository_id,
                    HierarchyOwner::WorkItem(item.work_item_id),
                    document,
                    &at,
                )?;
            }
            transition_feature(
                transaction,
                &current,
                WorkflowState::Planned,
                WorkflowActor::Application,
                published_at,
                &serde_json::json!({ "commit": commit }).to_string(),
                None,
            )?;
            transaction.execute(
                "UPDATE feature_planning_proposals
                 SET status = 'published', published_commit = ?2
                 WHERE feature_id = ?1 AND status = 'publishing'",
                params![feature_id.to_string(), commit],
            )?;
            Ok(())
        })?;
        self.published_outcome(feature_id)?.ok_or_else(|| {
            AppError::Domain("published Feature projection is unavailable".to_owned())
        })
    }

    fn draft_for_idempotency(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<FeaturePlanningDraft>, AppError> {
        self.store.read(|connection| {
            let feature_id = connection
                .query_row(
                    "SELECT feature_id FROM feature_planning_contexts WHERE idempotency_key = ?1",
                    [idempotency_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            feature_id
                .as_deref()
                .map(|id| draft_by_feature(connection, parse_id(id)?))
                .transpose()
        })
    }

    fn source_context(
        &self,
        epic_id: EpicId,
        repository_id: RepositoryId,
    ) -> Result<PlanningSourceContext, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT document.content_hash, path.path
                     FROM epics epic
                     JOIN repositories repository
                       ON repository.workspace_id = epic.workspace_id
                      AND repository.id = ?2
                      AND repository.is_planning_store = 0
                     JOIN repository_paths path
                       ON path.repository_id = repository.id AND path.observed_until IS NULL
                     JOIN documents document ON document.epic_id = epic.id
                     WHERE epic.id = ?1",
                    params![epic_id.to_string(), repository_id.to_string()],
                    |row| {
                        Ok(PlanningSourceContext {
                            epic_content_hash: row.get(0)?,
                            repository_path: PathBuf::from(row.get::<_, String>(1)?),
                        })
                    },
                )
                .optional()?
                .ok_or(AppError::ResumeRepositoryMismatch)
        })
    }

    fn publication_context(&self, feature_id: FeatureId) -> Result<PublicationContext, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT context.workflow_run_id, feature.epic_id, feature.slug, feature.title,
                            feature.workflow_state, context.repository_id,
                            context.epic_content_hash, context.repository_head,
                            workspace.slug, epic.slug, epic_document.relative_path,
                            planning_repository.id, planning_path.path, code_path.path,
                            proposal.proposal_json
                     FROM feature_planning_contexts context
                     JOIN features feature ON feature.id = context.feature_id
                     JOIN epics epic ON epic.id = feature.epic_id
                     JOIN workspaces workspace ON workspace.id = epic.workspace_id
                     JOIN documents epic_document ON epic_document.epic_id = epic.id
                     JOIN repositories planning_repository
                       ON planning_repository.id = workspace.planning_store_repository_id
                     JOIN repository_paths planning_path
                       ON planning_path.repository_id = planning_repository.id
                      AND planning_path.observed_until IS NULL
                     JOIN repository_paths code_path
                       ON code_path.repository_id = context.repository_id
                      AND code_path.observed_until IS NULL
                     JOIN feature_planning_proposals proposal
                       ON proposal.feature_id = feature.id AND proposal.status = 'publishing'
                     WHERE feature.id = ?1",
                    [feature_id.to_string()],
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
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, String>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, String>(13)?,
                            row.get::<_, String>(14)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                workflow_run_id,
                epic_id,
                feature_slug,
                title,
                state,
                repository_id,
                epic_content_hash,
                repository_head,
                workspace_slug,
                epic_slug,
                epic_path,
                planning_repository_id,
                planning_store_path,
                repository_path,
                proposal_json,
            )) = row
            else {
                return Err(AppError::WorkflowOperationUnauthorized);
            };
            let proposal: StoredFeatureProposal = serde_json::from_str(&proposal_json)?;
            let repository_slugs = repository_slugs(connection, feature_id, &proposal)?;
            Ok(PublicationContext {
                draft: FeaturePlanningDraft {
                    feature_id,
                    workflow_run_id: parse_id(&workflow_run_id)?,
                    epic_id: parse_id(&epic_id)?,
                    repository_id: parse_id(&repository_id)?,
                    slug: parse_slug(&feature_slug)?,
                    title,
                    epic_content_hash,
                    repository_head,
                    state: parse_state(&state)?,
                },
                workspace_slug: parse_slug(&workspace_slug)?,
                epic_slug: parse_slug(&epic_slug)?,
                epic_path: PathBuf::from(epic_path),
                planning_repository_id: parse_id(&planning_repository_id)?,
                planning_store_path: PathBuf::from(planning_store_path),
                repository_path: PathBuf::from(repository_path),
                proposal,
                repository_slugs,
            })
        })
    }

    fn mark_reconciliation(
        &mut self,
        feature_id: FeatureId,
        occurred_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            let draft = draft_by_feature(transaction, feature_id)?;
            transition_feature(
                transaction,
                &draft,
                WorkflowState::ReconciliationRequired,
                WorkflowActor::Reconciliation,
                occurred_at,
                "{}",
                None,
            )
        })
    }

    fn published_outcome(
        &self,
        feature_id: FeatureId,
    ) -> Result<Option<FeaturePublicationOutcome>, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT context.workflow_run_id, proposal.proposal_json,
                            proposal.published_commit
                     FROM feature_planning_contexts context
                     JOIN feature_planning_proposals proposal
                       ON proposal.feature_id = context.feature_id
                     WHERE context.feature_id = ?1 AND proposal.status = 'published'",
                    [feature_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            row.map(|(workflow_run_id, proposal_json, commit)| {
                let proposal: StoredFeatureProposal = serde_json::from_str(&proposal_json)?;
                let work_item_ids = proposal
                    .work_items
                    .iter()
                    .map(|item| item.work_item_id)
                    .collect::<Vec<_>>();
                let first_work_item_id = proposal
                    .proposal
                    .first_work_item_slug
                    .as_ref()
                    .and_then(|slug| {
                        proposal
                            .work_items
                            .iter()
                            .find(|item| item.proposal.slug == *slug)
                    })
                    .map(|item| item.work_item_id);
                Ok(FeaturePublicationOutcome {
                    feature_id,
                    workflow_run_id: parse_id(&workflow_run_id)?,
                    commit,
                    work_item_ids,
                    first_work_item_id,
                })
            })
            .transpose()
        })
    }
}

pub(crate) fn activate_planning_for_binding(
    transaction: &Transaction<'_>,
    owner: HierarchyOwner,
    role: ManagedSessionRole,
    occurred_at: OffsetDateTime,
) -> Result<(), AppError> {
    let (HierarchyOwner::Feature(feature_id), ManagedSessionRole::FeaturePlanning) = (owner, role)
    else {
        return Ok(());
    };
    let draft = draft_by_feature(transaction, feature_id)?;
    if draft.state == WorkflowState::PlanningActive {
        return Ok(());
    }
    if draft.state != WorkflowState::PlanningLaunchPending {
        return Err(AppError::WorkflowOperationUnauthorized);
    }
    transition_feature(
        transaction,
        &draft,
        WorkflowState::PlanningActive,
        WorkflowActor::Integration,
        occurred_at,
        "{}",
        None,
    )
}

pub fn planner_bootstrap_prompt(draft: &FeaturePlanningDraft) -> String {
    format!(
        "Use the installed Agent Workboard workflow to plan Feature {}. Read hierarchy for workflow run {} and Feature {}. Collaborate with the user, then submit one complete Feature proposal through the typed operation. Do not publish documents or create Work items directly.",
        draft.title, draft.workflow_run_id, draft.feature_id
    )
}

struct PlanningSourceContext {
    epic_content_hash: String,
    repository_path: PathBuf,
}

struct PublicationContext {
    draft: FeaturePlanningDraft,
    workspace_slug: Slug,
    epic_slug: Slug,
    epic_path: PathBuf,
    planning_repository_id: RepositoryId,
    planning_store_path: PathBuf,
    repository_path: PathBuf,
    proposal: StoredFeatureProposal,
    repository_slugs: Vec<(RepositoryId, Slug)>,
}

impl PublicationContext {
    fn documents(&self) -> Result<Vec<NewPlanningDocument>, AppError> {
        let feature_repositories = self
            .repository_slugs
            .iter()
            .map(|(_, slug)| slug.clone())
            .collect::<Vec<_>>();
        let mut documents = vec![NewPlanningDocument {
            relative_path: PlanningStore::feature_path(
                &self.workspace_slug,
                &self.epic_slug,
                &self.draft.slug,
            ),
            front_matter: DocumentFrontMatter {
                id: self.proposal.feature_document_id,
                kind: DocumentKind::Feature,
                key: format!("{}/{}", self.epic_slug, self.draft.slug),
                status: None,
                repositories: feature_repositories,
            },
            body: self.proposal.proposal.feature_body.clone(),
        }];
        for item in &self.proposal.work_items {
            let repositories = item
                .proposal
                .repository_ids
                .iter()
                .map(|id| {
                    self.repository_slugs
                        .iter()
                        .find(|(repository_id, _)| repository_id == id)
                        .map(|(_, slug)| slug.clone())
                        .ok_or(AppError::WorkItemRepositoryMismatch)
                })
                .collect::<Result<Vec<_>, _>>()?;
            documents.push(NewPlanningDocument {
                relative_path: PlanningStore::work_item_path(
                    &self.workspace_slug,
                    &self.epic_slug,
                    &self.draft.slug,
                    &item.proposal.slug,
                ),
                front_matter: DocumentFrontMatter {
                    id: item.document_id,
                    kind: DocumentKind::WorkItem,
                    key: format!(
                        "{}/{}/{}",
                        self.epic_slug, self.draft.slug, item.proposal.slug
                    ),
                    status: Some(WorkItemStatus::Ready),
                    repositories,
                },
                body: item.proposal.body.clone(),
            });
        }
        Ok(documents)
    }
}

fn authenticated_planning_draft(
    transaction: &Transaction<'_>,
    feature_id: FeatureId,
    launch_token: &str,
    observed_at: OffsetDateTime,
) -> Result<FeaturePlanningDraft, AppError> {
    let valid: i64 = transaction.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM launch_intents intent
             JOIN managed_sessions managed ON managed.launch_intent_id = intent.id
             JOIN native_session_associations association
               ON association.session_id = managed.session_id
              AND association.feature_id = ?1
              AND association.associated_until IS NULL
             WHERE intent.workflow_token_hash = ?2 AND intent.status = 'bound'
               AND intent.role = 'feature_planning'
               AND intent.workflow_token_expires_at > ?3
               AND managed.managed_until IS NULL
         )",
        params![
            feature_id.to_string(),
            token_hash(launch_token),
            timestamp(observed_at),
        ],
        |row| row.get(0),
    )?;
    if valid == 0 {
        return Err(AppError::WorkflowOperationUnauthorized);
    }
    draft_by_feature(transaction, feature_id)
}

fn draft_by_feature(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
) -> Result<FeaturePlanningDraft, AppError> {
    connection
        .query_row(
            "SELECT context.workflow_run_id, feature.epic_id, context.repository_id,
                    feature.slug, feature.title, context.epic_content_hash,
                    context.repository_head, run.current_state
             FROM feature_planning_contexts context
             JOIN features feature ON feature.id = context.feature_id
             JOIN workflow_runs run ON run.id = context.workflow_run_id
             WHERE feature.id = ?1",
            [feature_id.to_string()],
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
                ))
            },
        )
        .optional()?
        .map(
            |(
                workflow_run_id,
                epic_id,
                repository_id,
                slug,
                title,
                epic_content_hash,
                repository_head,
                state,
            )| {
                Ok::<_, AppError>(FeaturePlanningDraft {
                    feature_id,
                    workflow_run_id: parse_id(&workflow_run_id)?,
                    epic_id: parse_id(&epic_id)?,
                    repository_id: parse_id(&repository_id)?,
                    slug: parse_slug(&slug)?,
                    title,
                    epic_content_hash,
                    repository_head,
                    state: parse_state(&state)?,
                })
            },
        )
        .transpose()?
        .ok_or_else(|| AppError::Domain("Feature planning workflow does not exist".to_owned()))
}

#[allow(clippy::too_many_arguments)]
fn append_event(
    transaction: &Transaction<'_>,
    run_id: WorkflowRunId,
    from: WorkflowState,
    to: WorkflowState,
    actor: WorkflowActor,
    occurred_at: OffsetDateTime,
    payload_json: &str,
    idempotency_key: Option<&str>,
) -> Result<WorkflowEventId, AppError> {
    if !from.can_transition_to(to) {
        return Err(AppError::Domain(format!(
            "workflow cannot transition from {} to {}",
            wire_name(from)?,
            wire_name(to)?
        )));
    }
    let sequence: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE run_id = ?1",
        [run_id.to_string()],
        |row| row.get(0),
    )?;
    let event_id = WorkflowEventId::generate();
    transaction.execute(
        "INSERT INTO workflow_events (
             id, run_id, sequence, from_state, to_state, actor, occurred_at,
             payload_json, idempotency_key
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event_id.to_string(),
            run_id.to_string(),
            sequence,
            wire_name(from)?,
            wire_name(to)?,
            wire_name(actor)?,
            timestamp(occurred_at),
            payload_json,
            idempotency_key,
        ],
    )?;
    Ok(event_id)
}

fn transition_feature(
    transaction: &Transaction<'_>,
    draft: &FeaturePlanningDraft,
    next: WorkflowState,
    actor: WorkflowActor,
    occurred_at: OffsetDateTime,
    payload_json: &str,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    append_event(
        transaction,
        draft.workflow_run_id,
        draft.state,
        next,
        actor,
        occurred_at,
        payload_json,
        idempotency_key,
    )?;
    let completed_at = next.is_terminal().then(|| timestamp(occurred_at));
    transaction.execute(
        "UPDATE workflow_runs SET current_state = ?2, completed_at = ?3 WHERE id = ?1",
        params![
            draft.workflow_run_id.to_string(),
            wire_name(next)?,
            completed_at,
        ],
    )?;
    transaction.execute(
        "UPDATE features SET workflow_state = ?2 WHERE id = ?1",
        params![draft.feature_id.to_string(), wire_name(next)?],
    )?;
    Ok(())
}

fn insert_document(
    transaction: &Transaction<'_>,
    repository_id: RepositoryId,
    owner: HierarchyOwner,
    document: &crate::planning_store::StoredDocument,
    observed_at: &str,
) -> Result<(), AppError> {
    let (feature_id, work_item_id, kind) = match owner {
        HierarchyOwner::Feature(id) => (Some(id.to_string()), None, "feature"),
        HierarchyOwner::WorkItem(id) => (None, Some(id.to_string()), "work_item"),
        HierarchyOwner::Epic(_) | HierarchyOwner::Workspace(_) => {
            return Err(AppError::Domain(
                "Feature publication cannot create an Epic or workspace document".to_owned(),
            ));
        }
    };
    transaction.execute(
        "INSERT INTO documents (
             id, repository_id, feature_id, work_item_id, kind, relative_path,
             content_hash, observed_commit, observed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            document.front_matter.id.to_string(),
            repository_id.to_string(),
            feature_id,
            work_item_id,
            kind,
            document.relative_path.to_string_lossy(),
            document.content_hash,
            document.observed_commit,
            observed_at,
        ],
    )?;
    transaction.execute(
        "INSERT INTO document_revisions (
             document_id, revision, content_hash, observed_commit, observed_at
         ) VALUES (?1, 1, ?2, ?3, ?4)",
        params![
            document.front_matter.id.to_string(),
            document.content_hash,
            document.observed_commit,
            observed_at,
        ],
    )?;
    Ok(())
}

fn repository_slugs(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
    proposal: &StoredFeatureProposal,
) -> Result<Vec<(RepositoryId, Slug)>, AppError> {
    let ids = proposal
        .work_items
        .iter()
        .flat_map(|item| item.proposal.repository_ids.iter().copied())
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    for id in ids {
        let slug = connection
            .query_row(
                "SELECT repository.slug FROM repositories repository
                 JOIN features feature
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE repository.id = ?1 AND feature.id = ?2
                   AND repository.workspace_id = epic.workspace_id
                   AND repository.is_planning_store = 0",
                params![id.to_string(), feature_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(AppError::WorkItemRepositoryMismatch)?;
        result.push((id, parse_slug(&slug)?));
    }
    result.sort_by(|left, right| left.1.as_str().cmp(right.1.as_str()));
    Ok(result)
}

fn proposal_work_item_count(
    transaction: &Transaction<'_>,
    feature_id: FeatureId,
) -> Result<usize, AppError> {
    let proposal_json: String = transaction.query_row(
        "SELECT proposal_json FROM feature_planning_proposals WHERE feature_id = ?1",
        [feature_id.to_string()],
        |row| row.get(0),
    )?;
    Ok(
        serde_json::from_str::<StoredFeatureProposal>(&proposal_json)?
            .work_items
            .len(),
    )
}

fn validate_proposal(proposal: &FeatureProposal) -> Result<(), AppError> {
    let encoded = serde_json::to_vec(proposal)?;
    if encoded.len() > MAX_PROPOSAL_BYTES
        || proposal.feature_body.trim().is_empty()
        || proposal.work_items.is_empty()
        || proposal.verification.is_empty()
        || proposal
            .verification
            .iter()
            .any(|item| item.trim().is_empty())
    {
        return Err(AppError::PlanningDocumentInvalid(
            "Feature proposal is empty, incomplete, or too large".to_owned(),
        ));
    }
    validate_hash(
        &proposal.expected_epic_content_hash,
        64,
        "Epic content hash",
    )?;
    if !matches!(proposal.expected_repository_head.len(), 40 | 64)
        || !proposal
            .expected_repository_head
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(AppError::PlanningDocumentInvalid(
            "repository head must be a complete Git object ID".to_owned(),
        ));
    }
    let slugs = proposal
        .work_items
        .iter()
        .map(|item| item.slug.clone())
        .collect::<HashSet<_>>();
    if slugs.len() != proposal.work_items.len() {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item slugs must be unique".to_owned(),
        ));
    }
    for item in &proposal.work_items {
        validate_title(&item.title)?;
        if item.body.trim().is_empty() || item.repository_ids.is_empty() {
            return Err(AppError::PlanningDocumentInvalid(
                "each Work item needs a body and repository".to_owned(),
            ));
        }
        if item
            .dependencies
            .iter()
            .any(|dependency| !slugs.contains(dependency) || dependency == &item.slug)
        {
            return Err(AppError::PlanningDocumentInvalid(
                "Work-item dependency is missing or self-referential".to_owned(),
            ));
        }
        if item.dependencies.iter().collect::<HashSet<_>>().len() != item.dependencies.len() {
            return Err(AppError::PlanningDocumentInvalid(
                "Work-item dependencies must be unique".to_owned(),
            ));
        }
    }
    validate_dependency_graph(proposal)?;
    if proposal
        .first_work_item_slug
        .as_ref()
        .is_some_and(|slug| !slugs.contains(slug))
    {
        return Err(AppError::PlanningDocumentInvalid(
            "first Work item does not exist in the proposal".to_owned(),
        ));
    }
    Ok(())
}

fn validate_dependency_graph(proposal: &FeatureProposal) -> Result<(), AppError> {
    let graph = proposal
        .work_items
        .iter()
        .map(|item| (item.slug.clone(), item.dependencies.clone()))
        .collect::<HashMap<_, _>>();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for slug in graph.keys() {
        visit_dependency(slug, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency(
    slug: &Slug,
    graph: &HashMap<Slug, Vec<Slug>>,
    visiting: &mut HashSet<Slug>,
    visited: &mut HashSet<Slug>,
) -> Result<(), AppError> {
    if visited.contains(slug) {
        return Ok(());
    }
    if !visiting.insert(slug.clone()) {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item dependencies must be acyclic".to_owned(),
        ));
    }
    for dependency in graph.get(slug).into_iter().flatten() {
        visit_dependency(dependency, graph, visiting, visited)?;
    }
    visiting.remove(slug);
    visited.insert(slug.clone());
    Ok(())
}

fn validate_title(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(AppError::Domain(
            "Feature or Work-item title is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(AppError::EmptyIdempotencyKey)
    } else {
        Ok(())
    }
}

fn validate_hash(value: &str, length: usize, label: &str) -> Result<(), AppError> {
    if value.len() != length || !value.bytes().all(|value| value.is_ascii_hexdigit()) {
        Err(AppError::PlanningDocumentInvalid(format!(
            "{label} is invalid"
        )))
    } else {
        Ok(())
    }
}

fn proposal_state(value: &str) -> Result<WorkflowState, AppError> {
    match value {
        "awaiting_approval" => Ok(WorkflowState::AwaitingApproval),
        "rejected" => Ok(WorkflowState::PlanningActive),
        "publishing" => Ok(WorkflowState::Publishing),
        "published" => Ok(WorkflowState::Planned),
        _ => Err(AppError::Domain("proposal state is invalid".to_owned())),
    }
}

fn parse_state(value: &str) -> Result<WorkflowState, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_slug(value: &str) -> Result<Slug, AppError> {
    Slug::new(value).map_err(|error| AppError::Domain(error.to_string()))
}

fn wire_name<T: Serialize>(value: T) -> Result<String, AppError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Domain("workflow wire value is invalid".to_owned()))
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
        .parse::<T>()
        .map_err(|error| AppError::Domain(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use serde_json::json;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{
        HierarchyOwner, ManagedLaunchMode, ManagedSessionRole, ProcessIdentity, Slug, Tool,
        WorkflowState,
    };

    use super::{CreateFeaturePlanning, FeatureProposal, ProposedWorkItem};
    use crate::AppError;
    use crate::checkout::PrepareFeatureCheckout;
    use crate::hooks::HookIngestionMutation;
    use crate::planning_store::PlanningStore;
    use crate::session_launch::BeginManagedSessionLaunch;
    use crate::workspace::{
        CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
    };

    struct Fixture {
        _directory: TempDir,
        app: WorkboardApplication,
        workspace_id: workboard_core::WorkspaceId,
        epic_id: workboard_core::EpicId,
        repository_id: workboard_core::RepositoryId,
        planning_store: PathBuf,
        code_repository: PathBuf,
        checkout: PathBuf,
        terminal: PathBuf,
        native: PathBuf,
        at: OffsetDateTime,
    }

    fn planning_capability_fixture(
        fixture: &Fixture,
    ) -> crate::session_launch::CapabilityLaunchInputs {
        let root = fixture._directory.path();
        let provider_home = root.join("provider-home");
        std::fs::create_dir_all(&provider_home).expect("provider home");
        std::fs::write(provider_home.join("auth.json"), b"{}").expect("provider credential");
        std::fs::write(provider_home.join(".credentials.json"), b"{}")
            .expect("provider credential");
        let executable = root.join("workboard.exe");
        if !executable.exists() {
            std::fs::write(&executable, b"").expect("executable fixture");
        }
        crate::session_launch::CapabilityLaunchInputs {
            bundle_parent: root.join("managed-sessions"),
            provider_home,
            workboard_executable: executable,
            database: fixture.app.database_path().to_path_buf(),
            repository: "fixture".to_owned(),
        }
    }

    struct ActivePlanning {
        draft: super::FeaturePlanningDraft,
        workflow_token: String,
    }

    fn git(directory: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .output()
            .expect("run Git fixture command");
        assert!(
            output.status.success(),
            "Git fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("Git fixture output")
            .trim()
            .to_owned()
    }

    fn configure_git(repository: &Path) {
        git(repository, &["config", "user.name", "Workboard Test"]);
        git(
            repository,
            &["config", "user.email", "workboard@example.invalid"],
        );
    }

    fn fixture() -> Fixture {
        let directory = TempDir::new().expect("temporary directory");
        let planning_store = directory.path().join("planning");
        let store = PlanningStore::create_or_link(&planning_store).expect("planning store");
        configure_git(store.root());

        let code_repository = directory.path().join("code");
        fs::create_dir(&code_repository).expect("code repository directory");
        git(&code_repository, &["init", "-b", "main"]);
        configure_git(&code_repository);
        fs::write(code_repository.join("README.md"), "# Demo\n").expect("code fixture");
        git(&code_repository, &["add", "README.md"]);
        git(&code_repository, &["commit", "-m", "Initial code"]);

        let mut app = WorkboardApplication::open(directory.path().join("workboard.sqlite"))
            .expect("open Workboard");
        let snapshot = app
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store.clone(),
            })
            .expect("initialise workspace");
        let repository = app
            .register_repository(RegisterRepository {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("code").expect("repository slug"),
                title: "Code".to_owned(),
                path: code_repository.clone(),
            })
            .expect("register code repository");
        let epic = app
            .create_epic(CreateEpic {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("launch").expect("Epic slug"),
                title: "Launch".to_owned(),
                body: "# Launch\n\nShip the first release.\n".to_owned(),
            })
            .expect("create Epic");

        let checkout = directory.path().join("worktrees").join("availability");
        fs::create_dir(directory.path().join("worktrees")).expect("worktree parent");
        let terminal = directory.path().join(terminal_name());
        let native = directory.path().join(native_name());
        fs::write(&terminal, []).expect("terminal fixture");
        fs::write(&native, []).expect("native fixture");
        Fixture {
            _directory: directory,
            app,
            workspace_id: snapshot.workspace.id,
            epic_id: epic.id,
            repository_id: repository.id,
            planning_store,
            code_repository,
            checkout,
            terminal,
            native,
            at: OffsetDateTime::parse(
                "2026-08-28T10:00:00Z",
                &time::format_description::well_known::Rfc3339,
            )
            .expect("fixture timestamp"),
        }
    }

    fn activate_planning(fixture: &mut Fixture, tool: Tool) -> ActivePlanning {
        let draft = fixture
            .app
            .planning_workflows()
            .create_feature(CreateFeaturePlanning {
                epic_id: fixture.epic_id,
                repository_id: fixture.repository_id,
                slug: Slug::new("availability").expect("Feature slug"),
                title: "Availability".to_owned(),
                idempotency_key: "create-availability".to_owned(),
                created_at: fixture.at,
            })
            .expect("create Feature planning workflow");
        let repeated = fixture
            .app
            .planning_workflows()
            .create_feature(CreateFeaturePlanning {
                epic_id: fixture.epic_id,
                repository_id: fixture.repository_id,
                slug: draft.slug.clone(),
                title: draft.title.clone(),
                idempotency_key: "create-availability".to_owned(),
                created_at: fixture.at,
            })
            .expect("repeat Feature creation");
        assert_eq!(repeated.feature_id, draft.feature_id);

        let checkout = fixture
            .app
            .checkout_service()
            .prepare_feature(PrepareFeatureCheckout {
                feature_id: draft.feature_id,
                repository_id: fixture.repository_id,
                target: fixture.checkout.clone(),
                branch: "feature/availability".to_owned(),
                create_branch: true,
                start_point: "main".to_owned(),
                idempotency_key: "checkout-availability".to_owned(),
                observed_at: fixture.at,
            })
            .expect("prepare Feature checkout");
        fixture
            .app
            .planning_workflows()
            .mark_launch_pending(
                draft.feature_id,
                checkout.checkout_id,
                fixture.at + time::Duration::seconds(1),
            )
            .expect("mark planning launch pending");
        let capability = planning_capability_fixture(fixture);
        let prepared = fixture
            .app
            .session_launch()
            .begin(BeginManagedSessionLaunch {
                owner: HierarchyOwner::Feature(draft.feature_id),
                role: ManagedSessionRole::FeaturePlanning,
                tool,
                mode: ManagedLaunchMode::New,
                checkout_id: checkout.checkout_id,
                working_directory: fixture.checkout.clone(),
                title: draft.title.clone(),
                terminal_window: Some(format!("workboard-feature-{}", draft.feature_id)),
                terminal_executable: fixture.terminal.clone(),
                native_executable: fixture.native.clone(),
                idempotency_key: "launch-availability-planner".to_owned(),
                created_at: fixture.at + time::Duration::seconds(2),
                expires_at: fixture.at + time::Duration::minutes(2),
                resume_context: None,
                profile: workboard_core::LaunchProfile::suggested(
                    tool,
                    ManagedSessionRole::FeaturePlanning,
                ),
                initial_prompt: Some(super::planner_bootstrap_prompt(&draft)),
                capability,
            })
            .expect("prepare planner launch");
        let launch_token = prepared.prepared.launch.launch_token().to_owned();
        let workflow_token = prepared
            .prepared
            .launch
            .workflow_token()
            .expect("workflow credential")
            .to_owned();
        fixture
            .app
            .session_launch()
            .bind_hook(&HookIngestionMutation {
                tool,
                payload_json: json!({
                    "session_id": "planning-thread",
                    "cwd": fixture.checkout,
                    "hook_event_name": "SessionStart",
                    "source": "startup"
                })
                .to_string(),
                observed_at: "2026-08-28T10:00:03Z".to_owned(),
                launch_token: Some(launch_token),
                process: Some(
                    ProcessIdentity::new(42, fixture.at, &fixture.native, Some(7))
                        .expect("process identity"),
                ),
            })
            .expect("bind planner session");
        ActivePlanning {
            draft: super::FeaturePlanningDraft {
                state: WorkflowState::PlanningActive,
                ..draft
            },
            workflow_token,
        }
    }

    fn proposal(
        active: &ActivePlanning,
        repository_id: workboard_core::RepositoryId,
    ) -> FeatureProposal {
        FeatureProposal {
            feature_body: "# Availability\n\n## Outcome\n\nExpose availability.\n".to_owned(),
            work_items: vec![
                ProposedWorkItem {
                    slug: Slug::new("api").expect("Work-item slug"),
                    title: "Availability API".to_owned(),
                    body: "# Availability API\n\nImplement and verify the endpoint.\n".to_owned(),
                    repository_ids: vec![repository_id],
                    dependencies: Vec::new(),
                },
                ProposedWorkItem {
                    slug: Slug::new("client").expect("Work-item slug"),
                    title: "Availability client".to_owned(),
                    body: "# Availability client\n\nConsume the endpoint.\n".to_owned(),
                    repository_ids: vec![repository_id],
                    dependencies: vec![Slug::new("api").expect("dependency slug")],
                },
            ],
            expected_epic_content_hash: active.draft.epic_content_hash.clone(),
            expected_repository_head: active.draft.repository_head.clone(),
            verification: vec!["Run the workspace test suite".to_owned()],
            first_work_item_slug: Some(Slug::new("api").expect("first Work-item slug")),
        }
    }

    #[test]
    fn plans_approves_and_publishes_a_feature_once() {
        let mut fixture = fixture();
        let active = activate_planning(&mut fixture, Tool::Codex);
        assert!(matches!(
            fixture.app.planning_workflows().submit_proposal(
                active.draft.feature_id,
                "wrong-token",
                proposal(&active, fixture.repository_id),
                "proposal-unauthorised",
                fixture.at + time::Duration::minutes(3),
            ),
            Err(AppError::WorkflowOperationUnauthorized)
        ));
        let first = fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "proposal-availability-v1",
                fixture.at + time::Duration::minutes(3),
            )
            .expect("submit proposal");
        let repeated = fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "proposal-availability-v1",
                fixture.at + time::Duration::minutes(4),
            )
            .expect("repeat proposal");
        assert_eq!(first, repeated);
        assert_eq!(first.state, WorkflowState::AwaitingApproval);
        fixture
            .app
            .planning_workflows()
            .reject_proposal(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(5),
            )
            .expect("reject proposal");
        fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "proposal-availability-v2",
                fixture.at + time::Duration::minutes(6),
            )
            .expect("resubmit proposal");
        fixture
            .app
            .planning_workflows()
            .approve_proposal(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(7),
            )
            .expect("approve proposal");
        let before = git(&fixture.planning_store, &["rev-parse", "HEAD"]);
        let before_count = git(&fixture.planning_store, &["rev-list", "--count", &before])
            .parse::<u32>()
            .expect("planning commit count");
        let published = fixture
            .app
            .planning_workflows()
            .publish_approved(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(8),
            )
            .expect("publish proposal");
        assert_ne!(before, published.commit);
        assert_eq!(published.work_item_ids.len(), 2);
        assert_eq!(
            published.first_work_item_id,
            published.work_item_ids.first().copied()
        );
        assert_eq!(
            fixture
                .app
                .planning_workflows()
                .publish_approved(
                    active.draft.feature_id,
                    fixture.at + time::Duration::minutes(9),
                )
                .expect("repeat publication"),
            published
        );
        let snapshot = fixture
            .app
            .snapshot(fixture.workspace_id)
            .expect("snapshot");
        let feature = snapshot
            .features
            .iter()
            .find(|feature| feature.id == active.draft.feature_id)
            .expect("published Feature");
        assert_eq!(feature.state, WorkflowState::Planned);
        assert!(feature.document_id.is_some());
        assert_eq!(
            snapshot
                .work_items
                .iter()
                .filter(|item| item.feature_id == feature.id)
                .count(),
            2
        );
        let dependencies = fixture
            .app
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT item.proposal_order, dependency.proposal_order
                     FROM work_item_dependencies edge
                     JOIN work_items item ON item.id = edge.work_item_id
                     JOIN work_items dependency ON dependency.id = edge.dependency_work_item_id",
                        [],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("published dependency");
        assert_eq!(dependencies, (1, 0));
        let published_count = git(
            &fixture.planning_store,
            &["rev-list", "--count", &published.commit],
        )
        .parse::<u32>()
        .expect("published commit count");
        assert_eq!(published_count, before_count + 1);
    }

    #[test]
    fn changed_code_head_enters_reconciliation_without_publication() {
        let mut fixture = fixture();
        let active = activate_planning(&mut fixture, Tool::Codex);
        fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "proposal-before-code-change",
                fixture.at + time::Duration::minutes(3),
            )
            .expect("submit proposal");
        fixture
            .app
            .planning_workflows()
            .approve_proposal(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(4),
            )
            .expect("approve proposal");
        fs::write(fixture.code_repository.join("changed.txt"), "changed\n")
            .expect("changed code fixture");
        git(&fixture.code_repository, &["add", "changed.txt"]);
        git(
            &fixture.code_repository,
            &["commit", "-m", "Change baseline"],
        );
        assert!(matches!(
            fixture.app.planning_workflows().publish_approved(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(5),
            ),
            Err(AppError::WorkflowDocumentChanged)
        ));
        let snapshot = fixture
            .app
            .snapshot(fixture.workspace_id)
            .expect("snapshot");
        let feature = snapshot
            .features
            .iter()
            .find(|feature| feature.id == active.draft.feature_id)
            .expect("draft Feature");
        assert_eq!(feature.state, WorkflowState::ReconciliationRequired);
        assert!(feature.document_id.is_none());
        assert!(snapshot.work_items.is_empty());
    }

    #[test]
    fn claude_binding_completes_the_same_provider_neutral_publication() {
        let mut fixture = fixture();
        let active = activate_planning(&mut fixture, Tool::Claude);
        fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "claude-proposal",
                fixture.at + time::Duration::minutes(3),
            )
            .expect("submit Claude proposal");
        fixture
            .app
            .planning_workflows()
            .approve_proposal(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(4),
            )
            .expect("approve Claude proposal");
        let published = fixture
            .app
            .planning_workflows()
            .publish_approved(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(5),
            )
            .expect("publish Claude proposal");
        assert_eq!(published.work_item_ids.len(), 2);
    }

    #[test]
    fn invalid_proposal_and_failed_commit_create_no_false_success() {
        let mut fixture = fixture();
        let active = activate_planning(&mut fixture, Tool::Codex);
        let mut invalid = proposal(&active, fixture.repository_id);
        invalid.feature_body.clear();
        assert!(matches!(
            fixture.app.planning_workflows().submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                invalid,
                "invalid-proposal",
                fixture.at + time::Duration::minutes(3),
            ),
            Err(AppError::PlanningDocumentInvalid(_))
        ));
        let mut cyclic = proposal(&active, fixture.repository_id);
        cyclic.work_items[0].dependencies = vec![cyclic.work_items[1].slug.clone()];
        assert!(matches!(
            fixture.app.planning_workflows().submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                cyclic,
                "cyclic-proposal",
                fixture.at + time::Duration::minutes(3),
            ),
            Err(AppError::PlanningDocumentInvalid(_))
        ));
        let mut duplicate = proposal(&active, fixture.repository_id);
        let duplicate_dependency = duplicate.work_items[0].slug.clone();
        duplicate.work_items[1]
            .dependencies
            .push(duplicate_dependency);
        assert!(matches!(
            fixture.app.planning_workflows().submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                duplicate,
                "duplicate-dependency-proposal",
                fixture.at + time::Duration::minutes(3),
            ),
            Err(AppError::PlanningDocumentInvalid(_))
        ));
        fixture
            .app
            .planning_workflows()
            .submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "valid-proposal",
                fixture.at + time::Duration::minutes(4),
            )
            .expect("submit valid proposal");
        fixture
            .app
            .planning_workflows()
            .approve_proposal(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(5),
            )
            .expect("approve valid proposal");
        let hook = fixture.planning_store.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").expect("rejecting commit hook");
        assert!(matches!(
            fixture.app.planning_workflows().publish_approved(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(6),
            ),
            Err(AppError::PlanningGit { .. })
        ));
        let failed = fixture
            .app
            .snapshot(fixture.workspace_id)
            .expect("failed publication snapshot");
        let feature = failed
            .features
            .iter()
            .find(|feature| feature.id == active.draft.feature_id)
            .expect("draft Feature");
        assert_eq!(feature.state, WorkflowState::Publishing);
        assert!(feature.document_id.is_none());
        assert!(failed.work_items.is_empty());
        fs::remove_file(hook).expect("remove rejecting hook");
        let retried = fixture
            .app
            .planning_workflows()
            .publish_approved(
                active.draft.feature_id,
                fixture.at + time::Duration::minutes(7),
            )
            .expect("retry interrupted publication");
        assert_eq!(retried.work_item_ids.len(), 2);
    }

    #[test]
    fn workflow_credential_expires_independently_of_launch_binding() {
        let mut fixture = fixture();
        let active = activate_planning(&mut fixture, Tool::Codex);
        assert!(matches!(
            fixture.app.planning_workflows().submit_proposal(
                active.draft.feature_id,
                &active.workflow_token,
                proposal(&active, fixture.repository_id),
                "expired-workflow-credential",
                fixture.at + time::Duration::hours(13),
            ),
            Err(AppError::WorkflowOperationUnauthorized)
        ));
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
}
