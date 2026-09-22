use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{
    ConversationId, DocumentId, DocumentKind, FeatureId, HierarchyOwner, ManagedSessionRole,
    RepositoryId, Slug, WorkItemId, WorkItemKey, WorkItemStatus, WorkspaceId,
};

use crate::AppError;
use crate::git::{GitCli, GitWorktreeResolver};
use crate::planning_store::{
    DocumentFrontMatter, NewPlanningDocument, PlanningStore, StoredDocument,
};
use crate::planning_workflow::ProposedWorkItem;
use crate::storage::SqliteStore;
use crate::workflow_operations::WorkflowPrincipal;

const MAX_PROPOSAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeFeatureWorkItems {
    pub feature_id: FeatureId,
    pub work_items: Vec<ProposedWorkItem>,
    pub expected_feature_content_hash: String,
    pub expected_repository_head: String,
    pub idempotency_key: String,
    pub proposed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureWorkItemProposal {
    pub id: String,
    pub workspace_id: WorkspaceId,
    pub feature_id: FeatureId,
    pub repository_id: RepositoryId,
    pub session_id: ConversationId,
    pub work_items: Vec<ProposedWorkItem>,
    pub status: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureWorkItemPublication {
    pub proposal_id: String,
    pub feature_id: FeatureId,
    pub commit: String,
    pub work_item_ids: Vec<WorkItemId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredProposal {
    work_items: Vec<StoredWorkItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredWorkItem {
    work_item_id: WorkItemId,
    document_id: DocumentId,
    proposal: ProposedWorkItem,
}

pub struct FeatureWorkItemPlanningService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> FeatureWorkItemPlanningService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn propose(
        &mut self,
        workflow_token: &str,
        request: ProposeFeatureWorkItems,
    ) -> Result<FeatureWorkItemProposal, AppError> {
        validate_idempotency_key(&request.idempotency_key)?;
        validate_items(&request.work_items)?;
        let principal = crate::workflow_operations::WorkflowOperationService::new(self.store)
            .authenticate(workflow_token, request.proposed_at)?;
        authorize(&principal, request.feature_id)?;
        let repository_id = self.repository_id_for_checkout(principal.checkout_id)?;
        if let Some(existing) = self.by_idempotency(&request.idempotency_key)? {
            if existing.feature_id != request.feature_id
                || existing.work_items != request.work_items
                || existing.repository_id != repository_id
            {
                return Err(AppError::IdempotencyConflict);
            }
            return Ok(existing);
        }
        let context =
            self.proposal_context(request.feature_id, repository_id, principal.checkout_id)?;
        if context.feature_hash != request.expected_feature_content_hash {
            return Err(AppError::WorkflowDocumentChanged);
        }
        if GitCli.resolve(&context.repository_path)?.head_oid != request.expected_repository_head {
            return Err(AppError::CheckoutReconciliation {
                code: "planning_repository_changed".to_owned(),
                message: "the assigned repository changed before the proposal was submitted"
                    .to_owned(),
            });
        }
        validate_against_existing(&request.work_items, &context.existing_slugs)?;
        validate_repositories(&request.work_items, &context.repository_ids)?;
        let stored = StoredProposal {
            work_items: request
                .work_items
                .iter()
                .cloned()
                .map(|proposal| StoredWorkItem {
                    work_item_id: WorkItemId::generate(),
                    document_id: DocumentId::generate(),
                    proposal,
                })
                .collect(),
        };
        let proposal_id = uuid::Uuid::new_v4().to_string();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO feature_work_item_proposals (
                     id, workspace_id, feature_id, repository_id, session_id, idempotency_key,
                     proposal_json, observed_feature_hash, observed_repository_head, status,
                     created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'awaiting_approval', ?10)",
                params![
                    proposal_id,
                    principal.workspace_id.to_string(),
                    request.feature_id.to_string(),
                    repository_id.to_string(),
                    principal.session_id.to_string(),
                    request.idempotency_key,
                    serde_json::to_string(&stored)?,
                    request.expected_feature_content_hash,
                    request.expected_repository_head,
                    timestamp(request.proposed_at),
                ],
            )?;
            Ok(())
        })?;
        self.get(&proposal_id)
    }

    pub fn list(
        &self,
        workspace_id: WorkspaceId,
        feature_id: Option<FeatureId>,
    ) -> Result<Vec<FeatureWorkItemProposal>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id FROM feature_work_item_proposals
                 WHERE workspace_id = ?1 AND (?2 IS NULL OR feature_id = ?2)
                 ORDER BY created_at DESC, id DESC",
            )?;
            let ids = statement
                .query_map(
                    params![
                        workspace_id.to_string(),
                        feature_id.map(|id| id.to_string())
                    ],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|id| read_proposal(connection, &id))
                .collect()
        })
    }

    pub fn reject(
        &mut self,
        proposal_id: &str,
        decided_at: OffsetDateTime,
    ) -> Result<FeatureWorkItemProposal, AppError> {
        self.store.write(|transaction| {
            let changed = transaction.execute(
                "UPDATE feature_work_item_proposals SET status = 'rejected', decided_at = ?2
                 WHERE id = ?1 AND status = 'awaiting_approval'",
                params![proposal_id, timestamp(decided_at)],
            )?;
            if changed == 0 {
                return Err(AppError::Domain(
                    "Work-item proposal is not awaiting approval".to_owned(),
                ));
            }
            Ok(())
        })?;
        self.get(proposal_id)
    }

    pub fn approve(
        &mut self,
        proposal_id: &str,
        decided_at: OffsetDateTime,
    ) -> Result<FeatureWorkItemPublication, AppError> {
        if let Some(outcome) = self.published_outcome(proposal_id)? {
            return Ok(outcome);
        }
        let context = self.publication_context(proposal_id)?;
        let documents = context.documents()?;
        let planning_store = PlanningStore::create_or_link(&context.planning_store_path)?;
        let stored_documents = planning_store.publish_batch_new(
            &documents,
            &format!("Add Work items to {}", context.feature_slug),
        )?;
        let commit = stored_documents
            .first()
            .and_then(|document| document.observed_commit.clone())
            .ok_or_else(|| AppError::Domain("planning-store commit is unavailable".to_owned()))?;
        let outcome = FeatureWorkItemPublication {
            proposal_id: proposal_id.to_owned(),
            feature_id: context.feature_id,
            commit,
            work_item_ids: context
                .stored
                .work_items
                .iter()
                .map(|item| item.work_item_id)
                .collect(),
        };
        let at = timestamp(decided_at);
        self.store.write(|transaction| {
            for (offset, (item, document)) in context
                .stored
                .work_items
                .iter()
                .zip(stored_documents.iter())
                .enumerate()
            {
                let key = WorkItemKey::new(format!(
                    "{}/{}/{}",
                    context.epic_slug, context.feature_slug, item.proposal.slug
                ))
                .map_err(|error| AppError::Domain(error.to_string()))?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at, proposal_order
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'backlog', ?6, ?7)",
                    params![
                        item.work_item_id.to_string(),
                        context.feature_id.to_string(),
                        key.as_str(),
                        item.proposal.slug.as_str(),
                        item.proposal.title,
                        at,
                        context.next_order
                            + i64::try_from(offset)
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
                insert_document(
                    transaction,
                    context.planning_repository_id,
                    item.work_item_id,
                    document,
                    &at,
                )?;
            }
            for item in &context.stored.work_items {
                for (order, dependency_slug) in item.proposal.dependencies.iter().enumerate() {
                    let dependency_id = context
                        .stored
                        .work_items
                        .iter()
                        .find(|candidate| candidate.proposal.slug == *dependency_slug)
                        .map(|candidate| candidate.work_item_id)
                        .or_else(|| context.existing_dependencies.get(dependency_slug).copied())
                        .ok_or_else(|| {
                            AppError::PlanningDocumentInvalid(format!(
                                "Work-item dependency {dependency_slug} is missing"
                            ))
                        })?;
                    transaction.execute(
                        "INSERT INTO work_item_dependencies (
                             work_item_id, dependency_work_item_id, dependency_order
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            item.work_item_id.to_string(),
                            dependency_id.to_string(),
                            i64::try_from(order)
                                .map_err(|error| AppError::Domain(error.to_string()))?,
                        ],
                    )?;
                }
            }
            transaction.execute(
                "UPDATE feature_work_item_proposals
                 SET status = 'approved', outcome_json = ?2, decided_at = ?3
                 WHERE id = ?1 AND status = 'awaiting_approval'",
                params![proposal_id, serde_json::to_string(&outcome)?, at],
            )?;
            Ok(())
        })?;
        Ok(outcome)
    }

    fn proposal_context(
        &self,
        feature_id: FeatureId,
        repository_id: RepositoryId,
        checkout_id: workboard_core::CheckoutId,
    ) -> Result<ProposalContext, AppError> {
        self.store.read(|connection| {
            let (state, feature_hash, repository_path) = connection
                .query_row(
                    "SELECT feature.workflow_state, document.content_hash, path.path
                     FROM features feature
                     JOIN epics epic ON epic.id = feature.epic_id
                     JOIN documents document ON document.feature_id = feature.id
                     JOIN feature_checkouts feature_checkout
                       ON feature_checkout.feature_id = feature.id
                      AND feature_checkout.repository_id = ?2
                      AND feature_checkout.checkout_id = ?3
                     JOIN checkouts checkout
                       ON checkout.id = feature_checkout.checkout_id
                      AND checkout.repository_id = ?2
                      AND checkout.availability = 'available'
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     WHERE feature.id = ?1",
                    params![
                        feature_id.to_string(),
                        repository_id.to_string(),
                        checkout_id.to_string()
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            PathBuf::from(row.get::<_, String>(2)?),
                        ))
                    },
                )
                .optional()?
                .ok_or(AppError::ResumeRepositoryMismatch)?;
            if state != "planned" {
                return Err(AppError::Domain(
                    "Work items can only be added to a published Feature".to_owned(),
                ));
            }
            Ok(ProposalContext {
                feature_hash,
                repository_path,
                existing_slugs: read_existing_slugs(connection, feature_id)?,
                repository_ids: read_repository_ids(connection, feature_id)?,
            })
        })
    }

    fn publication_context(&self, proposal_id: &str) -> Result<PublicationContext, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT workspace.slug, epic.slug, feature.slug, feature.id,
                            workspace.planning_store_repository_id, planning_path.path,
                            proposal.proposal_json
                     FROM feature_work_item_proposals proposal
                     JOIN features feature ON feature.id = proposal.feature_id
                     JOIN epics epic ON epic.id = feature.epic_id
                     JOIN workspaces workspace ON workspace.id = epic.workspace_id
                     JOIN repository_paths planning_path
                       ON planning_path.repository_id = workspace.planning_store_repository_id
                      AND planning_path.observed_until IS NULL
                     WHERE proposal.id = ?1 AND proposal.status = 'awaiting_approval'",
                    [proposal_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| {
                    AppError::Domain("Work-item proposal is not awaiting approval".to_owned())
                })?;
            let feature_id = parse_id(&row.3)?;
            let stored: StoredProposal = serde_json::from_str(&row.6)?;
            let repository_ids = stored
                .work_items
                .iter()
                .flat_map(|item| item.proposal.repository_ids.iter().copied())
                .collect::<HashSet<_>>();
            Ok(PublicationContext {
                workspace_slug: parse_slug(&row.0)?,
                epic_slug: parse_slug(&row.1)?,
                feature_slug: parse_slug(&row.2)?,
                feature_id,
                planning_repository_id: parse_id(&row.4)?,
                planning_store_path: PathBuf::from(&row.5),
                repository_slugs: read_repository_slugs(connection, feature_id, &repository_ids)?,
                existing_dependencies: read_existing_dependencies(connection, feature_id)?,
                next_order: connection.query_row(
                    "SELECT COALESCE(MAX(proposal_order) + 1, 0)
                     FROM work_items WHERE feature_id = ?1",
                    [feature_id.to_string()],
                    |row| row.get(0),
                )?,
                stored,
            })
        })
    }

    fn published_outcome(
        &self,
        proposal_id: &str,
    ) -> Result<Option<FeatureWorkItemPublication>, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT outcome_json FROM feature_work_item_proposals
                     WHERE id = ?1 AND status = 'approved'",
                    [proposal_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|json| serde_json::from_str(&json).map_err(Into::into))
                .transpose()
        })
    }

    fn get(&self, proposal_id: &str) -> Result<FeatureWorkItemProposal, AppError> {
        self.store
            .read(|connection| read_proposal(connection, proposal_id))
    }

    fn repository_id_for_checkout(
        &self,
        checkout_id: workboard_core::CheckoutId,
    ) -> Result<RepositoryId, AppError> {
        self.store.read(|connection| {
            let value = connection
                .query_row(
                    "SELECT repository_id FROM checkouts WHERE id = ?1",
                    [checkout_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(AppError::ResumeCheckoutRequired)?;
            parse_id(&value)
        })
    }

    fn by_idempotency(&self, key: &str) -> Result<Option<FeatureWorkItemProposal>, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT id FROM feature_work_item_proposals WHERE idempotency_key = ?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|id| read_proposal(connection, &id))
                .transpose()
        })
    }
}

struct ProposalContext {
    feature_hash: String,
    repository_path: PathBuf,
    existing_slugs: HashSet<Slug>,
    repository_ids: HashSet<RepositoryId>,
}

struct PublicationContext {
    workspace_slug: Slug,
    epic_slug: Slug,
    feature_slug: Slug,
    feature_id: FeatureId,
    planning_repository_id: RepositoryId,
    planning_store_path: PathBuf,
    stored: StoredProposal,
    repository_slugs: HashMap<RepositoryId, Slug>,
    existing_dependencies: HashMap<Slug, WorkItemId>,
    next_order: i64,
}

impl PublicationContext {
    fn documents(&self) -> Result<Vec<NewPlanningDocument>, AppError> {
        self.stored
            .work_items
            .iter()
            .map(|item| {
                let repositories = item
                    .proposal
                    .repository_ids
                    .iter()
                    .map(|id| {
                        self.repository_slugs
                            .get(id)
                            .cloned()
                            .ok_or(AppError::WorkItemRepositoryMismatch)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(NewPlanningDocument {
                    relative_path: PlanningStore::work_item_path(
                        &self.workspace_slug,
                        &self.epic_slug,
                        &self.feature_slug,
                        &item.proposal.slug,
                    ),
                    front_matter: DocumentFrontMatter {
                        id: item.document_id,
                        kind: DocumentKind::WorkItem,
                        key: format!(
                            "{}/{}/{}",
                            self.epic_slug, self.feature_slug, item.proposal.slug
                        ),
                        status: Some(WorkItemStatus::Backlog),
                        repositories,
                    },
                    body: item.proposal.body.clone(),
                })
            })
            .collect()
    }
}

fn authorize(principal: &WorkflowPrincipal, feature_id: FeatureId) -> Result<(), AppError> {
    if principal.owner != HierarchyOwner::Feature(feature_id)
        || principal.role != ManagedSessionRole::FeaturePlanning
    {
        return Err(AppError::WorkflowOperationUnauthorized);
    }
    Ok(())
}

fn validate_items(items: &[ProposedWorkItem]) -> Result<(), AppError> {
    if items.is_empty() || serde_json::to_vec(items)?.len() > MAX_PROPOSAL_BYTES {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item proposal is empty or too large".to_owned(),
        ));
    }
    let slugs = items
        .iter()
        .map(|item| item.slug.clone())
        .collect::<HashSet<_>>();
    if slugs.len() != items.len() {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item slugs must be unique".to_owned(),
        ));
    }
    for item in items {
        if item.title.trim().is_empty()
            || item.title.len() > 200
            || item.title.chars().any(char::is_control)
            || item.body.trim().is_empty()
            || item.repository_ids.is_empty()
        {
            return Err(AppError::PlanningDocumentInvalid(
                "each Work item needs a valid title, body, and repository".to_owned(),
            ));
        }
        if item.repository_ids.iter().collect::<HashSet<_>>().len() != item.repository_ids.len()
            || item.dependencies.iter().collect::<HashSet<_>>().len() != item.dependencies.len()
            || item
                .dependencies
                .iter()
                .any(|dependency| dependency == &item.slug)
        {
            return Err(AppError::PlanningDocumentInvalid(
                "Work-item repositories and dependencies must be unique and cannot be self-referential"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_against_existing(
    items: &[ProposedWorkItem],
    existing: &HashSet<Slug>,
) -> Result<(), AppError> {
    let proposed = items
        .iter()
        .map(|item| item.slug.clone())
        .collect::<HashSet<_>>();
    if proposed.iter().any(|slug| existing.contains(slug)) {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item slug already exists in this Feature".to_owned(),
        ));
    }
    for item in items {
        if item
            .dependencies
            .iter()
            .any(|dependency| !proposed.contains(dependency) && !existing.contains(dependency))
        {
            return Err(AppError::PlanningDocumentInvalid(
                "Work-item dependency is not in this Feature".to_owned(),
            ));
        }
    }
    let graph = items
        .iter()
        .map(|item| {
            (
                item.slug.clone(),
                item.dependencies
                    .iter()
                    .filter(|dependency| proposed.contains(*dependency))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for slug in graph.keys() {
        visit(slug, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit(
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
        visit(dependency, graph, visiting, visited)?;
    }
    visiting.remove(slug);
    visited.insert(slug.clone());
    Ok(())
}

fn validate_repositories(
    items: &[ProposedWorkItem],
    repositories: &HashSet<RepositoryId>,
) -> Result<(), AppError> {
    if items
        .iter()
        .flat_map(|item| &item.repository_ids)
        .any(|id| !repositories.contains(id))
    {
        Err(AppError::WorkItemRepositoryMismatch)
    } else {
        Ok(())
    }
}

fn read_proposal(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<FeatureWorkItemProposal, AppError> {
    let row = connection
        .query_row(
            "SELECT id, workspace_id, feature_id, repository_id, session_id,
                    proposal_json, status, created_at
             FROM feature_work_item_proposals WHERE id = ?1",
            [id],
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
        .ok_or_else(|| AppError::Domain("Work-item proposal was not found".to_owned()))?;
    let stored: StoredProposal = serde_json::from_str(&row.5)?;
    Ok(FeatureWorkItemProposal {
        id: row.0,
        workspace_id: parse_id(&row.1)?,
        feature_id: parse_id(&row.2)?,
        repository_id: parse_id(&row.3)?,
        session_id: parse_id(&row.4)?,
        work_items: stored
            .work_items
            .into_iter()
            .map(|item| item.proposal)
            .collect(),
        status: row.6,
        created_at: parse_timestamp(&row.7)?,
    })
}

fn read_existing_slugs(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
) -> Result<HashSet<Slug>, AppError> {
    let mut statement = connection.prepare("SELECT slug FROM work_items WHERE feature_id = ?1")?;
    let values = statement
        .query_map([feature_id.to_string()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    values.into_iter().map(|slug| parse_slug(&slug)).collect()
}

fn read_existing_dependencies(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
) -> Result<HashMap<Slug, WorkItemId>, AppError> {
    let mut statement =
        connection.prepare("SELECT slug, id FROM work_items WHERE feature_id = ?1")?;
    let values = statement
        .query_map([feature_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    values
        .into_iter()
        .map(|(slug, id)| Ok((parse_slug(&slug)?, parse_id(&id)?)))
        .collect()
}

fn read_repository_ids(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
) -> Result<HashSet<RepositoryId>, AppError> {
    let mut statement = connection.prepare(
        "SELECT repository.id
         FROM repositories repository
         JOIN features feature
         JOIN epics epic ON epic.id = feature.epic_id
         WHERE feature.id = ?1 AND repository.workspace_id = epic.workspace_id
           AND repository.is_planning_store = 0",
    )?;
    let values = statement
        .query_map([feature_id.to_string()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    values.into_iter().map(|id| parse_id(&id)).collect()
}

fn read_repository_slugs(
    connection: &rusqlite::Connection,
    feature_id: FeatureId,
    ids: &HashSet<RepositoryId>,
) -> Result<HashMap<RepositoryId, Slug>, AppError> {
    let available = read_repository_ids(connection, feature_id)?;
    if ids.iter().any(|id| !available.contains(id)) {
        return Err(AppError::WorkItemRepositoryMismatch);
    }
    ids.iter()
        .map(|id| {
            let slug: String = connection.query_row(
                "SELECT slug FROM repositories WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )?;
            Ok((*id, parse_slug(&slug)?))
        })
        .collect()
}

fn insert_document(
    transaction: &Transaction<'_>,
    repository_id: RepositoryId,
    work_item_id: WorkItemId,
    document: &StoredDocument,
    observed_at: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO documents (
             id, repository_id, work_item_id, kind, relative_path, content_hash,
             observed_commit, observed_at
         ) VALUES (?1, ?2, ?3, 'work_item', ?4, ?5, ?6, ?7)",
        params![
            document.front_matter.id.to_string(),
            repository_id.to_string(),
            work_item_id.to_string(),
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

pub fn work_item_planner_prompt(feature_id: FeatureId, request: &str) -> String {
    format!(
        "Use the installed Agent Workboard workflow to add Work items to Feature {feature_id}. Read the assigned hierarchy, discuss and refine this request with the user: {request}. When the user is satisfied, submit the complete Work-item proposal through work_items_propose. Do not create planning documents or execution sessions directly."
    )
}

fn validate_idempotency_key(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(AppError::EmptyIdempotencyKey)
    } else {
        Ok(())
    }
}

fn parse_slug(value: &str) -> Result<Slug, AppError> {
    Slug::new(value).map_err(|error| AppError::Domain(error.to_string()))
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
    value.unix_timestamp_nanos().to_string()
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, AppError> {
    let value = value
        .parse::<i128>()
        .map_err(|error| AppError::Domain(error.to_string()))?;
    OffsetDateTime::from_unix_timestamp_nanos(value)
        .map_err(|error| AppError::Domain(error.to_string()))
}
