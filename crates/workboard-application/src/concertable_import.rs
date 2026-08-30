use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    DocumentId, DocumentKind, EpicId, FeatureId, ImportBatchId, RepositoryId, Slug, WorkItemId,
    WorkItemKey, WorkItemStatus, WorkspaceId,
};

use crate::AppError;
use crate::git::{GitCli, GitWorktreeResolver};
use crate::planning_store::{DocumentFrontMatter, NewPlanningDocument, PlanningStore};
use crate::workspace::WorkboardApplication;

pub const CONCERTABLE_IMPORT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    pub relative_path: PathBuf,
    pub content_hash: String,
    pub first_line: Option<u32>,
    pub last_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcertableWorkItemImport {
    pub selected: bool,
    pub id: WorkItemId,
    pub document_id: DocumentId,
    pub slug: Slug,
    pub title: String,
    pub status: WorkItemStatus,
    pub body: String,
    pub source: SourceReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcertableFeatureImport {
    pub selected: bool,
    pub id: FeatureId,
    pub document_id: DocumentId,
    pub slug: Slug,
    pub title: String,
    pub body: String,
    pub source: SourceReference,
    pub work_items: Vec<ConcertableWorkItemImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcertableEpicImport {
    pub selected: bool,
    pub id: EpicId,
    pub document_id: DocumentId,
    pub slug: Slug,
    pub title: String,
    pub body: String,
    pub source: Option<SourceReference>,
    pub features: Vec<ConcertableFeatureImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcertableImportPreview {
    pub format_version: u32,
    pub source_repository: PathBuf,
    pub source_head: String,
    pub epics: Vec<ConcertableEpicImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcertableImportOutcome {
    pub import_id: ImportBatchId,
    pub preview_hash: String,
    pub planning_commit: String,
    pub epics: usize,
    pub features: usize,
    pub work_items: usize,
    pub source_destinations: usize,
    pub already_applied: bool,
}

#[derive(Debug, Clone)]
struct SourceFile {
    relative_path: PathBuf,
    content_hash: String,
    title: String,
    body: String,
}

#[derive(Debug, Clone, Copy)]
enum ImportedOwner {
    Epic(EpicId),
    Feature(FeatureId),
    WorkItem(WorkItemId),
}

#[derive(Debug)]
struct PreparedDocument {
    owner: ImportedOwner,
    source: Option<SourceReference>,
    document: NewPlanningDocument,
}

pub fn preview_concertable_plans(repository: &Path) -> Result<ConcertableImportPreview, AppError> {
    let root = git_root(repository)?;
    let source_head = git_text(&root, ["rev-parse", "--verify", "HEAD"])?;
    let plans_root = root.join("plans");
    if !plans_root.is_dir() {
        return Err(AppError::Domain(format!(
            "Concertable planning directory is unavailable: {}",
            plans_root.display()
        )));
    }
    let mut files = Vec::new();
    let mut visited_directories = HashSet::new();
    collect_markdown(&plans_root, &root, &mut visited_directories, &mut files)?;
    files.sort();
    let mut roadmaps = Vec::new();
    let mut plans = Vec::new();
    let mut progress_by_plan = HashMap::new();
    for relative_path in files {
        let name = relative_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name.ends_with("_ROADMAP.md") {
            roadmaps.push(read_source(&root, relative_path)?);
        } else if name.ends_with("_PLAN.md") {
            plans.push(read_source(&root, relative_path)?);
        } else if name.ends_with("_PROGRESS.md") {
            let key = document_pair_key(&relative_path, "_PROGRESS.md");
            if progress_by_plan
                .insert(key.clone(), read_source(&root, relative_path)?)
                .is_some()
            {
                return Err(AppError::Domain(format!(
                    "duplicate Concertable progress document key: {}",
                    key.display()
                )));
            }
        }
    }
    if plans.is_empty() {
        return Err(AppError::Domain(
            "no Concertable *_PLAN.md documents were found".to_owned(),
        ));
    }

    let mut epic_by_directory = HashMap::new();
    let mut epics = Vec::new();
    let mut epic_slugs = HashSet::new();
    for roadmap in roadmaps {
        let directory = roadmap
            .relative_path
            .parent()
            .unwrap_or(Path::new("plans"))
            .to_path_buf();
        let slug = unique_slug(
            derive_slug(&roadmap.title, roadmap.relative_path.file_stem()),
            &mut epic_slugs,
        )?;
        epic_by_directory.insert(directory, epics.len());
        epics.push(ConcertableEpicImport {
            selected: true,
            id: EpicId::generate(),
            document_id: DocumentId::generate(),
            slug,
            title: roadmap.title.clone(),
            body: roadmap.body.clone(),
            source: Some(source_reference(&roadmap, None, None)),
            features: Vec::new(),
        });
    }

    for plan in plans {
        let directory = plan
            .relative_path
            .parent()
            .unwrap_or(Path::new("plans"))
            .to_path_buf();
        let epic_index = match epic_by_directory.get(&directory).copied() {
            Some(index) => index,
            None => {
                let directory_name = directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("imported-plans");
                let title = title_case(directory_name);
                let slug = unique_slug(derive_slug(&title, None), &mut epic_slugs)?;
                let index = epics.len();
                epic_by_directory.insert(directory.clone(), index);
                epics.push(ConcertableEpicImport {
                    selected: true,
                    id: EpicId::generate(),
                    document_id: DocumentId::generate(),
                    slug,
                    title: title.clone(),
                    body: format!(
                        "# {title}\n\nImported plans from `{}`.\n",
                        directory.display()
                    ),
                    source: None,
                    features: Vec::new(),
                });
                index
            }
        };
        let feature_slugs = epics[epic_index]
            .features
            .iter()
            .map(|feature| feature.slug.to_string())
            .collect::<HashSet<_>>();
        let mut feature_slugs = feature_slugs;
        let slug = unique_slug(
            derive_slug(&plan.title, plan.relative_path.file_stem()),
            &mut feature_slugs,
        )?;
        let mut work_items = phase_work_items(&plan)?;
        if let Some(progress) =
            progress_by_plan.remove(&document_pair_key(&plan.relative_path, "_PLAN.md"))
        {
            let mut used = work_items
                .iter()
                .map(|item| item.slug.to_string())
                .collect::<HashSet<_>>();
            work_items.push(ConcertableWorkItemImport {
                selected: true,
                id: WorkItemId::generate(),
                document_id: DocumentId::generate(),
                slug: unique_slug("imported-progress".to_owned(), &mut used)?,
                title: format!("{} progress and handoff", plan.title),
                status: WorkItemStatus::Ready,
                body: progress.body.clone(),
                source: source_reference(&progress, None, None),
            });
        }
        if work_items.is_empty() {
            work_items.push(ConcertableWorkItemImport {
                selected: true,
                id: WorkItemId::generate(),
                document_id: DocumentId::generate(),
                slug: Slug::new("execute-plan")
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                title: format!("Execute {}", plan.title),
                status: WorkItemStatus::Ready,
                body: format!(
                    "# Execute {}\n\nUse the imported Feature document as the complete plan.\n",
                    plan.title
                ),
                source: source_reference(&plan, None, None),
            });
        }
        epics[epic_index].features.push(ConcertableFeatureImport {
            selected: true,
            id: FeatureId::generate(),
            document_id: DocumentId::generate(),
            slug,
            title: plan.title.clone(),
            body: plan.body.clone(),
            source: source_reference(&plan, None, None),
            work_items,
        });
    }
    if !progress_by_plan.is_empty() {
        let mut unmatched = progress_by_plan
            .into_values()
            .map(|progress| progress.relative_path)
            .collect::<Vec<_>>();
        unmatched.sort();
        return Err(AppError::Domain(format!(
            "Concertable progress documents have no matching plan: {}",
            unmatched
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    epics.sort_by(|left, right| left.slug.as_str().cmp(right.slug.as_str()));
    for epic in &mut epics {
        epic.features
            .sort_by(|left, right| left.slug.as_str().cmp(right.slug.as_str()));
    }
    Ok(ConcertableImportPreview {
        format_version: CONCERTABLE_IMPORT_FORMAT_VERSION,
        source_repository: root,
        source_head,
        epics,
    })
}

impl WorkboardApplication {
    pub fn apply_concertable_import(
        &mut self,
        workspace_id: WorkspaceId,
        repository_id: RepositoryId,
        preview: &ConcertableImportPreview,
    ) -> Result<ConcertableImportOutcome, AppError> {
        validate_preview(preview)?;
        let preview_hash = hash_bytes(&serde_json::to_vec(preview)?);
        if let Some(outcome) =
            self.existing_concertable_import(workspace_id, repository_id, &preview_hash)?
        {
            return Ok(outcome);
        }
        validate_source(preview)?;
        let (workspace_slug, planning_repository_id, planning_store_path) =
            self.workspace_planning_store(workspace_id)?;
        let (repository_slug, repository_common_directory) = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT slug, git_common_directory FROM repositories
                     WHERE id = ?1 AND workspace_id = ?2 AND is_planning_store = 0",
                    params![repository_id.to_string(), workspace_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
                .ok_or_else(|| {
                    AppError::Domain(
                        "import repository is not registered in the workspace".to_owned(),
                    )
                })
        })?;
        let source_repository = GitCli.resolve(&preview.source_repository)?;
        if !paths_equal(
            &source_repository.common_dir,
            Path::new(&repository_common_directory),
        ) {
            return Err(AppError::Domain(
                "Concertable import target does not match the source repository".to_owned(),
            ));
        }
        let repository_slug =
            Slug::new(repository_slug).map_err(|error| AppError::Domain(error.to_string()))?;
        let prepared = prepare_documents(&workspace_slug, &repository_slug, preview)?;
        self.preflight_import_collisions(workspace_id, planning_repository_id, preview, &prepared)?;
        let planning_store = PlanningStore::create_or_link(&planning_store_path)?;
        let new_documents = prepared
            .iter()
            .map(|document| document.document.clone())
            .collect::<Vec<_>>();
        let stored = planning_store.publish_batch_new(
            &new_documents,
            &format!("Import Concertable plans from {}", preview.source_head),
        )?;
        let planning_commit = stored
            .first()
            .and_then(|document| document.observed_commit.clone())
            .ok_or_else(|| AppError::PlanningGit {
                message: "Concertable import produced no planning commit".to_owned(),
            })?;
        let import_id = ImportBatchId::generate();
        let imported_at = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        let counts = selected_counts(preview);
        let source_destinations = prepared
            .iter()
            .filter(|document| document.source.is_some())
            .count();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (?1, ?2, ?3, 'concertable_plans', ?4, ?5, ?6, ?7, ?8)",
                params![
                    import_id.to_string(),
                    workspace_id.to_string(),
                    repository_id.to_string(),
                    path_text(&preview.source_repository)?,
                    preview.source_head,
                    preview_hash,
                    planning_commit,
                    imported_at,
                ],
            )?;
            for epic in preview.epics.iter().filter(|epic| epic.selected) {
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        epic.id.to_string(),
                        workspace_id.to_string(),
                        epic.slug.as_str(),
                        epic.title,
                        imported_at,
                    ],
                )?;
                for feature in epic.features.iter().filter(|feature| feature.selected) {
                    transaction.execute(
                        "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                         VALUES (?1, ?2, ?3, ?4, 'planned', ?5)",
                        params![
                            feature.id.to_string(),
                            epic.id.to_string(),
                            feature.slug.as_str(),
                            feature.title,
                            imported_at,
                        ],
                    )?;
                    for item in feature.work_items.iter().filter(|item| item.selected) {
                        let key = WorkItemKey::new(format!(
                            "{}/{}/{}",
                            epic.slug, feature.slug, item.slug
                        ))
                        .map_err(|error| AppError::Domain(error.to_string()))?;
                        transaction.execute(
                            "INSERT INTO work_items (
                                 id, feature_id, key, slug, title, status, created_at
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            params![
                                item.id.to_string(),
                                feature.id.to_string(),
                                key.as_str(),
                                item.slug.as_str(),
                                item.title,
                                status_name(item.status),
                                imported_at,
                            ],
                        )?;
                        transaction.execute(
                            "INSERT INTO work_item_repositories (work_item_id, repository_id)
                             VALUES (?1, ?2)",
                            params![item.id.to_string(), repository_id.to_string()],
                        )?;
                    }
                }
            }
            for (prepared, stored) in prepared.iter().zip(&stored) {
                let (epic_id, feature_id, work_item_id, kind, destination_kind, destination_id) =
                    match prepared.owner {
                        ImportedOwner::Epic(id) => (
                            Some(id.to_string()),
                            None,
                            None,
                            "epic",
                            "epic",
                            id.to_string(),
                        ),
                        ImportedOwner::Feature(id) => (
                            None,
                            Some(id.to_string()),
                            None,
                            "feature",
                            "feature",
                            id.to_string(),
                        ),
                        ImportedOwner::WorkItem(id) => (
                            None,
                            None,
                            Some(id.to_string()),
                            "work_item",
                            "work_item",
                            id.to_string(),
                        ),
                    };
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, feature_id, work_item_id, kind,
                         relative_path, content_hash, observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        stored.front_matter.id.to_string(),
                        planning_repository_id.to_string(),
                        epic_id,
                        feature_id,
                        work_item_id,
                        kind,
                        path_text(&stored.relative_path)?,
                        stored.content_hash,
                        stored.observed_commit,
                        imported_at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, ?4)",
                    params![
                        stored.front_matter.id.to_string(),
                        stored.content_hash,
                        stored.observed_commit,
                        imported_at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_memberships (
                         import_id, document_id, destination_kind
                     ) VALUES (?1, ?2, ?3)",
                    params![
                        import_id.to_string(),
                        stored.front_matter.id.to_string(),
                        destination_kind,
                    ],
                )?;
                if let Some(source) = &prepared.source {
                    transaction.execute(
                        "INSERT INTO import_source_destinations (
                             import_id, source_path, source_hash, destination_kind,
                             destination_id, document_id
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            import_id.to_string(),
                            path_text(&source.relative_path)?,
                            source.content_hash,
                            destination_kind,
                            destination_id,
                            stored.front_matter.id.to_string(),
                        ],
                    )?;
                }
            }
            for (prepared, stored) in prepared.iter().zip(&stored) {
                let (source_provenance, source_path, source_hash) = match &prepared.source {
                    Some(source) => (
                        "source",
                        Some(path_text(&source.relative_path)?.to_owned()),
                        Some(source.content_hash.clone()),
                    ),
                    None => {
                        transaction.execute(
                            "INSERT INTO import_document_synthetic_attestations (
                                 import_id, document_id, authority, attested_at
                             ) VALUES (?1, ?2, 'import_preview', ?3)",
                            params![
                                import_id.to_string(),
                                stored.front_matter.id.to_string(),
                                imported_at,
                            ],
                        )?;
                        ("synthetic", None, None)
                    }
                };
                transaction.execute(
                    "INSERT INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
                    params![
                        import_id.to_string(),
                        stored.front_matter.id.to_string(),
                        stored.content_hash,
                        source_provenance,
                        source_path,
                        source_hash,
                    ],
                )?;
            }
            transaction.execute(
                "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                 VALUES (?1, ?2)",
                params![import_id.to_string(), imported_at],
            )?;
            Ok(())
        })?;
        Ok(ConcertableImportOutcome {
            import_id,
            preview_hash,
            planning_commit,
            epics: counts.0,
            features: counts.1,
            work_items: counts.2,
            source_destinations,
            already_applied: false,
        })
    }

    fn existing_concertable_import(
        &self,
        workspace_id: WorkspaceId,
        repository_id: RepositoryId,
        preview_hash: &str,
    ) -> Result<Option<ConcertableImportOutcome>, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT batch.id, batch.planning_commit, finalization.import_id,
                            membership_failure.import_id, cohort_failure.import_id,
                            evidence_failure.import_id, provenance_failure.import_id
                       FROM import_batches batch
                       LEFT JOIN import_document_membership_finalizations finalization
                         ON finalization.import_id = batch.id
                       LEFT JOIN concertable_import_membership_evidence_failures membership_failure
                         ON membership_failure.import_id = batch.id
                       LEFT JOIN concertable_import_cohort_evidence_failures cohort_failure
                         ON cohort_failure.import_id = batch.id
                       LEFT JOIN concertable_import_durable_evidence_failures evidence_failure
                         ON evidence_failure.import_id = batch.id
                       LEFT JOIN concertable_import_provenance_consistency_failures provenance_failure
                         ON provenance_failure.import_id = batch.id
                     WHERE batch.preview_hash = ?1 AND batch.kind = 'concertable_plans'
                       AND batch.workspace_id = ?2
                       AND batch.repository_id = ?3",
                    params![
                        preview_hash,
                        workspace_id.to_string(),
                        repository_id.to_string(),
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                        ))
                    },
                )
                .optional()?;
            row.map(
                |(
                    id,
                    planning_commit,
                    finalization,
                    membership_failure,
                    cohort_failure,
                    evidence_failure,
                    provenance_failure,
                )| {
                if finalization.is_none() {
                    return Err(AppError::Domain(format!(
                        "Concertable import batch {id} has no finalized document membership"
                    )));
                }
                if membership_failure.is_some()
                    || cohort_failure.is_some()
                    || evidence_failure.is_some()
                    || provenance_failure.is_some()
                {
                    return Err(AppError::Domain(format!(
                        "Concertable import batch {id} has invalid finalized document membership evidence"
                    )));
                }
                let planning_commit = planning_commit.ok_or_else(|| {
                    AppError::Domain(format!(
                        "Concertable import batch {id} has no planning commit"
                    ))
                })?;
                let import_id = id
                    .parse()
                    .map_err(|error| AppError::Domain(format!("invalid import ID: {error}")))?;
                let mut counts = [0_usize; 3];
                for (index, kind) in ["epic", "feature", "work_item"].iter().enumerate() {
                    let count: i64 = connection.query_row(
                        "SELECT COUNT(*) FROM import_document_memberships
                          WHERE import_id = ?1 AND destination_kind = ?2",
                        params![id, kind],
                        |row| row.get(0),
                    )?;
                    counts[index] = usize::try_from(count)
                        .map_err(|_| AppError::Domain("invalid import count".to_owned()))?;
                }
                let source_destinations: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM import_document_evidence
                      WHERE import_id = ?1 AND source_provenance = 'source'",
                    [id.as_str()],
                    |row| row.get(0),
                )?;
                Ok(ConcertableImportOutcome {
                    import_id,
                    preview_hash: preview_hash.to_owned(),
                    planning_commit,
                    epics: counts[0],
                    features: counts[1],
                    work_items: counts[2],
                    source_destinations: usize::try_from(source_destinations)
                        .map_err(|_| AppError::Domain("invalid import count".to_owned()))?,
                    already_applied: true,
                })
            },
            )
            .transpose()
        })
    }

    fn preflight_import_collisions(
        &self,
        workspace_id: WorkspaceId,
        planning_repository_id: RepositoryId,
        preview: &ConcertableImportPreview,
        prepared: &[PreparedDocument],
    ) -> Result<(), AppError> {
        self.store.read(|connection| {
            for epic in preview.epics.iter().filter(|epic| epic.selected) {
                let collision: i64 = connection.query_row(
                    "SELECT EXISTS (SELECT 1 FROM epics WHERE id = ?1 OR (workspace_id = ?2 AND slug = ?3))",
                    params![epic.id.to_string(), workspace_id.to_string(), epic.slug.as_str()],
                    |row| row.get(0),
                )?;
                if collision != 0 {
                    return Err(AppError::IdempotencyConflict);
                }
                for feature in epic.features.iter().filter(|feature| feature.selected) {
                    let collision: i64 = connection.query_row(
                        "SELECT EXISTS (SELECT 1 FROM features WHERE id = ?1 OR (epic_id = ?2 AND slug = ?3))",
                        params![feature.id.to_string(), epic.id.to_string(), feature.slug.as_str()],
                        |row| row.get(0),
                    )?;
                    if collision != 0 {
                        return Err(AppError::IdempotencyConflict);
                    }
                    for item in feature.work_items.iter().filter(|item| item.selected) {
                        let key = format!("{}/{}/{}", epic.slug, feature.slug, item.slug);
                        let collision: i64 = connection.query_row(
                            "SELECT EXISTS (SELECT 1 FROM work_items WHERE id = ?1 OR key = ?2 OR (feature_id = ?3 AND slug = ?4))",
                            params![item.id.to_string(), key, feature.id.to_string(), item.slug.as_str()],
                            |row| row.get(0),
                        )?;
                        if collision != 0 {
                            return Err(AppError::IdempotencyConflict);
                        }
                    }
                }
            }
            for document in prepared {
                let collision: i64 = connection.query_row(
                    "SELECT EXISTS (
                         SELECT 1 FROM documents
                         WHERE id = ?1 OR (repository_id = ?2 AND relative_path = ?3)
                     )",
                    params![
                        document.document.front_matter.id.to_string(),
                        planning_repository_id.to_string(),
                        path_text(&document.document.relative_path)?,
                    ],
                    |row| row.get(0),
                )?;
                if collision != 0 {
                    return Err(AppError::IdempotencyConflict);
                }
            }
            Ok(())
        })
    }
}

fn prepare_documents(
    workspace: &Slug,
    repository: &Slug,
    preview: &ConcertableImportPreview,
) -> Result<Vec<PreparedDocument>, AppError> {
    let mut result = Vec::new();
    for epic in preview.epics.iter().filter(|epic| epic.selected) {
        result.push(PreparedDocument {
            owner: ImportedOwner::Epic(epic.id),
            source: epic.source.clone(),
            document: NewPlanningDocument {
                relative_path: PlanningStore::epic_path(workspace, &epic.slug),
                front_matter: DocumentFrontMatter {
                    id: epic.document_id,
                    kind: DocumentKind::Epic,
                    key: epic.slug.to_string(),
                    status: None,
                    repositories: vec![repository.clone()],
                },
                body: epic.body.clone(),
            },
        });
        for feature in epic.features.iter().filter(|feature| feature.selected) {
            result.push(PreparedDocument {
                owner: ImportedOwner::Feature(feature.id),
                source: Some(feature.source.clone()),
                document: NewPlanningDocument {
                    relative_path: PlanningStore::feature_path(
                        workspace,
                        &epic.slug,
                        &feature.slug,
                    ),
                    front_matter: DocumentFrontMatter {
                        id: feature.document_id,
                        kind: DocumentKind::Feature,
                        key: format!("{}/{}", epic.slug, feature.slug),
                        status: None,
                        repositories: vec![repository.clone()],
                    },
                    body: feature.body.clone(),
                },
            });
            for item in feature.work_items.iter().filter(|item| item.selected) {
                result.push(PreparedDocument {
                    owner: ImportedOwner::WorkItem(item.id),
                    source: Some(item.source.clone()),
                    document: NewPlanningDocument {
                        relative_path: PlanningStore::work_item_path(
                            workspace,
                            &epic.slug,
                            &feature.slug,
                            &item.slug,
                        ),
                        front_matter: DocumentFrontMatter {
                            id: item.document_id,
                            kind: DocumentKind::WorkItem,
                            key: format!("{}/{}/{}", epic.slug, feature.slug, item.slug),
                            status: Some(item.status),
                            repositories: vec![repository.clone()],
                        },
                        body: item.body.clone(),
                    },
                });
            }
        }
    }
    Ok(result)
}

fn validate_preview(preview: &ConcertableImportPreview) -> Result<(), AppError> {
    if preview.format_version != CONCERTABLE_IMPORT_FORMAT_VERSION {
        return Err(AppError::Domain(format!(
            "unsupported Concertable import preview version: {}",
            preview.format_version
        )));
    }
    if !preview.source_repository.is_absolute() || preview.source_head.trim().is_empty() {
        return Err(AppError::Domain(
            "Concertable import source is invalid".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for epic in &preview.epics {
        if !epic.selected {
            if epic.features.iter().any(|feature| feature.selected) {
                return Err(AppError::Domain(
                    "a selected Feature cannot belong to an unselected Epic".to_owned(),
                ));
            }
            continue;
        }
        if epic.source.is_none() && !epic.features.iter().any(|feature| feature.selected) {
            return Err(AppError::Domain(
                "a generated Epic must contain a selected Feature".to_owned(),
            ));
        }
        validate_import_text(&epic.title, &epic.body)?;
        ensure_unique(&mut ids, epic.id.to_string(), "destination ID")?;
        ensure_unique(&mut ids, epic.document_id.to_string(), "document ID")?;
        ensure_unique(&mut paths, epic.slug.to_string(), "Epic slug")?;
        let mut feature_slugs = HashSet::new();
        for feature in &epic.features {
            if !feature.selected {
                if feature.work_items.iter().any(|item| item.selected) {
                    return Err(AppError::Domain(
                        "a selected Work item cannot belong to an unselected Feature".to_owned(),
                    ));
                }
                continue;
            }
            validate_import_text(&feature.title, &feature.body)?;
            ensure_unique(&mut ids, feature.id.to_string(), "destination ID")?;
            ensure_unique(&mut ids, feature.document_id.to_string(), "document ID")?;
            ensure_unique(&mut feature_slugs, feature.slug.to_string(), "Feature slug")?;
            let mut item_slugs = HashSet::new();
            for item in item_selected(feature) {
                validate_import_text(&item.title, &item.body)?;
                ensure_unique(&mut ids, item.id.to_string(), "destination ID")?;
                ensure_unique(&mut ids, item.document_id.to_string(), "document ID")?;
                ensure_unique(&mut item_slugs, item.slug.to_string(), "Work-item slug")?;
                WorkItemKey::new(format!("{}/{}/{}", epic.slug, feature.slug, item.slug))
                    .map_err(|error| AppError::Domain(error.to_string()))?;
            }
        }
    }
    if selected_counts(preview).1 == 0 {
        return Err(AppError::Domain(
            "the preview selects no Feature plans".to_owned(),
        ));
    }
    Ok(())
}

fn item_selected(
    feature: &ConcertableFeatureImport,
) -> impl Iterator<Item = &ConcertableWorkItemImport> {
    feature.work_items.iter().filter(|item| item.selected)
}

fn validate_import_text(title: &str, body: &str) -> Result<(), AppError> {
    if title.trim().is_empty() || body.trim().is_empty() {
        return Err(AppError::Domain(
            "selected imported titles and document bodies cannot be blank".to_owned(),
        ));
    }
    Ok(())
}

fn ensure_unique(values: &mut HashSet<String>, value: String, label: &str) -> Result<(), AppError> {
    if values.insert(value.clone()) {
        Ok(())
    } else {
        Err(AppError::Domain(format!("duplicate {label}: {value}")))
    }
}

fn validate_source(preview: &ConcertableImportPreview) -> Result<(), AppError> {
    let root = git_root(&preview.source_repository)?;
    if !paths_equal(&root, &preview.source_repository) {
        return Err(AppError::Domain(
            "Concertable import source repository moved after preview".to_owned(),
        ));
    }
    let head = git_text(&root, ["rev-parse", "--verify", "HEAD"])?;
    if head != preview.source_head {
        return Err(AppError::WorkflowDocumentChanged);
    }
    let mut checked = HashMap::new();
    for source in preview_sources(preview) {
        let source_path = concertable_source_path(&root, &source.relative_path)?;
        let existing = checked
            .entry(source.relative_path.clone())
            .or_insert_with(|| fs::read(&source_path).map(|bytes| hash_bytes(&bytes)));
        let actual = existing
            .as_ref()
            .map_err(|source_error| AppError::PlanningStoreIo {
                operation: "revalidating a Concertable source document",
                path: source_path,
                source: std::io::Error::new(source_error.kind(), source_error.to_string()),
            })?;
        if actual != &source.content_hash {
            return Err(AppError::WorkflowDocumentChanged);
        }
    }
    Ok(())
}

fn concertable_source_path(root: &Path, relative_path: &Path) -> Result<PathBuf, AppError> {
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::Domain(format!(
            "unsafe Concertable source path: {}",
            relative_path.display()
        )));
    }
    let path = root.join(relative_path);
    let metadata = fs::symlink_metadata(&path).map_err(|source| AppError::PlanningStoreIo {
        operation: "inspecting a Concertable source document",
        path: path.clone(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::Domain(format!(
            "linked Concertable planning paths are unsupported: {}",
            path.display()
        )));
    }
    let canonical = safe_concertable_path(root, &path)?;
    if !paths_equal(&path, &canonical) {
        return Err(AppError::Domain(format!(
            "linked Concertable planning paths are unsupported: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

fn preview_sources(preview: &ConcertableImportPreview) -> Vec<&SourceReference> {
    let mut result = Vec::new();
    for epic in preview.epics.iter().filter(|epic| epic.selected) {
        if let Some(source) = &epic.source {
            result.push(source);
        }
        for feature in epic.features.iter().filter(|feature| feature.selected) {
            result.push(&feature.source);
            result.extend(item_selected(feature).map(|item| &item.source));
        }
    }
    result
}

fn phase_work_items(source: &SourceFile) -> Result<Vec<ConcertableWorkItemImport>, AppError> {
    let lines = source.body.lines().collect::<Vec<_>>();
    let headings = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| parse_heading(line).map(|(level, title)| (index, level, title)))
        .collect::<Vec<_>>();
    let phase_section = headings
        .iter()
        .find(|(_, _, title)| is_phase_section(title));
    let section_end = phase_section.map(|(section_line, section_level, _)| {
        headings
            .iter()
            .find(|(other_line, other_level, _)| {
                other_line > section_line && other_level <= section_level
            })
            .map(|(line, _, _)| *line)
            .unwrap_or(lines.len())
    });
    let mut selected = Vec::new();
    for (heading_index, (line_index, level, title)) in headings.iter().enumerate() {
        let is_numbered_section_child = phase_section.is_some_and(|(_, section_level, _)| {
            level == &(section_level + 1) && starts_with_number(title)
        });
        if !is_phase_title(title) && !is_numbered_section_child {
            continue;
        }
        if let Some((section_line, section_level, _)) = phase_section
            && (line_index <= section_line
                || line_index >= &section_end.unwrap_or(lines.len())
                || level <= section_level)
        {
            continue;
        }
        let end = headings
            .iter()
            .skip(heading_index + 1)
            .find(|(_, next_level, _)| next_level <= level)
            .map(|(line, _, _)| *line)
            .unwrap_or(lines.len());
        selected.push((*line_index, end, clean_heading_title(title)));
    }
    if selected.is_empty() && phase_section.is_some() {
        selected = list_phase_items(
            &lines,
            phase_section.map_or(0, |(line, _, _)| line + 1),
            section_end.unwrap_or(lines.len()),
        );
    }
    let mut slugs = HashSet::new();
    selected
        .into_iter()
        .map(|(start, end, title)| {
            let body = format!("{}\n", lines[start..end].join("\n"));
            let status = if lines[start].contains("[x]") || lines[start].contains("[X]") {
                WorkItemStatus::Done
            } else {
                explicit_completed_status(&title)
            };
            Ok(ConcertableWorkItemImport {
                selected: true,
                id: WorkItemId::generate(),
                document_id: DocumentId::generate(),
                slug: unique_slug(derive_slug(&title, None), &mut slugs)?,
                title,
                status,
                body,
                source: source_reference(
                    source,
                    Some(u32::try_from(start + 1).map_err(|_| {
                        AppError::Domain("source line exceeds supported range".to_owned())
                    })?),
                    Some(u32::try_from(end).map_err(|_| {
                        AppError::Domain("source line exceeds supported range".to_owned())
                    })?),
                ),
            })
        })
        .collect()
}

fn list_phase_items(lines: &[&str], start: usize, end: usize) -> Vec<(usize, usize, String)> {
    let candidates = lines[start..end]
        .iter()
        .enumerate()
        .filter_map(|(offset, line)| {
            let line_index = start + offset;
            list_item_title(line).map(|title| (line_index, title))
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, (line_index, title))| {
            let next = candidates
                .get(index + 1)
                .map_or(end, |(next_line, _)| *next_line);
            (is_phase_title(title) || starts_with_number(title))
                .then(|| (*line_index, next, clean_list_item_title(lines[*line_index])))
        })
        .collect()
}

fn list_item_title(line: &str) -> Option<String> {
    if line.len() != line.trim_start().len() {
        return None;
    }
    let mut value = line.trim();
    if let Some(rest) = value
        .strip_prefix("- ")
        .or_else(|| value.strip_prefix("* "))
    {
        value = rest;
        if value.starts_with('[') && value.as_bytes().get(2) == Some(&b']') {
            value = value.get(3..)?.trim_start();
        }
    } else {
        let (number, rest) = value.split_once(". ")?;
        if !number.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        let _ = rest;
        return Some(value.to_owned());
    }
    Some(value.to_owned())
}

fn clean_list_item_title(line: &str) -> String {
    let value = list_item_title(line).unwrap_or_else(|| line.trim().to_owned());
    let value = value.trim_start_matches("**");
    let title = value.split_once("**").map_or(value, |(title, _)| title);
    clean_heading_title(title).trim_end_matches('.').to_owned()
}

fn is_phase_section(title: &str) -> bool {
    let title = clean_heading_title(title).to_ascii_lowercase();
    title == "phases"
        || title.ends_with(" phases")
        || title == "checkpoints"
        || title.contains("phased migration plan")
        || title.contains("independently mergeable checkpoints")
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=4).contains(&level) || trimmed.as_bytes().get(level) != Some(&b' ') {
        return None;
    }
    Some((level, trimmed[level + 1..].trim().to_owned()))
}

fn is_phase_title(title: &str) -> bool {
    let normalised = title
        .trim_matches('~')
        .trim_start_matches(|character: char| !character.is_ascii_alphanumeric())
        .to_ascii_lowercase();
    let words = normalised.split_whitespace().collect::<Vec<_>>();
    words.windows(2).enumerate().any(|(index, window)| {
        let marker = window[0].trim_matches(|c: char| !c.is_ascii_alphabetic());
        matches!(marker, "phase" | "stage" | "checkpoint")
            && words[..index].iter().all(|word| {
                word.chars()
                    .all(|character| character.is_ascii_digit() || ".()-".contains(character))
            })
            && window[1]
                .trim_matches(|character: char| !character.is_ascii_alphanumeric())
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
    })
}

fn starts_with_number(title: &str) -> bool {
    title
        .trim_start()
        .split_once('.')
        .is_some_and(|(number, _)| {
            !number.is_empty() && number.chars().all(|character| character.is_ascii_digit())
        })
}

fn clean_heading_title(title: &str) -> String {
    title.replace('`', "").replace("~~", "").trim().to_owned()
}

fn explicit_completed_status(title: &str) -> WorkItemStatus {
    let terminal = title
        .split_whitespace()
        .next_back()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_ascii_alphabetic())
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if title.contains('✅')
        || matches!(
            terminal.as_str(),
            "done" | "complete" | "completed" | "shipped" | "merged"
        )
    {
        WorkItemStatus::Done
    } else {
        WorkItemStatus::Ready
    }
}

fn selected_counts(preview: &ConcertableImportPreview) -> (usize, usize, usize) {
    let epics = preview.epics.iter().filter(|epic| epic.selected).count();
    let features = preview
        .epics
        .iter()
        .filter(|epic| epic.selected)
        .flat_map(|epic| epic.features.iter())
        .filter(|feature| feature.selected)
        .count();
    let work_items = preview
        .epics
        .iter()
        .filter(|epic| epic.selected)
        .flat_map(|epic| epic.features.iter().filter(|feature| feature.selected))
        .flat_map(item_selected)
        .count();
    (epics, features, work_items)
}

fn collect_markdown(
    root: &Path,
    repository: &Path,
    visited_directories: &mut HashSet<PathBuf>,
    result: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    let canonical_root = safe_concertable_path(repository, root)?;
    if !visited_directories.insert(canonical_root.clone()) {
        return Err(AppError::Domain(format!(
            "Concertable planning directory was visited more than once: {}",
            canonical_root.display()
        )));
    }
    for entry in fs::read_dir(&canonical_root).map_err(|source| AppError::PlanningStoreIo {
        operation: "reading Concertable planning files",
        path: canonical_root.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| AppError::PlanningStoreIo {
            operation: "reading a Concertable planning entry",
            path: canonical_root.clone(),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| AppError::PlanningStoreIo {
            operation: "inspecting a Concertable planning entry",
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::Domain(format!(
                "linked Concertable planning paths are unsupported: {}",
                path.display()
            )));
        }
        let canonical_path = safe_concertable_path(repository, &path)?;
        if !paths_equal(&path, &canonical_path) {
            return Err(AppError::Domain(format!(
                "linked Concertable planning paths are unsupported: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_markdown(&canonical_path, repository, visited_directories, result)?;
        } else if metadata.is_file()
            && canonical_path
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("md")
        {
            let name = canonical_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name.ends_with("_ROADMAP.md")
                || name.ends_with("_PLAN.md")
                || name.ends_with("_PROGRESS.md")
            {
                result.push(
                    canonical_path
                        .strip_prefix(repository)
                        .map_err(|_| {
                            AppError::Domain(
                                "Concertable planning path escaped its repository".to_owned(),
                            )
                        })?
                        .to_path_buf(),
                );
            }
        }
    }
    Ok(())
}

fn safe_concertable_path(repository: &Path, path: &Path) -> Result<PathBuf, AppError> {
    let canonical = path
        .canonicalize()
        .map_err(|source| AppError::PlanningStoreIo {
            operation: "resolving a Concertable planning path",
            path: path.to_path_buf(),
            source,
        })?;
    if canonical.strip_prefix(repository).is_err() {
        return Err(AppError::Domain(format!(
            "Concertable planning path escaped its repository: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

fn read_source(root: &Path, relative_path: PathBuf) -> Result<SourceFile, AppError> {
    let path = root.join(&relative_path);
    let bytes = fs::read(&path).map_err(|source| AppError::PlanningStoreIo {
        operation: "reading a Concertable planning document",
        path: path.clone(),
        source,
    })?;
    let body = String::from_utf8(bytes.clone()).map_err(|_| {
        AppError::PlanningDocumentInvalid(format!("{} is not UTF-8", path.display()))
    })?;
    let fallback = relative_path.file_stem().and_then(|stem| stem.to_str());
    let title = body
        .lines()
        .find_map(|line| {
            parse_heading(line)
                .filter(|(level, _)| *level == 1)
                .map(|(_, title)| clean_heading_title(&title))
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| title_case(fallback.unwrap_or("Imported plan")));
    Ok(SourceFile {
        relative_path,
        content_hash: hash_bytes(&bytes),
        title,
        body,
    })
}

fn source_reference(
    source: &SourceFile,
    first_line: Option<u32>,
    last_line: Option<u32>,
) -> SourceReference {
    SourceReference {
        relative_path: source.relative_path.clone(),
        content_hash: source.content_hash.clone(),
        first_line,
        last_line,
    }
}

fn derive_slug(title: &str, fallback: Option<&std::ffi::OsStr>) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
        if result.len() >= 92 {
            break;
        }
    }
    if result.is_empty() {
        fallback
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase().replace('_', "-"))
            .unwrap_or_else(|| "imported".to_owned())
    } else {
        result.trim_matches('-').to_owned()
    }
}

fn unique_slug(candidate: String, used: &mut HashSet<String>) -> Result<Slug, AppError> {
    for suffix in 1..=10_000_u32 {
        let value = if suffix == 1 {
            candidate.clone()
        } else {
            let prefix_length = 96_usize.saturating_sub(suffix.to_string().len());
            format!(
                "{}-{suffix}",
                &candidate[..candidate.len().min(prefix_length)]
            )
        };
        if used.insert(value.clone()) {
            return Slug::new(value).map_err(|error| AppError::Domain(error.to_string()));
        }
    }
    Err(AppError::Domain(
        "could not derive a unique slug".to_owned(),
    ))
}

fn title_case(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    characters.as_str().to_ascii_lowercase()
                )
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn document_stem(name: &str, suffix: &str) -> String {
    name.strip_suffix(suffix).unwrap_or(name).to_owned()
}

fn document_pair_key(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    path.parent()
        .unwrap_or(Path::new(""))
        .join(document_stem(name, suffix))
}

fn status_name(status: WorkItemStatus) -> &'static str {
    match status {
        WorkItemStatus::Backlog => "backlog",
        WorkItemStatus::Ready => "ready",
        WorkItemStatus::InProgress => "in_progress",
        WorkItemStatus::Blocked => "blocked",
        WorkItemStatus::Review => "review",
        WorkItemStatus::Done => "done",
        WorkItemStatus::Cancelled => "cancelled",
    }
}

fn git_root(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::Domain(format!(
            "Concertable repository is unavailable: {}",
            path.display()
        )));
    }
    let root = PathBuf::from(git_text(
        path,
        ["rev-parse", "--path-format=absolute", "--show-toplevel"],
    )?);
    root.canonicalize()
        .map_err(|source| AppError::PlanningStoreIo {
            operation: "resolving the Concertable repository",
            path: root,
            source,
        })
}

fn git_text<const N: usize>(root: &Path, arguments: [&str; N]) -> Result<String, AppError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(AppError::GitIo)?;
    if !output.status.success() {
        return Err(AppError::GitCommand {
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| AppError::GitOutputEncoding)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use rusqlite::{Connection, params};
    use tempfile::TempDir;
    use workboard_core::{DocumentId, EpicId, FeatureId, ImportBatchId, Slug};

    use super::{
        CONCERTABLE_IMPORT_FORMAT_VERSION, ConcertableEpicImport, concertable_source_path,
        explicit_completed_status, preview_concertable_plans,
    };
    use crate::workspace::{
        CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
    };

    #[test]
    fn preview_preserves_roadmaps_plans_phases_and_progress() {
        let fixture = Fixture::new();
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");

        assert_eq!(preview.format_version, CONCERTABLE_IMPORT_FORMAT_VERSION);
        assert_eq!(preview.epics.len(), 1);
        assert_eq!(preview.epics[0].features.len(), 1);
        let feature = &preview.epics[0].features[0];
        assert_eq!(feature.work_items.len(), 3);
        assert_eq!(
            feature.work_items[0].status,
            workboard_core::WorkItemStatus::Done
        );
        assert!(feature.work_items[2].body.contains("Current state"));
        assert!(feature.body.contains("Phase 2"));
    }

    #[test]
    fn completion_status_requires_an_explicit_terminal_marker() {
        assert_eq!(
            explicit_completed_status("Phase 2 — Complete migration"),
            workboard_core::WorkItemStatus::Ready
        );
        assert_eq!(
            explicit_completed_status("Phase 2 — Migration completion"),
            workboard_core::WorkItemStatus::Ready
        );
        assert_eq!(
            explicit_completed_status("Phase 2 — Migration complete"),
            workboard_core::WorkItemStatus::Done
        );
        assert_eq!(
            explicit_completed_status("Phase 2 — Migration [SHIPPED]"),
            workboard_core::WorkItemStatus::Done
        );
        assert_eq!(
            explicit_completed_status("Phase 2 — Migration ✅"),
            workboard_core::WorkItemStatus::Done
        );
    }

    #[test]
    fn preview_pairs_same_named_progress_with_its_own_directory() {
        let directory = TempDir::new().expect("temporary directory");
        let source = directory.path().join("Concertable");
        for (folder, title) in [("first", "First"), ("second", "Second")] {
            let plans = source.join("plans").join(folder);
            fs::create_dir_all(&plans).expect("create plans");
            fs::write(
                plans.join("SHARED_PLAN.md"),
                format!("# {title}\n\n## Phases\n\n### Phase 1 — Build\n\nBuild.\n"),
            )
            .expect("write plan");
            fs::write(
                plans.join("SHARED_PROGRESS.md"),
                format!("# {title} progress\n\n{title} progress body.\n"),
            )
            .expect("write progress");
        }
        initialise_repository(&source);

        let preview = preview_concertable_plans(&source).expect("preview plans");

        for title in ["First", "Second"] {
            let feature = preview
                .epics
                .iter()
                .flat_map(|epic| &epic.features)
                .find(|feature| feature.title == title)
                .expect("find Feature");
            let progress = feature
                .work_items
                .iter()
                .find(|item| item.title.ends_with("progress and handoff"))
                .expect("find progress Work item");
            assert!(progress.body.contains(&format!("{title} progress body.")));
        }
    }

    #[test]
    fn preview_rejects_unmatched_progress() {
        let directory = TempDir::new().expect("temporary directory");
        let source = directory.path().join("Concertable");
        let plans = source.join("plans/launch");
        fs::create_dir_all(&plans).expect("create plans");
        fs::write(
            plans.join("AVAILABILITY_PLAN.md"),
            "# Availability\n\n## Phases\n\n### Phase 1 — API\n\nBuild API.\n",
        )
        .expect("write plan");
        fs::write(
            plans.join("ORPHAN_PROGRESS.md"),
            "# Orphan progress\n\nDurable state.\n",
        )
        .expect("write progress");
        initialise_repository(&source);

        let error = preview_concertable_plans(&source).expect_err("reject unmatched progress");

        assert!(error.to_string().contains("ORPHAN_PROGRESS.md"));
    }

    #[test]
    fn source_reference_rejects_parent_traversal() {
        let fixture = Fixture::new();

        let error = concertable_source_path(&fixture.source, Path::new("../outside.md"))
            .expect_err("reject traversal");

        assert!(error.to_string().contains("unsafe Concertable source path"));
    }

    #[test]
    fn preview_rejects_linked_planning_directories() {
        let fixture = Fixture::new();
        let external = fixture.directory.path().join("external");
        fs::create_dir_all(&external).expect("create external directory");
        fs::write(
            external.join("EXTERNAL_PLAN.md"),
            "# External\n\n## Phases\n\n### Phase 1 — Escape\n\nEscape.\n",
        )
        .expect("write external plan");
        let linked = fixture.source.join("plans/linked");
        if !create_directory_link(&external, &linked) {
            return;
        }

        let error = preview_concertable_plans(&fixture.source).expect_err("reject linked plans");

        assert!(error.to_string().contains("planning path"));
    }

    #[test]
    fn apply_publishes_one_commit_and_is_idempotent() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let mut preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let imported_features = std::mem::take(&mut preview.epics[0].features);
        preview.epics.push(ConcertableEpicImport {
            selected: true,
            id: EpicId::generate(),
            document_id: DocumentId::generate(),
            slug: Slug::new("synthetic-epic").expect("synthetic epic slug"),
            title: "Synthetic epic".to_owned(),
            body: "# Synthetic epic\n".to_owned(),
            source: None,
            features: imported_features,
        });
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let unrelated_epic_id = EpicId::generate();
        let unrelated_document_id = DocumentId::generate();
        let unrelated_hash = "f".repeat(64);
        drop(application);
        let connection = Connection::open(&database).expect("open schema 24 database");
        restore_schema_22_import_database(&connection);
        connection
            .execute(
                "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                 VALUES (?1, ?2, 'unrelated', 'Unrelated', 'later')",
                params![
                    unrelated_epic_id.to_string(),
                    workspace.workspace.id.to_string(),
                ],
            )
            .expect("record unrelated epic");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, epic_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'epic', 'unrelated.md', ?4, ?5, 'later')",
                params![
                    unrelated_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    unrelated_epic_id.to_string(),
                    unrelated_hash,
                    first.planning_commit,
                ],
            )
            .expect("record unrelated document");
        connection
            .execute(
                "INSERT INTO document_revisions (
                     document_id, revision, content_hash, observed_commit, observed_at
                 ) VALUES (?1, 1, ?2, ?3, 'later')",
                params![
                    unrelated_document_id.to_string(),
                    unrelated_hash,
                    first.planning_commit,
                ],
            )
            .expect("record unrelated same-commit revision");
        drop(connection);
        let evidence_upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("legacy synthetic Epic must require explicit evidence"),
            Err(error) => error,
        };
        assert!(
            evidence_upgrade_error
                .to_string()
                .contains("schema migration 28")
        );
        let connection = Connection::open(&database).expect("open schema 27 database");
        connection
            .execute(
                "INSERT INTO import_document_synthetic_attestations (
                     import_id, document_id, authority, attested_at
                 ) VALUES (?1, ?2, 'explicit_repair', 'legacy-repair')",
                params![
                    first.import_id.to_string(),
                    preview.epics[1].document_id.to_string(),
                ],
            )
            .expect("attest legacy synthetic Epic");
        drop(connection);
        let mut application =
            WorkboardApplication::open(&database).expect("upgrade import database");
        let (membership_count, unrelated_membership): (i64, i64) = application
            .store
            .read(|connection| {
                let membership_count = connection.query_row(
                    "SELECT COUNT(*) FROM import_document_memberships WHERE import_id = ?1",
                    [first.import_id.to_string()],
                    |row| row.get(0),
                )?;
                let unrelated_membership = connection.query_row(
                    "SELECT COUNT(*) FROM import_document_memberships WHERE document_id = ?1",
                    [unrelated_document_id.to_string()],
                    |row| row.get(0),
                )?;
                Ok((membership_count, unrelated_membership))
            })
            .expect("read upgraded import membership");
        assert_eq!(membership_count, 6);
        assert_eq!(unrelated_membership, 0);
        let synthetic_document_id = preview.epics[1].document_id;
        let recursive_triggers: i64 = application
            .store
            .read(|connection| {
                connection
                    .pragma_query_value(None, "recursive_triggers", |row| row.get(0))
                    .map_err(Into::into)
            })
            .expect("read recursive trigger setting");
        assert_eq!(recursive_triggers, 0);
        let (evidence_before, attestation_before) = application
            .store
            .read(|connection| {
                let evidence = connection.query_row(
                    "SELECT revision, revision_content_hash, source_provenance,
                            source_path, source_hash
                       FROM import_document_evidence
                      WHERE import_id = ?1 AND document_id = ?2",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )?;
                let attestation = connection.query_row(
                    "SELECT authority, attested_at
                       FROM import_document_synthetic_attestations
                      WHERE import_id = ?1 AND document_id = ?2",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?;
                Ok((evidence, attestation))
            })
            .expect("read immutable import evidence");
        for (statement, expected) in [
            (
                "UPDATE import_document_evidence SET revision_content_hash = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
                  WHERE import_id = ?1 AND document_id = ?2",
                "import document evidence is immutable",
            ),
            (
                "DELETE FROM import_document_evidence
                  WHERE import_id = ?1 AND document_id = ?2",
                "import document evidence cannot be deleted",
            ),
            (
                "UPDATE import_document_synthetic_attestations SET attested_at = 'mutated'
                  WHERE import_id = ?1 AND document_id = ?2",
                "synthetic import document attestation is immutable",
            ),
            (
                "DELETE FROM import_document_synthetic_attestations
                  WHERE import_id = ?1 AND document_id = ?2",
                "synthetic import document attestation cannot be deleted",
            ),
        ] {
            let error = application
                .store
                .write(|transaction| {
                    transaction.execute(
                        statement,
                        params![
                            first.import_id.to_string(),
                            synthetic_document_id.to_string(),
                        ],
                    )?;
                    Ok(())
                })
                .expect_err("reject immutable evidence mutation");
            assert!(error.to_string().contains(expected), "{error}");
        }
        let replacement_hash = "a".repeat(64);
        let evidence_replace_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE document_revisions SET content_hash = ?2
                      WHERE document_id = ?1 AND revision = ?3",
                    params![
                        synthetic_document_id.to_string(),
                        replacement_hash,
                        evidence_before.0,
                    ],
                )?;
                transaction.execute(
                    "INSERT OR REPLACE INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, ?3, ?4, 'synthetic', NULL, NULL)",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                        evidence_before.0,
                        replacement_hash,
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject coordinated revision and evidence replacement");
        assert!(
            evidence_replace_error
                .to_string()
                .contains("import document evidence is immutable")
        );
        let attestation_replace_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT OR REPLACE INTO import_document_synthetic_attestations (
                         import_id, document_id, authority, attested_at
                     ) VALUES (?1, ?2, 'explicit_repair', 'replacement')",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject synthetic attestation replacement");
        assert!(
            attestation_replace_error
                .to_string()
                .contains("synthetic import document attestation is immutable")
        );
        let (evidence_after, attestation_after) = application
            .store
            .read(|connection| {
                let evidence = connection.query_row(
                    "SELECT revision, revision_content_hash, source_provenance,
                            source_path, source_hash
                       FROM import_document_evidence
                      WHERE import_id = ?1 AND document_id = ?2",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )?;
                let attestation = connection.query_row(
                    "SELECT authority, attested_at
                       FROM import_document_synthetic_attestations
                      WHERE import_id = ?1 AND document_id = ?2",
                    params![
                        first.import_id.to_string(),
                        synthetic_document_id.to_string(),
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?;
                Ok((evidence, attestation))
            })
            .expect("reread immutable import evidence");
        assert_eq!(evidence_after, evidence_before);
        assert_eq!(attestation_after, attestation_before);
        let revision_hash_after: String = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT content_hash FROM document_revisions
                          WHERE document_id = ?1 AND revision = ?2",
                        params![synthetic_document_id.to_string(), evidence_before.0],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("read revision after rejected replacement");
        assert_eq!(revision_hash_after, evidence_before.1);
        let partial_feature_id = FeatureId::generate();
        let partial_document_id = DocumentId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO features (
                         id, epic_id, slug, title, workflow_state, created_at
                     ) VALUES (?1, ?2, 'partial-feature', 'Partial feature', 'planned', 'later')",
                    params![
                        partial_feature_id.to_string(),
                        unrelated_epic_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, feature_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'feature', 'partial-feature.md', ?4, ?5, 'later')",
                    params![
                        partial_document_id.to_string(),
                        workspace.workspace.planning_store_repository_id.to_string(),
                        partial_feature_id.to_string(),
                        "b".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, 'later')",
                    params![
                        partial_document_id.to_string(),
                        "b".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                Ok(())
            })
            .expect("record partial-import target");
        let contradictory_import_id = ImportBatchId::generate();
        let contradictory_epic_id = EpicId::generate();
        let contradictory_feature_id = FeatureId::generate();
        let contradictory_epic_document_id = DocumentId::generate();
        let contradictory_feature_document_id = DocumentId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'contradictory', 'Contradictory', 'contradictory')",
                    params![
                        contradictory_epic_id.to_string(),
                        workspace.workspace.id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO features (
                         id, epic_id, slug, title, workflow_state, created_at
                     ) VALUES (?1, ?2, 'contradictory-feature', 'Contradictory feature',
                               'planned', 'contradictory')",
                    params![
                        contradictory_feature_id.to_string(),
                        contradictory_epic_id.to_string(),
                    ],
                )?;
                for (document_id, owner_id, kind, path, hash) in [
                    (
                        contradictory_epic_document_id,
                        contradictory_epic_id.to_string(),
                        "epic",
                        "contradictory.md",
                        "f".repeat(64),
                    ),
                    (
                        contradictory_feature_document_id,
                        contradictory_feature_id.to_string(),
                        "feature",
                        "contradictory-feature.md",
                        "b".repeat(64),
                    ),
                ] {
                    let owner_column = if kind == "epic" {
                        "epic_id"
                    } else {
                        "feature_id"
                    };
                    transaction.execute(
                        &format!(
                            "INSERT INTO documents (
                                 id, repository_id, {owner_column}, kind, relative_path,
                                 content_hash, observed_commit, observed_at
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'contradictory')"
                        ),
                        params![
                            document_id.to_string(),
                            workspace.workspace.planning_store_repository_id.to_string(),
                            owner_id,
                            kind,
                            path,
                            hash,
                            first.planning_commit,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO document_revisions (
                             document_id, revision, content_hash, observed_commit, observed_at
                         ) VALUES (?1, 1, ?2, ?3, 'contradictory')",
                        params![document_id.to_string(), hash, first.planning_commit],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'contradictory', ?3, ?4, ?5,
                               'contradictory', ?6)",
                    params![
                        contradictory_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "7".repeat(64),
                        first.planning_commit,
                        repository.id.to_string(),
                    ],
                )?;
                for (document_id, kind) in [
                    (contradictory_epic_document_id, "epic"),
                    (contradictory_feature_document_id, "feature"),
                ] {
                    transaction.execute(
                        "INSERT INTO import_document_memberships (
                             import_id, document_id, destination_kind
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            contradictory_import_id.to_string(),
                            document_id.to_string(),
                            kind,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'contradictory-feature.md', ?2, 'feature', ?3, ?4)",
                    params![
                        contradictory_import_id.to_string(),
                        "b".repeat(64),
                        contradictory_feature_id.to_string(),
                        contradictory_feature_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_synthetic_attestations (
                         import_id, document_id, authority, attested_at
                     ) VALUES (?1, ?2, 'import_preview', 'contradictory')",
                    params![
                        contradictory_import_id.to_string(),
                        contradictory_epic_document_id.to_string(),
                    ],
                )?;
                let source_error = transaction
                    .execute(
                        "INSERT INTO import_source_destinations (
                             import_id, source_path, source_hash, destination_kind,
                             destination_id, document_id
                         ) VALUES (?1, 'contradictory-epic.md', ?2, 'epic', ?3, ?4)",
                        params![
                            contradictory_import_id.to_string(),
                            "f".repeat(64),
                            contradictory_epic_id.to_string(),
                            contradictory_epic_document_id.to_string(),
                        ],
                    )
                    .expect_err("reject source mapping after synthetic attestation");
                assert!(
                    source_error
                        .to_string()
                        .contains("cannot have source destinations")
                );
                let source_evidence_error = transaction
                    .execute(
                        "INSERT INTO import_document_evidence (
                             import_id, document_id, revision, revision_content_hash,
                             source_provenance, source_path, source_hash
                         ) VALUES (?1, ?2, 1, ?3, 'source', 'contradictory-epic.md', ?4)",
                        params![
                            contradictory_import_id.to_string(),
                            contradictory_epic_document_id.to_string(),
                            "f".repeat(64),
                            "f".repeat(64),
                        ],
                    )
                    .expect_err("reject source evidence after synthetic attestation");
                assert!(
                    source_evidence_error
                        .to_string()
                        .contains("cannot have source evidence")
                );
                for (document_id, revision_hash, provenance, path, source_hash) in [
                    (
                        contradictory_epic_document_id,
                        "f".repeat(64),
                        "synthetic",
                        None,
                        None,
                    ),
                    (
                        contradictory_feature_document_id,
                        "b".repeat(64),
                        "source",
                        Some("contradictory-feature.md"),
                        Some("b".repeat(64)),
                    ),
                ] {
                    transaction.execute(
                        "INSERT INTO import_document_evidence (
                             import_id, document_id, revision, revision_content_hash,
                             source_provenance, source_path, source_hash
                         ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
                        params![
                            contradictory_import_id.to_string(),
                            document_id.to_string(),
                            revision_hash,
                            provenance,
                            path,
                            source_hash,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, 'contradictory')",
                    [contradictory_import_id.to_string()],
                )?;
                Ok(())
            })
            .expect("finalize consistent synthetic evidence");
        let insert_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_document_memberships (
                         import_id, document_id, destination_kind
                     ) VALUES (?1, ?2, 'epic')",
                    params![
                        first.import_id.to_string(),
                        unrelated_document_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject membership after finalization");
        assert!(insert_error.to_string().contains("membership is finalized"));
        let imported_document_id = preview.epics[0].document_id;
        let imported_epic_id = preview.epics[0].id;
        let imported_feature_id = preview.epics[1].features[0].id;
        let imported_work_item_id = preview.epics[1].features[0].work_items[0].id;
        let source_count_before: i64 = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM import_source_destinations WHERE import_id = ?1",
                        [first.import_id.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("count source mappings before rejected insert");
        let source_insert_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'late-source.md', ?2, 'epic', ?3, ?4)",
                    params![
                        first.import_id.to_string(),
                        "a".repeat(64),
                        imported_epic_id.to_string(),
                        imported_document_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject source mapping after finalization");
        assert!(
            source_insert_error
                .to_string()
                .contains("source destinations are finalized")
        );
        let source_count_after: i64 = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM import_source_destinations WHERE import_id = ?1",
                        [first.import_id.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("count source mappings after rejected insert");
        assert_eq!(source_count_after, source_count_before);
        let update_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE documents SET repository_id = ?2 WHERE id = ?1",
                    params![imported_document_id.to_string(), repository.id.to_string(),],
                )?;
                Ok(())
            })
            .expect_err("reject imported document reassignment");
        assert!(
            update_error
                .to_string()
                .contains("membership fields are immutable")
        );
        let owner_update_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE documents SET epic_id = ?2 WHERE id = ?1",
                    params![
                        imported_document_id.to_string(),
                        unrelated_epic_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject imported document owner reassignment");
        assert!(
            owner_update_error
                .to_string()
                .contains("membership fields are immutable")
        );
        let hierarchy_update_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE features SET epic_id = ?2 WHERE id = ?1",
                    params![
                        imported_feature_id.to_string(),
                        unrelated_epic_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject imported Feature parent reassignment");
        assert!(
            hierarchy_update_error
                .to_string()
                .contains("imported Feature parent is immutable")
        );
        let planning_repository_update_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE workspaces SET planning_store_repository_id = ?2 WHERE id = ?1",
                    params![
                        workspace.workspace.id.to_string(),
                        repository.id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject imported workspace planning repository reassignment");
        assert!(
            planning_repository_update_error
                .to_string()
                .contains("import workspace planning repository is immutable")
        );
        let work_item_repository_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![
                        imported_work_item_id.to_string(),
                        workspace.workspace.planning_store_repository_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect_err("reject imported Work item repository association");
        assert!(
            work_item_repository_error
                .to_string()
                .contains("imported Work item repositories are finalized")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE features SET workflow_state = 'planning_active' WHERE id = ?1",
                    [imported_feature_id.to_string()],
                )?;
                transaction.execute(
                    "UPDATE work_items SET status = 'review' WHERE id = ?1",
                    [imported_work_item_id.to_string()],
                )?;
                Ok(())
            })
            .expect("update ordinary imported workflow state");
        let partial_import_id = ImportBatchId::generate();
        let partial_finalization_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'partial', ?3, ?4, ?5, ?6, ?7)",
                    params![
                        partial_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "d".repeat(64),
                        first.planning_commit,
                        "later",
                        repository.id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_memberships (
                         import_id, document_id, destination_kind
                    ) VALUES (?1, ?2, 'epic')",
                    params![
                        partial_import_id.to_string(),
                        unrelated_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'partial.md', ?2, 'epic', ?3, ?4)",
                    params![
                        partial_import_id.to_string(),
                        "c".repeat(64),
                        unrelated_epic_id.to_string(),
                        unrelated_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, 1, ?3, 'source', 'partial.md', ?4)",
                    params![
                        partial_import_id.to_string(),
                        unrelated_document_id.to_string(),
                        "f".repeat(64),
                        "c".repeat(64),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, ?2)",
                    params![partial_import_id.to_string(), "later"],
                )?;
                Ok(())
            })
            .expect_err("reject partial import finalization");
        assert!(
            partial_finalization_error
                .to_string()
                .contains("finalization is invalid"),
            "{partial_finalization_error}"
        );
        let missing_source_import_id = ImportBatchId::generate();
        let missing_source_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'missing-source', ?3, ?4, ?5,
                               'later', ?6)",
                    params![
                        missing_source_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "e".repeat(64),
                        first.planning_commit,
                        repository.id.to_string(),
                    ],
                )?;
                for (document_id, kind) in [
                    (unrelated_document_id, "epic"),
                    (partial_document_id, "feature"),
                ] {
                    transaction.execute(
                        "INSERT INTO import_document_memberships (
                             import_id, document_id, destination_kind
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            missing_source_import_id.to_string(),
                            document_id.to_string(),
                            kind,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'missing-source-feature.md', ?2, 'feature', ?3, ?4)",
                    params![
                        missing_source_import_id.to_string(),
                        "b".repeat(64),
                        partial_feature_id.to_string(),
                        partial_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, 1, ?3, 'source', 'missing-source-feature.md', ?4)",
                    params![
                        missing_source_import_id.to_string(),
                        partial_document_id.to_string(),
                        "b".repeat(64),
                        "b".repeat(64),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, 'later')",
                    [missing_source_import_id.to_string()],
                )?;
                Ok(())
            })
            .expect_err("reject source-backed Epic without source evidence");
        assert!(
            missing_source_error
                .to_string()
                .contains("finalization is invalid")
        );
        let duplicate_source_import_id = ImportBatchId::generate();
        let duplicate_source_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'duplicate-source', ?3, ?4, ?5,
                               'later', ?6)",
                    params![
                        duplicate_source_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "0".repeat(64),
                        first.planning_commit,
                        repository.id.to_string(),
                    ],
                )?;
                for (document_id, kind) in [
                    (unrelated_document_id, "epic"),
                    (partial_document_id, "feature"),
                ] {
                    transaction.execute(
                        "INSERT INTO import_document_memberships (
                             import_id, document_id, destination_kind
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            duplicate_source_import_id.to_string(),
                            document_id.to_string(),
                            kind,
                        ],
                    )?;
                }
                for (path, hash) in [
                    ("duplicate-source-one.md", "f".repeat(64)),
                    ("duplicate-source-two.md", "e".repeat(64)),
                ] {
                    transaction.execute(
                        "INSERT INTO import_source_destinations (
                             import_id, source_path, source_hash, destination_kind,
                             destination_id, document_id
                         ) VALUES (?1, ?2, ?3, 'epic', ?4, ?5)",
                        params![
                            duplicate_source_import_id.to_string(),
                            path,
                            hash,
                            unrelated_epic_id.to_string(),
                            unrelated_document_id.to_string(),
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'duplicate-source-feature.md', ?2, 'feature', ?3, ?4)",
                    params![
                        duplicate_source_import_id.to_string(),
                        "b".repeat(64),
                        partial_feature_id.to_string(),
                        partial_document_id.to_string(),
                    ],
                )?;
                for (document_id, revision_hash, path, source_hash) in [
                    (
                        unrelated_document_id,
                        "f".repeat(64),
                        "duplicate-source-one.md",
                        "f".repeat(64),
                    ),
                    (
                        partial_document_id,
                        "b".repeat(64),
                        "duplicate-source-feature.md",
                        "b".repeat(64),
                    ),
                ] {
                    transaction.execute(
                        "INSERT INTO import_document_evidence (
                             import_id, document_id, revision, revision_content_hash,
                             source_provenance, source_path, source_hash
                         ) VALUES (?1, ?2, 1, ?3, 'source', ?4, ?5)",
                        params![
                            duplicate_source_import_id.to_string(),
                            document_id.to_string(),
                            revision_hash,
                            path,
                            source_hash,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, 'later')",
                    [duplicate_source_import_id.to_string()],
                )?;
                Ok(())
            })
            .expect_err("reject duplicate source evidence");
        assert!(
            duplicate_source_error
                .to_string()
                .contains("finalization is invalid")
        );
        let missing_epic_import_id = ImportBatchId::generate();
        let missing_epic_id = EpicId::generate();
        let missing_epic_feature_id = FeatureId::generate();
        let missing_epic_feature_document_id = DocumentId::generate();
        let missing_epic_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'missing-epic', ?3, ?4, ?5,
                               'missing-epic', ?6)",
                    params![
                        missing_epic_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "1".repeat(64),
                        first.planning_commit,
                        repository.id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'missing-epic', 'Missing Epic', 'missing-epic')",
                    params![
                        missing_epic_id.to_string(),
                        workspace.workspace.id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'complete-feature', 'Complete feature', 'planned',
                             'missing-epic')",
                    params![
                        missing_epic_feature_id.to_string(),
                        missing_epic_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, feature_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'feature', 'missing-epic-feature.md', ?4, ?5,
                               'missing-epic')",
                    params![
                        missing_epic_feature_document_id.to_string(),
                        workspace.workspace.planning_store_repository_id.to_string(),
                        missing_epic_feature_id.to_string(),
                        "2".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, 'missing-epic')",
                    params![
                        missing_epic_feature_document_id.to_string(),
                        "2".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_memberships (
                         import_id, document_id, destination_kind
                     ) VALUES (?1, ?2, 'feature')",
                    params![
                        missing_epic_import_id.to_string(),
                        missing_epic_feature_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'missing-epic-feature.md', ?2, 'feature', ?3, ?4)",
                    params![
                        missing_epic_import_id.to_string(),
                        "2".repeat(64),
                        missing_epic_feature_id.to_string(),
                        missing_epic_feature_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, 1, ?3, 'source', 'missing-epic-feature.md', ?4)",
                    params![
                        missing_epic_import_id.to_string(),
                        missing_epic_feature_document_id.to_string(),
                        "2".repeat(64),
                        "2".repeat(64),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, 'missing-epic')",
                    [missing_epic_import_id.to_string()],
                )?;
                Ok(())
            })
            .expect_err("reject finalization without the generated Epic document");
        assert!(
            missing_epic_error
                .to_string()
                .contains("finalization is invalid")
        );
        let epic_only_import_id = ImportBatchId::generate();
        let epic_only_id = EpicId::generate();
        let epic_only_document_id = DocumentId::generate();
        let epic_only_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO import_batches (
                         id, workspace_id, kind, source_path, source_head, preview_hash,
                         planning_commit, imported_at, repository_id
                     ) VALUES (?1, ?2, 'concertable_plans', 'epic-only', ?3, ?4, ?5,
                               'epic-only', ?6)",
                    params![
                        epic_only_import_id.to_string(),
                        workspace.workspace.id.to_string(),
                        preview.source_head,
                        "3".repeat(64),
                        first.planning_commit,
                        repository.id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'epic-only', 'Epic only', 'epic-only')",
                    params![epic_only_id.to_string(), workspace.workspace.id.to_string(),],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'epic', 'epic-only.md', ?4, ?5, 'epic-only')",
                    params![
                        epic_only_document_id.to_string(),
                        workspace.workspace.planning_store_repository_id.to_string(),
                        epic_only_id.to_string(),
                        "4".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, 'epic-only')",
                    params![
                        epic_only_document_id.to_string(),
                        "4".repeat(64),
                        first.planning_commit,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_memberships (
                         import_id, document_id, destination_kind
                     ) VALUES (?1, ?2, 'epic')",
                    params![
                        epic_only_import_id.to_string(),
                        epic_only_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, 'epic-only.md', ?2, 'epic', ?3, ?4)",
                    params![
                        epic_only_import_id.to_string(),
                        "4".repeat(64),
                        epic_only_id.to_string(),
                        epic_only_document_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_evidence (
                         import_id, document_id, revision, revision_content_hash,
                         source_provenance, source_path, source_hash
                     ) VALUES (?1, ?2, 1, ?3, 'source', 'epic-only.md', ?4)",
                    params![
                        epic_only_import_id.to_string(),
                        epic_only_document_id.to_string(),
                        "4".repeat(64),
                        "4".repeat(64),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                     VALUES (?1, 'epic-only')",
                    [epic_only_import_id.to_string()],
                )?;
                Ok(())
            })
            .expect_err("reject source-backed Epic-only finalization");
        assert!(
            epic_only_error
                .to_string()
                .contains("finalization is invalid")
        );
        let second = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("repeat import");
        let snapshot = application
            .snapshot(workspace.workspace.id)
            .expect("snapshot");

        assert!(!first.already_applied);
        assert!(second.already_applied);
        assert_eq!(first.import_id, second.import_id);
        assert_eq!(first.epics, second.epics);
        assert_eq!(first.features, second.features);
        assert_eq!(first.work_items, second.work_items);
        assert_eq!(first.source_destinations, second.source_destinations);
        assert_eq!(first.epics, 2);
        assert_eq!(first.source_destinations, 5);
        assert_eq!(snapshot.epics.len(), 4);
        assert_eq!(snapshot.features.len(), 3);
        assert_eq!(snapshot.work_items.len(), 3);
        assert_eq!(snapshot.documents.len(), 10);
        assert_eq!(
            git_count(&fixture.directory.path().join("planning-store")),
            2
        );
        let (imported_at, revision_content_hash): (String, String) = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT batch.imported_at, revision.content_hash
                           FROM import_batches batch
                           JOIN document_revisions revision
                             ON revision.document_id = ?2 AND revision.revision = 1
                          WHERE batch.id = ?1",
                        params![
                            first.import_id.to_string(),
                            imported_document_id.to_string(),
                        ],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("read finalized import timestamp");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE document_revisions SET observed_at = 'invalid-revision'
                     WHERE document_id = ?1 AND revision = 1",
                    [imported_document_id.to_string()],
                )?;
                Ok(())
            })
            .expect("invalidate finalized revision evidence");
        let revision_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with invalid revision evidence");
        assert!(
            revision_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE document_revisions SET observed_at = ?2
                     WHERE document_id = ?1 AND revision = 1",
                    params![imported_document_id.to_string(), imported_at],
                )?;
                Ok(())
            })
            .expect("restore finalized revision evidence");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE document_revisions SET content_hash = ?2
                     WHERE document_id = ?1 AND revision = 1",
                    params![imported_document_id.to_string(), "6".repeat(64)],
                )?;
                Ok(())
            })
            .expect("mutate finalized revision hash");
        let revision_hash_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with mutated revision hash");
        assert!(
            revision_hash_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE document_revisions SET content_hash = ?2
                     WHERE document_id = ?1 AND revision = 1",
                    params![imported_document_id.to_string(), revision_content_hash],
                )?;
                Ok(())
            })
            .expect("restore finalized revision hash");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 2, ?2, ?3, ?4)",
                    params![
                        imported_document_id.to_string(),
                        "6".repeat(64),
                        first.planning_commit,
                        imported_at,
                    ],
                )?;
                Ok(())
            })
            .expect("duplicate finalized revision tuple");
        let duplicate_revision_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with duplicate revision evidence");
        assert!(
            duplicate_revision_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "DELETE FROM document_revisions WHERE document_id = ?1 AND revision = 2",
                    [imported_document_id.to_string()],
                )?;
                Ok(())
            })
            .expect("remove duplicate revision evidence");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "DELETE FROM document_revisions WHERE document_id = ?1 AND revision = 1",
                    [imported_document_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 2, ?2, ?3, ?4)",
                    params![
                        imported_document_id.to_string(),
                        revision_content_hash,
                        first.planning_commit,
                        imported_at,
                    ],
                )?;
                Ok(())
            })
            .expect("replace finalized revision identity");
        let replacement_revision_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with replaced revision identity");
        assert!(
            replacement_revision_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "DELETE FROM document_revisions WHERE document_id = ?1 AND revision = 2",
                    [imported_document_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, ?4)",
                    params![
                        imported_document_id.to_string(),
                        revision_content_hash,
                        first.planning_commit,
                        imported_at,
                    ],
                )?;
                Ok(())
            })
            .expect("restore finalized revision identity");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 2, ?2, 'later-commit', 'later-revision')",
                    params![imported_document_id.to_string(), "6".repeat(64)],
                )?;
                Ok(())
            })
            .expect("record ordinary later revision");
        application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("allow replay with an ordinary later revision");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "DELETE FROM document_revisions WHERE document_id = ?1 AND revision = 2",
                    [imported_document_id.to_string()],
                )?;
                Ok(())
            })
            .expect("remove ordinary later revision");
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE epics SET created_at = 'invalid-hierarchy' WHERE id = ?1",
                    [imported_epic_id.to_string()],
                )?;
                Ok(())
            })
            .expect("invalidate finalized hierarchy evidence");
        let hierarchy_evidence_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with invalid hierarchy evidence");
        assert!(
            hierarchy_evidence_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE epics SET created_at = ?2 WHERE id = ?1",
                    params![imported_epic_id.to_string(), imported_at],
                )?;
                Ok(())
            })
            .expect("restore finalized hierarchy evidence");
        let extra_epic_id = EpicId::generate();
        let extra_document_id = DocumentId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'extra-evidence', 'Extra evidence', ?3)",
                    params![
                        extra_epic_id.to_string(),
                        workspace.workspace.id.to_string(),
                        imported_at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'epic', 'extra-evidence.md', ?4, ?5, ?6)",
                    params![
                        extra_document_id.to_string(),
                        workspace.workspace.planning_store_repository_id.to_string(),
                        extra_epic_id.to_string(),
                        "5".repeat(64),
                        first.planning_commit,
                        imported_at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, ?4)",
                    params![
                        extra_document_id.to_string(),
                        "5".repeat(64),
                        first.planning_commit,
                        imported_at,
                    ],
                )?;
                Ok(())
            })
            .expect("add same-batch evidence");
        let extra_evidence_error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject replay with additional same-batch evidence");
        assert!(
            extra_evidence_error
                .to_string()
                .contains("invalid finalized document membership evidence")
        );
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "DELETE FROM document_revisions WHERE document_id = ?1",
                    [extra_document_id.to_string()],
                )?;
                transaction.execute(
                    "DELETE FROM documents WHERE id = ?1",
                    [extra_document_id.to_string()],
                )?;
                transaction.execute(
                    "DELETE FROM epics WHERE id = ?1",
                    [extra_epic_id.to_string()],
                )?;
                Ok(())
            })
            .expect("remove additional same-batch evidence");
        let repaired_replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay after restoring finalized evidence");
        assert!(repaired_replay.already_applied);
        assert_eq!(repaired_replay.import_id, first.import_id);
        assert_eq!(repaired_replay.epics, first.epics);
        assert_eq!(repaired_replay.features, first.features);
        assert_eq!(repaired_replay.work_items, first.work_items);
        assert_eq!(
            repaired_replay.source_destinations,
            first.source_destinations
        );
    }

    #[test]
    fn schema_26_source_ambiguity_requires_repair_before_upgrade() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let target_epic_id = preview.epics[0].id;
        let target_document_id = preview.epics[0].document_id;
        drop(application);

        let connection = Connection::open(&database).expect("open schema 30 database");
        restore_schema_26_import_database(&connection);
        connection
            .execute_batch("DROP TRIGGER import_source_destinations_finalized_insert;")
            .expect("open schema 26 source repair slot");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, 'duplicate-source.md', ?2, 'epic', ?3, ?4)",
                params![
                    first.import_id.to_string(),
                    "d".repeat(64),
                    target_epic_id.to_string(),
                    target_document_id.to_string(),
                ],
            )
            .expect("record ambiguous schema 26 source");
        connection
            .execute_batch(
                "CREATE TRIGGER import_source_destinations_finalized_insert
                 BEFORE INSERT ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;",
            )
            .expect("close schema 26 source repair slot");
        drop(connection);

        let upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("ambiguous schema 26 source must stop schema 28"),
            Err(error) => error,
        };
        assert!(upgrade_error.to_string().contains("schema migration 28"));
        assert!(
            upgrade_error
                .to_string()
                .contains(&first.import_id.to_string())
        );
        let connection = Connection::open(&database).expect("open rejected schema 27 database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read rejected source upgrade version");
        let migration_state: (i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM schema_migrations WHERE version = 27),
                     (SELECT COUNT(*) FROM schema_migrations WHERE version = 28)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read rejected source migration stamps");
        let preserved_state: (i64, i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM import_source_destinations
                       WHERE import_id = ?1 AND document_id = ?2),
                     (SELECT COUNT(*) FROM import_document_membership_finalizations
                       WHERE import_id = ?1),
                     (SELECT COUNT(*) FROM import_document_evidence WHERE import_id = ?1)",
                params![first.import_id.to_string(), target_document_id.to_string(),],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read preserved ambiguous source rows");
        assert_eq!(version, 27);
        assert_eq!(migration_state, (1, 0));
        assert_eq!(preserved_state, (2, 1, 0));
        connection
            .execute_batch("DROP TRIGGER import_source_destinations_finalized_delete;")
            .expect("open schema 27 source repair slot");
        connection
            .execute(
                "DELETE FROM import_source_destinations
                  WHERE import_id = ?1 AND source_path = 'duplicate-source.md'",
                [first.import_id.to_string()],
            )
            .expect("remove ambiguous source");
        connection
            .execute_batch(
                "CREATE TRIGGER import_source_destinations_finalized_delete
                 BEFORE DELETE ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = OLD.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;",
            )
            .expect("close schema 27 source repair slot");
        drop(connection);

        let mut application =
            WorkboardApplication::open(&database).expect("complete repaired schema 30 upgrade");
        assert_eq!(
            application
                .store
                .health()
                .expect("repaired schema 30 health")
                .schema_version,
            32
        );
        let (membership_count, evidence_count, source_evidence_count): (i64, i64, i64) = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT
                             (SELECT COUNT(*) FROM import_document_memberships WHERE import_id = ?1),
                             (SELECT COUNT(*) FROM import_document_evidence WHERE import_id = ?1),
                             (SELECT COUNT(*) FROM import_document_evidence
                               WHERE import_id = ?1 AND source_provenance = 'source')",
                        [first.import_id.to_string()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("read repaired durable evidence");
        assert_eq!(evidence_count, membership_count);
        assert_eq!(source_evidence_count, first.source_destinations as i64);
        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay repaired schema 26 import");
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert_eq!(replay.epics, first.epics);
        assert_eq!(replay.features, first.features);
        assert_eq!(replay.work_items, first.work_items);
        assert_eq!(replay.source_destinations, first.source_destinations);
    }

    #[test]
    fn schema_28_missing_synthetic_attestation_requires_repair_before_upgrade() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let mut preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let imported_features = std::mem::take(&mut preview.epics[0].features);
        preview.epics.push(ConcertableEpicImport {
            selected: true,
            id: EpicId::generate(),
            document_id: DocumentId::generate(),
            slug: Slug::new("synthetic-epic").expect("synthetic epic slug"),
            title: "Synthetic epic".to_owned(),
            body: "# Synthetic epic\n".to_owned(),
            source: None,
            features: imported_features,
        });
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let synthetic_document_id = preview.epics[1].document_id;
        drop(application);

        let connection = Connection::open(&database).expect("open schema 30 database");
        restore_schema_28_import_database(&connection);
        connection
            .execute_batch("DROP TRIGGER import_document_synthetic_attestations_no_delete;")
            .expect("open schema 28 attestation repair slot");
        connection
            .execute(
                "DELETE FROM import_document_synthetic_attestations
                  WHERE import_id = ?1 AND document_id = ?2",
                params![
                    first.import_id.to_string(),
                    synthetic_document_id.to_string(),
                ],
            )
            .expect("remove schema 28 synthetic attestation");
        connection
            .execute_batch(
                "CREATE TRIGGER import_document_synthetic_attestations_no_delete
                 BEFORE DELETE ON import_document_synthetic_attestations
                 BEGIN
                     SELECT RAISE(ABORT, 'synthetic import document attestation cannot be deleted');
                 END;",
            )
            .expect("close schema 28 attestation repair slot");
        drop(connection);

        let upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("missing schema 28 attestation must stop schema 29"),
            Err(error) => error,
        };
        assert!(upgrade_error.to_string().contains("schema migration 29"));
        assert!(
            upgrade_error
                .to_string()
                .contains(&first.import_id.to_string())
        );
        let connection = Connection::open(&database).expect("open rejected schema 28 database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read rejected provenance upgrade version");
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 29",
                [],
                |row| row.get(0),
            )
            .expect("count schema 29 stamps");
        let preserved_state: (i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM import_document_evidence
                       WHERE import_id = ?1 AND document_id = ?2
                         AND source_provenance = 'synthetic'),
                     (SELECT COUNT(*) FROM import_document_synthetic_attestations
                       WHERE import_id = ?1 AND document_id = ?2)",
                params![
                    first.import_id.to_string(),
                    synthetic_document_id.to_string(),
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read preserved schema 28 provenance");
        assert_eq!(version, 28);
        assert_eq!(migration_count, 0);
        assert_eq!(preserved_state, (1, 0));
        connection
            .execute(
                "INSERT INTO import_document_synthetic_attestations (
                     import_id, document_id, authority, attested_at
                 ) VALUES (?1, ?2, 'explicit_repair', 'schema-29-repair')",
                params![
                    first.import_id.to_string(),
                    synthetic_document_id.to_string(),
                ],
            )
            .expect("restore verified synthetic attestation");
        drop(connection);

        let mut application =
            WorkboardApplication::open(&database).expect("complete repaired schema 30 upgrade");
        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay repaired schema 28 import");
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert_eq!(replay.epics, first.epics);
        assert_eq!(replay.features, first.features);
        assert_eq!(replay.work_items, first.work_items);
        assert_eq!(replay.source_destinations, first.source_destinations);
    }

    #[test]
    fn schema_28_contradictory_source_attestation_requires_repair_before_upgrade() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let target_document_id = preview.epics[0].document_id;
        let source: (String, String, String, String) = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT source_path, source_hash, destination_kind, destination_id
                           FROM import_source_destinations
                          WHERE import_id = ?1 AND document_id = ?2",
                        params![first.import_id.to_string(), target_document_id.to_string(),],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("read source-backed Epic evidence");
        drop(application);

        let connection = Connection::open(&database).expect("open schema 30 database");
        restore_schema_28_import_database(&connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_source_destinations_finalized_insert;
                 DROP TRIGGER import_source_destinations_finalized_delete;",
            )
            .expect("open schema 28 source repair slots");
        connection
            .execute(
                "DELETE FROM import_source_destinations
                  WHERE import_id = ?1 AND document_id = ?2",
                params![first.import_id.to_string(), target_document_id.to_string(),],
            )
            .expect("temporarily remove schema 28 source");
        connection
            .execute(
                "INSERT INTO import_document_synthetic_attestations (
                     import_id, document_id, authority, attested_at
                 ) VALUES (?1, ?2, 'explicit_repair', 'contradictory-repair')",
                params![first.import_id.to_string(), target_document_id.to_string(),],
            )
            .expect("record contradictory schema 28 attestation");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    first.import_id.to_string(),
                    source.0,
                    source.1,
                    source.2,
                    source.3,
                    target_document_id.to_string(),
                ],
            )
            .expect("restore contradictory schema 28 source");
        connection
            .execute_batch(
                "CREATE TRIGGER import_source_destinations_finalized_insert
                 BEFORE INSERT ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;
                 CREATE TRIGGER import_source_destinations_finalized_delete
                 BEFORE DELETE ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = OLD.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;",
            )
            .expect("close schema 28 source repair slots");
        drop(connection);

        let upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("contradictory schema 28 provenance must stop schema 29"),
            Err(error) => error,
        };
        assert!(upgrade_error.to_string().contains("schema migration 29"));
        assert!(
            upgrade_error
                .to_string()
                .contains(&first.import_id.to_string())
        );
        let connection = Connection::open(&database).expect("open rejected schema 28 database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read rejected contradiction upgrade version");
        let migration_state: (i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM schema_migrations WHERE version = 29),
                     (SELECT COUNT(*) FROM schema_migrations WHERE version = 30)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read rejected contradiction migration stamps");
        let preserved_state: (i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM import_source_destinations
                       WHERE import_id = ?1 AND document_id = ?2),
                     (SELECT COUNT(*) FROM import_document_evidence
                       WHERE import_id = ?1 AND document_id = ?2
                         AND source_provenance = 'source'),
                     (SELECT COUNT(*) FROM import_document_synthetic_attestations
                       WHERE import_id = ?1 AND document_id = ?2),
                     (SELECT COUNT(*) FROM import_document_memberships
                       WHERE import_id = ?1 AND document_id = ?2),
                     (SELECT COUNT(*) FROM import_document_membership_finalizations
                       WHERE import_id = ?1)",
                params![first.import_id.to_string(), target_document_id.to_string(),],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("read preserved contradictory provenance");
        assert_eq!(version, 28);
        assert_eq!(migration_state, (0, 0));
        assert_eq!(preserved_state, (1, 1, 1, 1, 1));
        connection
            .execute_batch("DROP TRIGGER import_document_synthetic_attestations_no_delete;")
            .expect("open schema 28 attestation repair slot");
        connection
            .execute(
                "DELETE FROM import_document_synthetic_attestations
                  WHERE import_id = ?1 AND document_id = ?2",
                params![first.import_id.to_string(), target_document_id.to_string(),],
            )
            .expect("remove contradictory attestation");
        connection
            .execute_batch(
                "CREATE TRIGGER import_document_synthetic_attestations_no_delete
                 BEFORE DELETE ON import_document_synthetic_attestations
                 BEGIN
                     SELECT RAISE(ABORT, 'synthetic import document attestation cannot be deleted');
                 END;",
            )
            .expect("close schema 28 attestation repair slot");
        drop(connection);

        let mut application =
            WorkboardApplication::open(&database).expect("complete repaired schema 30 upgrade");
        assert_eq!(
            application
                .store
                .health()
                .expect("repaired schema 30 health")
                .schema_version,
            32
        );
        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay repaired contradictory provenance");
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert_eq!(replay.epics, first.epics);
        assert_eq!(replay.features, first.features);
        assert_eq!(replay.work_items, first.work_items);
        assert_eq!(replay.source_destinations, first.source_destinations);
    }

    #[test]
    fn schema_24_partial_finalization_requires_repair_before_upgrade() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        drop(application);

        let partial_import_id = ImportBatchId::generate();
        let partial_epic_id = EpicId::generate();
        let partial_feature_id = FeatureId::generate();
        let partial_epic_document_id = DocumentId::generate();
        let partial_feature_document_id = DocumentId::generate();
        let connection = Connection::open(&database).expect("open schema 30 database");
        restore_schema_24_import_database(&connection);
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, kind, source_path, source_head, preview_hash,
                     planning_commit, imported_at, repository_id
                 ) VALUES (?1, ?2, 'concertable_plans', 'partial-schema-24', ?3, ?4, ?5,
                           'schema-24-partial', ?6)",
                params![
                    partial_import_id.to_string(),
                    workspace.workspace.id.to_string(),
                    preview.source_head,
                    "9".repeat(64),
                    first.planning_commit,
                    repository.id.to_string(),
                ],
            )
            .expect("record schema 24 import batch");
        connection
            .execute(
                "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                 VALUES (?1, ?2, 'partial-schema-24', 'Partial schema 24', 'schema-24-partial')",
                params![
                    partial_epic_id.to_string(),
                    workspace.workspace.id.to_string(),
                ],
            )
            .expect("record schema 24 Epic");
        connection
            .execute(
                "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                 VALUES (?1, ?2, 'partial-feature', 'Partial feature', 'planned',
                         'schema-24-partial')",
                params![partial_feature_id.to_string(), partial_epic_id.to_string(),],
            )
            .expect("record schema 24 Feature");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, epic_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'epic', 'partial-schema-24.md', ?4, ?5,
                           'schema-24-partial')",
                params![
                    partial_epic_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    partial_epic_id.to_string(),
                    "7".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("record schema 24 Epic document");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, feature_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'feature', 'partial-schema-24-feature.md', ?4, ?5,
                           'schema-24-partial')",
                params![
                    partial_feature_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    partial_feature_id.to_string(),
                    "8".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("record schema 24 Feature document");
        for (document_id, hash) in [
            (partial_epic_document_id, "7".repeat(64)),
            (partial_feature_document_id, "8".repeat(64)),
        ] {
            connection
                .execute(
                    "INSERT INTO document_revisions (
                         document_id, revision, content_hash, observed_commit, observed_at
                     ) VALUES (?1, 1, ?2, ?3, 'schema-24-partial')",
                    params![document_id.to_string(), hash, first.planning_commit],
                )
                .expect("record schema 24 document revision");
        }
        connection
            .execute(
                "INSERT INTO import_document_memberships (
                     import_id, document_id, destination_kind
                 ) VALUES (?1, ?2, 'epic')",
                params![
                    partial_import_id.to_string(),
                    partial_epic_document_id.to_string(),
                ],
            )
            .expect("record partial schema 24 membership");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, 'partial-schema-24.md', ?2, 'epic', ?3, ?4)",
                params![
                    partial_import_id.to_string(),
                    "7".repeat(64),
                    partial_epic_id.to_string(),
                    partial_epic_document_id.to_string(),
                ],
            )
            .expect("record partial schema 24 source mapping");
        connection
            .execute(
                "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                 VALUES (?1, 'schema-24-partial')",
                [partial_import_id.to_string()],
            )
            .expect("schema 24 accepts partial finalization");
        drop(connection);

        let upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("partial finalization must stop schema 25"),
            Err(error) => error,
        };
        assert!(upgrade_error.to_string().contains("schema migration 25"));
        assert!(
            upgrade_error
                .to_string()
                .contains(&partial_import_id.to_string())
        );

        let connection = Connection::open(&database).expect("reopen schema 24 database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version after rejected upgrade");
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 25",
                [],
                |row| row.get(0),
            )
            .expect("count schema 25 stamps");
        let partial_state: (i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM import_document_memberships WHERE import_id = ?1),
                     (SELECT COUNT(*) FROM import_document_membership_finalizations
                       WHERE import_id = ?1)",
                [partial_import_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read preserved partial finalization");
        assert_eq!(version, 24);
        assert_eq!(migration_count, 0);
        assert_eq!(partial_state, (1, 1));

        connection
            .execute_batch(
                "DROP TRIGGER import_document_memberships_finalized;
                 DROP TRIGGER import_source_destinations_finalized_insert;",
            )
            .expect("open verified schema 24 repair slots");
        connection
            .execute(
                "INSERT INTO import_document_memberships (
                     import_id, document_id, destination_kind
                 ) VALUES (?1, ?2, 'feature')",
                params![
                    partial_import_id.to_string(),
                    partial_feature_document_id.to_string(),
                ],
            )
            .expect("repair schema 24 membership");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, 'partial-schema-24-feature.md', ?2, 'feature', ?3, ?4)",
                params![
                    partial_import_id.to_string(),
                    "8".repeat(64),
                    partial_feature_id.to_string(),
                    partial_feature_document_id.to_string(),
                ],
            )
            .expect("repair schema 24 source mapping");
        connection
            .execute_batch(
                "CREATE TRIGGER import_document_memberships_finalized
                 BEFORE INSERT ON import_document_memberships
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership is finalized');
                 END;
                 CREATE TRIGGER import_source_destinations_finalized_insert
                 BEFORE INSERT ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;",
            )
            .expect("close schema 24 repair slots");
        drop(connection);

        let mut application =
            WorkboardApplication::open(&database).expect("complete schema 30 upgrade");
        assert_eq!(
            application
                .store
                .health()
                .expect("schema 30 health")
                .schema_version,
            32
        );
        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay valid import after repaired upgrade");
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert_eq!(replay.epics, first.epics);
        assert_eq!(replay.features, first.features);
        assert_eq!(replay.work_items, first.work_items);
        assert_eq!(replay.source_destinations, first.source_destinations);
        drop(application);

        let cohort_import_id = ImportBatchId::generate();
        let cohort_epic_id = EpicId::generate();
        let cohort_feature_id = FeatureId::generate();
        let cohort_epic_document_id = DocumentId::generate();
        let cohort_feature_document_id = DocumentId::generate();
        let connection = Connection::open(&database).expect("reopen schema 30 database");
        restore_schema_25_import_database(&connection);
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, kind, source_path, source_head, preview_hash,
                     planning_commit, imported_at, repository_id
                 ) VALUES (?1, ?2, 'concertable_plans', 'schema-25-cohort', ?3, ?4, ?5,
                           'schema-25-cohort', ?6)",
                params![
                    cohort_import_id.to_string(),
                    workspace.workspace.id.to_string(),
                    preview.source_head,
                    "a".repeat(64),
                    first.planning_commit,
                    repository.id.to_string(),
                ],
            )
            .expect("record schema 25 cohort batch");
        connection
            .execute(
                "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                 VALUES (?1, ?2, 'schema-25-cohort', 'Schema 25 cohort', 'schema-25-cohort')",
                params![
                    cohort_epic_id.to_string(),
                    workspace.workspace.id.to_string(),
                ],
            )
            .expect("record schema 25 cohort Epic");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, epic_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'epic', 'schema-25-cohort.md', ?4, ?5,
                           'schema-25-cohort')",
                params![
                    cohort_epic_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    cohort_epic_id.to_string(),
                    "b".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("record schema 25 cohort document");
        connection
            .execute(
                "INSERT INTO document_revisions (
                     document_id, revision, content_hash, observed_commit, observed_at
                 ) VALUES (?1, 1, ?2, ?3, 'schema-25-cohort')",
                params![
                    cohort_epic_document_id.to_string(),
                    "b".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("record schema 25 cohort revision");
        connection
            .execute(
                "INSERT INTO import_document_memberships (
                     import_id, document_id, destination_kind
                 ) VALUES (?1, ?2, 'epic')",
                params![
                    cohort_import_id.to_string(),
                    cohort_epic_document_id.to_string(),
                ],
            )
            .expect("record schema 25 cohort membership");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, 'schema-25-cohort.md', ?2, 'epic', ?3, ?4)",
                params![
                    cohort_import_id.to_string(),
                    "b".repeat(64),
                    cohort_epic_id.to_string(),
                    cohort_epic_document_id.to_string(),
                ],
            )
            .expect("record schema 25 cohort source");
        connection
            .execute(
                "INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
                 VALUES (?1, 'schema-25-cohort')",
                [cohort_import_id.to_string()],
            )
            .expect("schema 25 accepts Epic-only cohort");
        drop(connection);

        let cohort_upgrade_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("incomplete schema 25 cohort must stop schema 26"),
            Err(error) => error,
        };
        assert!(
            cohort_upgrade_error
                .to_string()
                .contains("schema migration 26")
        );
        assert!(
            cohort_upgrade_error
                .to_string()
                .contains(&cohort_import_id.to_string())
        );
        let connection = Connection::open(&database).expect("repair schema 25 cohort");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read rejected cohort upgrade version");
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 26",
                [],
                |row| row.get(0),
            )
            .expect("count schema 26 stamps");
        let cohort_state: (i64, i64) = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM import_document_memberships WHERE import_id = ?1),
                     (SELECT COUNT(*) FROM import_document_membership_finalizations
                       WHERE import_id = ?1)",
                [cohort_import_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read preserved schema 25 cohort");
        assert_eq!(version, 25);
        assert_eq!(migration_count, 0);
        assert_eq!(cohort_state, (1, 1));
        connection
            .execute_batch(
                "DROP TRIGGER import_document_memberships_finalized;
                 DROP TRIGGER import_source_destinations_finalized_insert;",
            )
            .expect("open schema 25 cohort repair slots");
        connection
            .execute(
                "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                 VALUES (?1, ?2, 'schema-25-feature', 'Schema 25 feature', 'planned',
                         'schema-25-cohort')",
                params![cohort_feature_id.to_string(), cohort_epic_id.to_string()],
            )
            .expect("repair schema 25 Feature");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, feature_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'feature', 'schema-25-feature.md', ?4, ?5,
                           'schema-25-cohort')",
                params![
                    cohort_feature_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    cohort_feature_id.to_string(),
                    "c".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("repair schema 25 Feature document");
        connection
            .execute(
                "INSERT INTO document_revisions (
                     document_id, revision, content_hash, observed_commit, observed_at
                 ) VALUES (?1, 1, ?2, ?3, 'schema-25-cohort')",
                params![
                    cohort_feature_document_id.to_string(),
                    "c".repeat(64),
                    first.planning_commit,
                ],
            )
            .expect("repair schema 25 Feature revision");
        connection
            .execute(
                "INSERT INTO import_document_memberships (
                     import_id, document_id, destination_kind
                 ) VALUES (?1, ?2, 'feature')",
                params![
                    cohort_import_id.to_string(),
                    cohort_feature_document_id.to_string(),
                ],
            )
            .expect("repair schema 25 Feature membership");
        connection
            .execute(
                "INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (?1, 'schema-25-feature.md', ?2, 'feature', ?3, ?4)",
                params![
                    cohort_import_id.to_string(),
                    "c".repeat(64),
                    cohort_feature_id.to_string(),
                    cohort_feature_document_id.to_string(),
                ],
            )
            .expect("repair schema 25 Feature source");
        connection
            .execute_batch(
                "CREATE TRIGGER import_document_memberships_finalized
                 BEFORE INSERT ON import_document_memberships
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership is finalized');
                 END;
                 CREATE TRIGGER import_source_destinations_finalized_insert
                 BEFORE INSERT ON import_source_destinations
                 WHEN EXISTS (
                     SELECT 1 FROM import_document_membership_finalizations finalization
                      WHERE finalization.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import source destinations are finalized');
                 END;",
            )
            .expect("close schema 25 cohort repair slots");
        drop(connection);

        let mut application =
            WorkboardApplication::open(&database).expect("complete schema 30 cohort upgrade");
        assert_eq!(
            application
                .store
                .health()
                .expect("repaired schema 30 health")
                .schema_version,
            32
        );
        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay after schema 25 cohort repair");
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert_eq!(replay.epics, first.epics);
        assert_eq!(replay.features, first.features);
        assert_eq!(replay.work_items, first.work_items);
        assert_eq!(replay.source_destinations, first.source_destinations);
    }

    #[test]
    fn schema_23_upgrade_rejects_incomplete_and_ambiguous_membership() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let mapped_document_id = preview.epics[0].features[0].document_id;
        let unrelated_epic_id = EpicId::generate();
        let unrelated_document_id = DocumentId::generate();
        drop(application);

        let connection = Connection::open(&database).expect("open schema 24 database");
        restore_schema_23_import_database(&connection);
        let revision: (String, String, String) = connection
            .query_row(
                "SELECT content_hash, observed_commit, observed_at
                   FROM document_revisions WHERE document_id = ?1",
                [mapped_document_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read mapped revision");
        connection
            .execute(
                "DELETE FROM document_revisions WHERE document_id = ?1",
                [mapped_document_id.to_string()],
            )
            .expect("remove mapped revision");
        drop(connection);

        let missing_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("missing mapped revision must stop schema 24"),
            Err(error) => error,
        };
        assert!(missing_error.to_string().contains("schema migration 24"));
        assert!(
            missing_error
                .to_string()
                .contains(&first.import_id.to_string())
        );

        let connection = Connection::open(&database).expect("reopen schema 23 database");
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 24",
                [],
                |row| row.get(0),
            )
            .expect("count schema 24 stamps");
        assert_eq!(migration_count, 0);
        connection
            .execute(
                "INSERT INTO document_revisions (
                     document_id, revision, content_hash, observed_commit, observed_at
                 ) VALUES (?1, 1, ?2, ?3, ?4)",
                params![
                    mapped_document_id.to_string(),
                    revision.0,
                    revision.1,
                    revision.2,
                ],
            )
            .expect("restore mapped revision");
        let imported_at: String = connection
            .query_row(
                "SELECT imported_at FROM import_batches WHERE id = ?1",
                [first.import_id.to_string()],
                |row| row.get(0),
            )
            .expect("read import timestamp");
        let unrelated_hash = "e".repeat(64);
        connection
            .execute(
                "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                 VALUES (?1, ?2, 'ambiguous', 'Ambiguous', ?3)",
                params![
                    unrelated_epic_id.to_string(),
                    workspace.workspace.id.to_string(),
                    imported_at,
                ],
            )
            .expect("record ambiguous epic");
        connection
            .execute(
                "INSERT INTO documents (
                     id, repository_id, epic_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'epic', 'ambiguous.md', ?4, ?5, ?6)",
                params![
                    unrelated_document_id.to_string(),
                    workspace.workspace.planning_store_repository_id.to_string(),
                    unrelated_epic_id.to_string(),
                    unrelated_hash,
                    first.planning_commit,
                    imported_at,
                ],
            )
            .expect("record ambiguous document");
        connection
            .execute(
                "INSERT INTO document_revisions (
                     document_id, revision, content_hash, observed_commit, observed_at
                 ) VALUES (?1, 1, ?2, ?3, ?4)",
                params![
                    unrelated_document_id.to_string(),
                    unrelated_hash,
                    first.planning_commit,
                    imported_at,
                ],
            )
            .expect("record ambiguous revision");
        drop(connection);

        let ambiguous_error = match WorkboardApplication::open(&database) {
            Ok(_) => panic!("ambiguous same-batch evidence must stop schema 24"),
            Err(error) => error,
        };
        assert!(ambiguous_error.to_string().contains("schema migration 24"));

        let connection = Connection::open(&database).expect("repair schema 23 database");
        connection
            .execute(
                "DELETE FROM document_revisions WHERE document_id = ?1",
                [unrelated_document_id.to_string()],
            )
            .expect("remove ambiguous revision");
        connection
            .execute(
                "DELETE FROM documents WHERE id = ?1",
                [unrelated_document_id.to_string()],
            )
            .expect("remove ambiguous document");
        connection
            .execute(
                "DELETE FROM epics WHERE id = ?1",
                [unrelated_epic_id.to_string()],
            )
            .expect("remove ambiguous epic");
        drop(connection);

        let application =
            WorkboardApplication::open(&database).expect("complete schema 24 upgrade");
        let membership_count: i64 = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM import_document_memberships WHERE import_id = ?1",
                        [first.import_id.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("count repaired membership");
        assert_eq!(
            membership_count,
            i64::try_from(first.epics + first.features + first.work_items)
                .expect("membership count")
        );
        assert_eq!(
            application
                .store
                .health()
                .expect("upgraded storage health")
                .schema_version,
            32
        );
    }

    #[test]
    fn apply_rejects_an_unrelated_target_repository() {
        let fixture = Fixture::new();
        let unrelated = fixture.directory.path().join("Unrelated");
        fs::create_dir_all(&unrelated).expect("create unrelated repository");
        fs::write(unrelated.join("README.md"), "# Unrelated\n").expect("write unrelated file");
        initialise_repository(&unrelated);
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register source repository");
        let unrelated = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("unrelated").expect("repository slug"),
                title: "Unrelated".to_owned(),
                path: unrelated,
            })
            .expect("register unrelated repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");

        let error = application
            .apply_concertable_import(workspace.workspace.id, unrelated.id, &preview)
            .expect_err("reject unrelated target");

        assert!(error.to_string().contains("target does not match"));
        assert!(
            application
                .snapshot(workspace.workspace.id)
                .expect("snapshot")
                .epics
                .is_empty()
        );
    }

    #[test]
    fn replay_is_scoped_and_survives_source_retirement() {
        let fixture = Fixture::new();
        let unrelated_path = fixture.directory.path().join("Unrelated");
        fs::create_dir_all(&unrelated_path).expect("create unrelated repository");
        fs::write(unrelated_path.join("README.md"), "# Unrelated\n").expect("write unrelated file");
        initialise_repository(&unrelated_path);
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register source repository");
        let unrelated = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("unrelated").expect("repository slug"),
                title: "Unrelated".to_owned(),
                path: unrelated_path,
            })
            .expect("register unrelated repository");
        let preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        let imported_work_item = application
            .snapshot(workspace.workspace.id)
            .expect("snapshot imported Work items")
            .work_items[0]
            .id;
        let association_error = application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    [imported_work_item.to_string(), unrelated.id.to_string()],
                )?;
                Ok(())
            })
            .expect_err("reject imported Work item association after finalization");
        assert!(
            association_error
                .to_string()
                .contains("imported Work item repositories are finalized")
        );
        fs::rename(
            &fixture.source,
            fixture.directory.path().join("Retired-Concertable"),
        )
        .expect("retire source repository");

        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay import");
        let other_target =
            application.apply_concertable_import(workspace.workspace.id, unrelated.id, &preview);

        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
        assert!(other_target.is_err());
    }

    #[test]
    fn replay_without_selected_work_items_survives_source_retirement() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store,
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        let mut preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        for item in &mut preview.epics[0].features[0].work_items {
            item.selected = false;
        }
        let first = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply import");
        fs::rename(
            &fixture.source,
            fixture.directory.path().join("Retired-Concertable"),
        )
        .expect("retire source repository");

        let replay = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect("replay import");

        assert_eq!(first.work_items, 0);
        assert!(replay.already_applied);
        assert_eq!(replay.import_id, first.import_id);
    }

    #[test]
    fn document_id_collision_precedes_planning_publication() {
        let fixture = Fixture::new();
        let database = fixture.directory.path().join("workboard.sqlite");
        let planning_store = fixture.directory.path().join("planning-store");
        let mut application = WorkboardApplication::open(&database).expect("open Workboard");
        let workspace = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("demo").expect("workspace slug"),
                title: "Demo".to_owned(),
                planning_store_path: planning_store.clone(),
            })
            .expect("initialise workspace");
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("concertable").expect("repository slug"),
                title: "Concertable".to_owned(),
                path: fixture.source.clone(),
            })
            .expect("register repository");
        application
            .create_epic(CreateEpic {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("existing").expect("Epic slug"),
                title: "Existing".to_owned(),
                body: "# Existing\n\nExisting work.\n".to_owned(),
            })
            .expect("create existing Epic");
        let before = application
            .snapshot(workspace.workspace.id)
            .expect("snapshot before collision");
        let existing_document_id = before.documents[0].id;
        let head = git_head(&planning_store);
        let mut preview = preview_concertable_plans(&fixture.source).expect("preview plans");
        preview.epics[0].document_id = existing_document_id;

        let error = application
            .apply_concertable_import(workspace.workspace.id, repository.id, &preview)
            .expect_err("reject document collision");
        let after = application
            .snapshot(workspace.workspace.id)
            .expect("snapshot after collision");

        assert!(matches!(error, crate::AppError::IdempotencyConflict));
        assert_eq!(git_head(&planning_store), head);
        assert_eq!(after.epics, before.epics);
        assert_eq!(after.documents, before.documents);
    }

    struct Fixture {
        directory: TempDir,
        source: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = TempDir::new().expect("temporary directory");
            let source = directory.path().join("Concertable");
            fs::create_dir_all(source.join("plans/launch")).expect("create plans");
            fs::write(
                source.join("plans/launch/LAUNCH_ROADMAP.md"),
                "# Launch roadmap\n\n## Outcome\n\nShip.\n",
            )
            .expect("write roadmap");
            fs::write(
                source.join("plans/launch/AVAILABILITY_PLAN.md"),
                "# Availability\n\n## Phases\n\n### Phase 1 — API ✅\n\nBuild API.\n\n### Phase 2 — UI\n\nBuild UI.\n",
            )
            .expect("write plan");
            fs::write(
                source.join("plans/launch/AVAILABILITY_PROGRESS.md"),
                "# Availability progress\n\n## Current state\n\nAPI landed.\n",
            )
            .expect("write progress");
            successful(
                Command::new("git")
                    .arg("init")
                    .args(["-b", "main"])
                    .arg(&source),
            );
            successful(
                Command::new("git")
                    .arg("-C")
                    .arg(&source)
                    .args(["add", "."]),
            );
            successful(
                Command::new("git")
                    .arg("-C")
                    .arg(&source)
                    .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
                    .args(["commit", "-m", "Seed plans"]),
            );
            Self { directory, source }
        }
    }

    fn initialise_repository(source: &Path) {
        successful(
            Command::new("git")
                .arg("init")
                .args(["-b", "main"])
                .arg(source),
        );
        successful(Command::new("git").arg("-C").arg(source).args(["add", "."]));
        successful(
            Command::new("git")
                .arg("-C")
                .arg(source)
                .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
                .args(["commit", "-m", "Seed plans"]),
        );
    }

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).expect("create directory link");
        true
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        match std::os::windows::fs::symlink_dir(target, link) {
            Ok(()) => true,
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                false
            }
            Err(error) => panic!("create directory link: {error}"),
        }
    }

    fn successful(command: &mut Command) {
        let output = command.output().expect("run Git");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn restore_schema_30_import_database(connection: &Connection) {
        crate::storage::drop_workspace_planning_schema(connection);
        connection
            .execute_batch(
                "DROP TABLE checkout_reconciliation_events;
                 DROP TABLE checkout_readiness;
                 ALTER TABLE managed_session_requests DROP COLUMN repository_id;
                 ALTER TABLE managed_session_requests DROP COLUMN readiness_generation;
                 ALTER TABLE managed_session_requests DROP COLUMN checkout_id;
                 DELETE FROM schema_migrations WHERE version = 31;
                 PRAGMA user_version = 30;",
            )
            .expect("restore schema 30 import database");
    }

    fn git_count(root: &Path) -> usize {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .expect("count commits");
        String::from_utf8(output.stdout)
            .expect("UTF-8 count")
            .trim()
            .parse()
            .expect("numeric count")
    }

    fn restore_schema_28_import_database(connection: &Connection) {
        restore_schema_30_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_document_membership_finalizations_valid;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_replace;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_replace;
                 DROP TRIGGER IF EXISTS import_source_destinations_no_synthetic_attestation_insert;
                 DROP TRIGGER IF EXISTS import_source_destinations_no_synthetic_attestation_update;
                 DROP TRIGGER IF EXISTS import_document_evidence_source_attestation_valid;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_source_evidence;
                 DROP VIEW IF EXISTS concertable_import_provenance_consistency_failures;
                 CREATE TRIGGER import_document_membership_finalizations_valid
                 BEFORE INSERT ON import_document_membership_finalizations
                 WHEN NOT EXISTS (
                     SELECT 1 FROM import_batches batch
                      WHERE batch.id = NEW.import_id
                        AND batch.kind = 'concertable_plans'
                        AND batch.imported_at = NEW.finalized_at
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_membership_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_cohort_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_durable_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership finalization is invalid');
                 END;
                 DELETE FROM schema_migrations WHERE version >= 29;
                 PRAGMA user_version = 28;",
            )
            .expect("restore schema 28 import database");
    }

    fn restore_schema_26_import_database(connection: &Connection) {
        restore_schema_30_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_document_membership_finalizations_valid;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_replace;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_replace;
                 DROP TRIGGER IF EXISTS import_source_destinations_no_synthetic_attestation_insert;
                 DROP TRIGGER IF EXISTS import_source_destinations_no_synthetic_attestation_update;
                 DROP TRIGGER IF EXISTS import_document_evidence_source_attestation_valid;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_source_evidence;
                 DROP VIEW IF EXISTS concertable_import_provenance_consistency_failures;
                 DROP VIEW IF EXISTS concertable_import_durable_evidence_failures;
                 DROP TRIGGER IF EXISTS import_document_evidence_valid;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_update;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_delete;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_valid;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_update;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_delete;
                 DROP TABLE IF EXISTS import_document_evidence;
                 DROP TABLE IF EXISTS import_document_synthetic_attestations;
                 CREATE TRIGGER import_document_membership_finalizations_valid
                 BEFORE INSERT ON import_document_membership_finalizations
                 WHEN NOT EXISTS (
                     SELECT 1 FROM import_batches batch
                      WHERE batch.id = NEW.import_id
                        AND batch.kind = 'concertable_plans'
                        AND batch.imported_at = NEW.finalized_at
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_membership_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_cohort_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership finalization is invalid');
                 END;
                 DELETE FROM schema_migrations WHERE version >= 27;
                 PRAGMA user_version = 26;",
            )
            .expect("restore schema 26 import database");
    }

    fn restore_schema_25_import_database(connection: &Connection) {
        restore_schema_26_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_document_membership_finalizations_valid;
                 DROP VIEW IF EXISTS concertable_import_cohort_evidence_failures;
                 CREATE TRIGGER import_document_membership_finalizations_valid
                 BEFORE INSERT ON import_document_membership_finalizations
                 WHEN NOT EXISTS (
                     SELECT 1 FROM import_batches batch
                      WHERE batch.id = NEW.import_id
                        AND batch.kind = 'concertable_plans'
                        AND batch.imported_at = NEW.finalized_at
                 )
                 OR EXISTS (
                     SELECT 1 FROM concertable_import_membership_evidence_failures failure
                      WHERE failure.import_id = NEW.import_id
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership finalization is invalid');
                 END;
                 DELETE FROM schema_migrations WHERE version >= 26;
                 PRAGMA user_version = 25;",
            )
            .expect("restore schema 25 import database");
    }

    fn restore_schema_24_import_database(connection: &Connection) {
        restore_schema_25_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_insert;
                 DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_update;
                 DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_delete;
                 DROP VIEW IF EXISTS concertable_import_durable_evidence_failures;
                 DROP TRIGGER IF EXISTS import_document_evidence_valid;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_update;
                 DROP TRIGGER IF EXISTS import_document_evidence_no_delete;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_valid;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_update;
                 DROP TRIGGER IF EXISTS import_document_synthetic_attestations_no_delete;
                 DROP TABLE IF EXISTS import_document_evidence;
                 DROP TABLE IF EXISTS import_document_synthetic_attestations;
                 DROP TRIGGER IF EXISTS import_planning_repository_parent_immutable;
                 DROP TRIGGER IF EXISTS import_workspace_planning_repository_immutable;
                 DROP TRIGGER IF EXISTS import_work_item_membership_parent_immutable;
                 DROP TRIGGER IF EXISTS import_feature_membership_parent_immutable;
                 DROP TRIGGER IF EXISTS import_epic_membership_parent_immutable;
                 DROP VIEW IF EXISTS concertable_import_cohort_evidence_failures;
                 DROP VIEW IF EXISTS concertable_import_membership_evidence_failures;
                 DROP VIEW IF EXISTS concertable_import_expected_documents;
                 DROP TRIGGER IF EXISTS import_document_membership_finalizations_valid;
                 DROP TRIGGER IF EXISTS import_document_member_fields_immutable;
                 CREATE TRIGGER import_document_membership_finalizations_valid
                 BEFORE INSERT ON import_document_membership_finalizations
                 WHEN NOT EXISTS (
                     SELECT 1 FROM import_batches batch
                      WHERE batch.id = NEW.import_id
                        AND batch.kind = 'concertable_plans'
                        AND EXISTS (
                            SELECT 1 FROM import_document_memberships membership
                             WHERE membership.import_id = batch.id
                        )
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership finalization is invalid');
                 END;
                 CREATE TRIGGER import_document_member_fields_immutable
                 BEFORE UPDATE OF repository_id, kind ON documents
                 WHEN (NEW.repository_id IS NOT OLD.repository_id OR NEW.kind IS NOT OLD.kind)
                      AND EXISTS (
                          SELECT 1 FROM import_document_memberships membership
                           WHERE membership.document_id = OLD.id
                      )
                 BEGIN
                     SELECT RAISE(ABORT, 'import document membership fields are immutable');
                 END;
                 DELETE FROM schema_migrations WHERE version >= 25;
                 PRAGMA user_version = 24;",
            )
            .expect("restore schema 24 import database");
    }

    fn restore_schema_23_import_database(connection: &Connection) {
        restore_schema_24_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_insert;
                 DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_update;
                 DROP TRIGGER IF EXISTS import_work_item_repositories_finalized_delete;
                 DROP TRIGGER IF EXISTS import_planning_repository_parent_immutable;
                 DROP TRIGGER IF EXISTS import_workspace_planning_repository_immutable;
                 DROP TRIGGER IF EXISTS import_work_item_membership_parent_immutable;
                 DROP TRIGGER IF EXISTS import_feature_membership_parent_immutable;
                 DROP TRIGGER IF EXISTS import_epic_membership_parent_immutable;
                 DROP VIEW IF EXISTS concertable_import_membership_evidence_failures;
                 DROP VIEW IF EXISTS concertable_import_expected_documents;
                 DROP TRIGGER import_source_destinations_finalized_insert;
                 DROP TRIGGER import_source_destinations_finalized_update;
                 DROP TRIGGER import_source_destinations_finalized_delete;
                 DROP TRIGGER import_document_batches_finalized;
                 DROP TRIGGER import_document_member_fields_immutable;
                 DROP TRIGGER import_document_memberships_finalized;
                 DROP TRIGGER import_document_membership_finalizations_no_update;
                 DROP TRIGGER import_document_membership_finalizations_no_delete;
                 DROP TRIGGER import_document_membership_finalizations_valid;
                 DROP TABLE import_document_membership_finalizations;
                 DELETE FROM schema_migrations WHERE version >= 24;
                 PRAGMA user_version = 23;",
            )
            .expect("restore schema 23 import database");
    }

    fn restore_schema_22_import_database(connection: &Connection) {
        restore_schema_23_import_database(connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_document_memberships_no_update;
                 DROP TRIGGER import_document_memberships_no_delete;
                 DROP TRIGGER import_document_memberships_valid;
                 DROP TABLE import_document_memberships;
                 DELETE FROM schema_migrations WHERE version >= 23;
                 PRAGMA user_version = 22;",
            )
            .expect("restore schema 22 import database");
    }

    fn git_head(root: &Path) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("read HEAD");
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .expect("UTF-8 HEAD")
            .trim()
            .to_owned()
    }
}
