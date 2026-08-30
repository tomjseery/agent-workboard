use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use workboard_core::{
    AssociationIntervalId, Checkout, CheckoutAvailability, CheckoutId, CheckoutPathId,
    CheckoutPathInterval, ConversationId, ConversationRef, DocumentId, EffectiveCheckout, Epic,
    EpicId, Feature, HierarchyOwner, LaunchProfile, LaunchProfileSource, ManagedSessionRole,
    MarkdownDocument, NativeSession, NativeSessionAssociation, ReasoningEffort, Repository,
    RepositoryId, RepositoryPath, RepositoryPathId, RepositoryRemote, Slug, Tool, WorkItem,
    WorkItemId, WorkItemKey, WorkItemStatus, WorkflowState, Workspace, WorkspaceId,
    WorkspaceSnapshot,
};

use crate::AppError;
use crate::checkout::CheckoutService;
use crate::follow_up::FollowUpService;
use crate::git::{GitCli, GitRepositoryDiscovery, GitWorktreeResolver};
use crate::integration_service::IntegrationService;
use crate::native_sources::NativeSourceService;
use crate::planning_store::{DocumentFrontMatter, PlanningStore, StoredDocument};
use crate::planning_workflow::PlanningWorkflowService;
use crate::recovery::RecoveryService;
use crate::session_launch::SessionLaunchService;
use crate::storage::{SqliteStore, StorageHealth};
use crate::work_projection::{WorkItemProjection, WorkProjectionService};
use crate::workflow_operations::{AssignedContext, WorkflowOperationService};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialiseWorkspace {
    pub slug: Slug,
    pub title: String,
    pub planning_store_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRepository {
    pub workspace_id: WorkspaceId,
    pub slug: Slug,
    pub title: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEpic {
    pub workspace_id: WorkspaceId,
    pub slug: Slug,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedCheckout {
    pub checkout_id: CheckoutId,
    pub repository_id: RepositoryId,
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedSessionTarget {
    pub session_id: ConversationId,
    pub owner: HierarchyOwner,
    pub role: ManagedSessionRole,
    pub tool: Tool,
    pub native_id: String,
    pub profile: LaunchProfile,
    pub checkout: ManagedCheckout,
}

pub struct WorkboardApplication {
    pub(crate) store: SqliteStore,
}

impl WorkboardApplication {
    pub fn open(database_path: impl Into<PathBuf>) -> Result<Self, AppError> {
        Ok(Self {
            store: SqliteStore::open(database_path)?,
        })
    }

    pub fn database_path(&self) -> &Path {
        self.store.path()
    }

    pub fn session_launch(&mut self) -> SessionLaunchService<'_> {
        SessionLaunchService::new(&mut self.store)
    }

    pub fn checkout_service(&mut self) -> CheckoutService<'_> {
        CheckoutService::new(&mut self.store)
    }

    pub fn work_item_projection(
        &self,
        work_item_id: WorkItemId,
    ) -> Result<WorkItemProjection, AppError> {
        WorkProjectionService::new(&self.store).project(work_item_id)
    }

    pub fn preferred_launch_profile(
        &self,
        workspace_id: WorkspaceId,
        tool: Tool,
        role: ManagedSessionRole,
    ) -> Result<LaunchProfile, AppError> {
        let stored = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT profile.schema_version, profile.model, profile.effort
                     FROM launch_profile_preferences preference
                     JOIN launch_profiles profile ON profile.id = preference.profile_id
                     WHERE preference.workspace_id = ?1 AND preference.provider = ?2
                       AND preference.role = ?3",
                    params![
                        workspace_id.to_string(),
                        tool_name(tool),
                        session_role_name(role)?,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, u32>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        stored.map_or_else(
            || Ok(LaunchProfile::suggested(tool, role)),
            |(schema_version, model, effort)| {
                let profile = LaunchProfile {
                    schema_version,
                    tool,
                    model: Some(model),
                    effort: Some(parse_reasoning_effort(&effort)?),
                    role,
                    source: LaunchProfileSource::Preference,
                };
                profile
                    .validate_for_launch(tool, role)
                    .map_err(|error| AppError::Domain(error.to_string()))?;
                Ok(profile)
            },
        )
    }

    pub fn remember_launch_profile(
        &mut self,
        workspace_id: WorkspaceId,
        profile: &LaunchProfile,
        updated_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        profile
            .validate_for_launch(profile.tool, profile.role)
            .map_err(|error| AppError::Domain(error.to_string()))?;
        let profile_id = uuid::Uuid::new_v4().to_string();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO launch_profiles (
                     id, schema_version, provider, model, effort, role, source, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'preference', ?7)",
                params![
                    profile_id,
                    i64::from(profile.schema_version),
                    tool_name(profile.tool),
                    profile.model.as_deref(),
                    profile.effort.map(ReasoningEffort::as_str),
                    session_role_name(profile.role)?,
                    updated_at.unix_timestamp_nanos().to_string(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO launch_profile_preferences (
                     workspace_id, provider, role, profile_id, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(workspace_id, provider, role) DO UPDATE SET
                     profile_id = excluded.profile_id,
                     updated_at = excluded.updated_at",
                params![
                    workspace_id.to_string(),
                    tool_name(profile.tool),
                    session_role_name(profile.role)?,
                    profile_id,
                    updated_at.unix_timestamp_nanos().to_string(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn native_sources(&mut self) -> NativeSourceService<'_> {
        NativeSourceService::new(&mut self.store)
    }

    pub fn integrations(&mut self) -> IntegrationService<'_> {
        IntegrationService::new(&mut self.store)
    }

    pub fn planning_workflows(&mut self) -> PlanningWorkflowService<'_> {
        PlanningWorkflowService::new(&mut self.store)
    }

    pub fn recovery(&mut self) -> RecoveryService<'_> {
        RecoveryService::new(&mut self.store)
    }

    pub fn managed_transcript_roots(&self, tool: Tool) -> Result<Vec<PathBuf>, AppError> {
        let directory = match tool {
            Tool::Claude => "projects",
            Tool::Codex => "sessions",
        };
        let provider = match tool {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
        };
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT DISTINCT capability_bundle_root FROM launch_intents
                 WHERE capability_bundle_root IS NOT NULL AND provider = ?1",
            )?;
            let roots = statement
                .query_map([provider], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(roots
                .into_iter()
                .map(|root| PathBuf::from(root).join(directory))
                .filter(|path| path.is_dir())
                .collect())
        })
    }

    pub fn workspace_planning(
        &mut self,
    ) -> crate::workspace_planning::WorkspacePlanningService<'_> {
        crate::workspace_planning::WorkspacePlanningService::new(&mut self.store)
    }

    pub fn workflow_operations(&mut self) -> WorkflowOperationService<'_> {
        WorkflowOperationService::new(&mut self.store)
    }

    pub fn follow_ups(&mut self) -> FollowUpService<'_> {
        FollowUpService::new(&mut self.store)
    }

    pub fn assigned_hierarchy(
        &mut self,
        workflow_token: &str,
        observed_at: OffsetDateTime,
    ) -> Result<AssignedContext, AppError> {
        let principal = self
            .workflow_operations()
            .authenticate(workflow_token, observed_at)?;
        let snapshot = self.snapshot(principal.workspace_id)?;
        let (epic, feature, work_item) = match principal.owner {
            HierarchyOwner::Workspace(_) => (None, None, None),
            HierarchyOwner::Epic(epic_id) => (
                Some(
                    snapshot
                        .epics
                        .iter()
                        .find(|epic| epic.id == epic_id)
                        .cloned()
                        .ok_or_else(|| missing_assigned_entity("epic"))?,
                ),
                None,
                None,
            ),
            HierarchyOwner::Feature(feature_id) => {
                let feature = snapshot
                    .features
                    .iter()
                    .find(|feature| feature.id == feature_id)
                    .cloned()
                    .ok_or_else(|| missing_assigned_entity("feature"))?;
                let epic = snapshot
                    .epics
                    .iter()
                    .find(|epic| epic.id == feature.epic_id)
                    .cloned()
                    .ok_or_else(|| missing_assigned_entity("epic"))?;
                (Some(epic), Some(feature), None)
            }
            HierarchyOwner::WorkItem(work_item_id) => {
                let work_item = snapshot
                    .work_items
                    .iter()
                    .find(|item| item.id == work_item_id)
                    .cloned()
                    .ok_or(AppError::WorkItemNotFound)?;
                let feature = snapshot
                    .features
                    .iter()
                    .find(|feature| feature.id == work_item.feature_id)
                    .cloned()
                    .ok_or_else(|| missing_assigned_entity("feature"))?;
                let epic = snapshot
                    .epics
                    .iter()
                    .find(|epic| epic.id == feature.epic_id)
                    .cloned()
                    .ok_or_else(|| missing_assigned_entity("epic"))?;
                (Some(epic), Some(feature), Some(work_item))
            }
        };
        let lineage_owners = epic
            .iter()
            .map(|epic| HierarchyOwner::Epic(epic.id))
            .chain(
                feature
                    .iter()
                    .map(|feature| HierarchyOwner::Feature(feature.id)),
            )
            .chain(
                work_item
                    .iter()
                    .map(|item| HierarchyOwner::WorkItem(item.id)),
            )
            .collect::<Vec<_>>();
        let dependency_items = if let Some(work_item) = &work_item {
            self.workflow_operations()
                .assigned_dependency_ids(work_item.id)?
                .into_iter()
                .map(|dependency_id| {
                    snapshot
                        .work_items
                        .iter()
                        .find(|item| item.id == dependency_id)
                        .cloned()
                        .ok_or(AppError::WorkItemNotFound)
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let mut document_owners = lineage_owners.clone();
        document_owners.extend(
            dependency_items
                .iter()
                .map(|item| HierarchyOwner::WorkItem(item.id)),
        );
        let checkout_ids = if let Some(work_item) = &work_item {
            let mut checkout_ids = snapshot
                .effective_checkouts
                .iter()
                .filter(|checkout| checkout.work_item_id == Some(work_item.id))
                .map(|checkout| checkout.checkout_id)
                .collect::<Vec<_>>();
            checkout_ids.sort_by_key(ToString::to_string);
            checkout_ids.dedup();
            if checkout_ids.len() != work_item.repository_ids.len()
                || !checkout_ids.contains(&principal.checkout_id)
            {
                return Err(AppError::ResumeCheckoutRequired);
            }
            checkout_ids
        } else {
            vec![principal.checkout_id]
        };
        let repositories = checkout_ids
            .into_iter()
            .map(|checkout_id| {
                self.workflow_operations()
                    .assigned_repository_checkout(&principal, checkout_id)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let documents = self
            .workflow_operations()
            .assigned_documents(&principal, &document_owners)?;
        if documents.len() != document_owners.len() {
            return Err(AppError::External {
                code: "assigned_document_missing".to_owned(),
                message: "the authenticated hierarchy does not have every required document"
                    .to_owned(),
            });
        }
        let sessions = self
            .workflow_operations()
            .assigned_sessions(&principal, &lineage_owners)?;
        let dependencies = dependency_items
            .into_iter()
            .map(|dependency| {
                let document = documents
                    .iter()
                    .find(|document| {
                        document.document.owner == HierarchyOwner::WorkItem(dependency.id)
                    })
                    .cloned()
                    .ok_or_else(|| missing_assigned_entity("dependency_document"))?;
                Ok(crate::workflow_operations::AssignedDependency {
                    work_item: dependency,
                    document,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(AssignedContext {
            schema_version: 2,
            principal,
            epic,
            feature,
            work_item,
            dependencies,
            repositories,
            documents,
            sessions,
        })
    }

    pub fn effective_work_item_checkout(
        &self,
        work_item_id: WorkItemId,
    ) -> Result<ManagedCheckout, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT checkout.id, checkout.repository_id, path.path, item.title
                     FROM effective_work_item_checkouts effective
                     JOIN checkouts checkout
                       ON checkout.id = effective.checkout_id
                      AND checkout.availability = 'available'
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     JOIN work_items item ON item.id = effective.work_item_id
                     WHERE effective.work_item_id = ?1
                     ORDER BY checkout.repository_id",
            )?;
            let rows = statement
                .query_map([work_item_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let [(checkout_id, repository_id, path, title)] = rows.as_slice() else {
                return if rows.is_empty() {
                    Err(AppError::ResumeCheckoutRequired)
                } else {
                    Err(AppError::External {
                        code: "checkout_selection_required".to_owned(),
                        message: "the Work item has multiple repository checkouts; select one"
                            .to_owned(),
                    })
                };
            };
            Ok(ManagedCheckout {
                checkout_id: parse_id(checkout_id)?,
                repository_id: parse_id(repository_id)?,
                path: PathBuf::from(path),
                title: title.clone(),
            })
        })
    }

    pub fn ensure_repository_checkout(
        &mut self,
        repository_id: RepositoryId,
        observed_at: OffsetDateTime,
    ) -> Result<ManagedCheckout, AppError> {
        let (path, title) = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT path.path, repository.title
                     FROM repositories repository
                     JOIN repository_paths path
                       ON path.repository_id = repository.id AND path.observed_until IS NULL
                     WHERE repository.id = ?1 AND repository.is_planning_store = 0",
                    [repository_id.to_string()],
                    |row| {
                        Ok((
                            PathBuf::from(row.get::<_, String>(0)?),
                            row.get::<_, String>(1)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(AppError::ResumeRepositoryMismatch)
        })?;
        let resolved = GitCli.resolve(&path)?;
        let identity = path_text(&resolved.git_dir)?;
        let branch = resolved
            .branch
            .as_deref()
            .and_then(|value| value.strip_prefix("refs/heads/"));
        let at = observed_at
            .format(&Rfc3339)
            .map_err(|error| AppError::Domain(error.to_string()))?;
        let checkout_id = self.store.write(|transaction| {
            let existing = transaction
                .query_row(
                    "SELECT id FROM checkouts
                     WHERE repository_id = ?1 AND git_worktree_identity = ?2",
                    params![repository_id.to_string(), identity],
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
                     availability, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'available', ?6)
                 ON CONFLICT(repository_id, git_worktree_identity) DO UPDATE SET
                     branch = excluded.branch,
                     head = excluded.head,
                     availability = 'available'",
                params![
                    checkout_id.to_string(),
                    repository_id.to_string(),
                    identity,
                    branch,
                    resolved.head_oid,
                    at,
                ],
            )?;
            let current = transaction
                .query_row(
                    "SELECT id, path FROM checkout_paths
                     WHERE checkout_id = ?1 AND observed_until IS NULL",
                    [checkout_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if current
                .as_ref()
                .is_none_or(|(_, current)| !paths_equal(Path::new(current), &resolved.path))
            {
                if let Some((path_id, _)) = current {
                    transaction.execute(
                        "UPDATE checkout_paths SET observed_until = ?2 WHERE id = ?1",
                        params![path_id, at],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        CheckoutPathId::generate().to_string(),
                        checkout_id.to_string(),
                        path_text(&resolved.path)?,
                        at,
                    ],
                )?;
            }
            Ok(checkout_id)
        })?;
        Ok(ManagedCheckout {
            checkout_id,
            repository_id,
            path: resolved.path,
            title,
        })
    }

    pub fn override_work_item_checkout(
        &mut self,
        work_item_id: WorkItemId,
        checkout_id: CheckoutId,
        observed_at: OffsetDateTime,
    ) -> Result<ManagedCheckout, AppError> {
        let checkout = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT checkout.repository_id, path.path, item.title
                     FROM work_items item
                     JOIN work_item_repositories target ON target.work_item_id = item.id
                     JOIN checkouts checkout
                       ON checkout.repository_id = target.repository_id
                      AND checkout.id = ?2
                      AND checkout.availability = 'available'
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     WHERE item.id = ?1",
                    params![work_item_id.to_string(), checkout_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let (repository_id, path, title) = checkout.ok_or(AppError::ResumeRepositoryMismatch)?;
        let repository_id = parse_id::<RepositoryId>(&repository_id)?;
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO work_item_checkout_overrides (
                     work_item_id, repository_id, checkout_id, assigned_at
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(work_item_id, repository_id) DO UPDATE SET
                     checkout_id = excluded.checkout_id,
                     assigned_at = excluded.assigned_at",
                params![
                    work_item_id.to_string(),
                    repository_id.to_string(),
                    checkout_id.to_string(),
                    observed_at.unix_timestamp_nanos().to_string(),
                ],
            )?;
            Ok(())
        })?;
        Ok(ManagedCheckout {
            checkout_id,
            repository_id,
            path: PathBuf::from(path),
            title,
        })
    }

    pub fn managed_session_checkout(
        &self,
        session_id: ConversationId,
    ) -> Result<ManagedCheckout, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT checkout.id, checkout.repository_id, path.path, session.native_id
                     FROM managed_sessions managed
                     JOIN checkouts checkout
                       ON checkout.id = managed.checkout_id
                      AND checkout.availability = 'available'
                     JOIN checkout_paths path
                       ON path.checkout_id = checkout.id AND path.observed_until IS NULL
                     JOIN native_sessions session ON session.id = managed.session_id
                     WHERE managed.session_id = ?1
                     ORDER BY managed.managed_from DESC LIMIT 1",
                    [session_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            let (checkout_id, repository_id, path, title) =
                row.ok_or(AppError::ResumeCheckoutRequired)?;
            Ok(ManagedCheckout {
                checkout_id: parse_id(&checkout_id)?,
                repository_id: parse_id(&repository_id)?,
                path: PathBuf::from(path),
                title,
            })
        })
    }

    pub fn managed_session_target(
        &self,
        session_id: ConversationId,
    ) -> Result<ManagedSessionTarget, AppError> {
        let checkout = self.managed_session_checkout(session_id)?;
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT association.epic_id, association.feature_id,
                            association.work_item_id, managed.role,
                            session.provider, session.native_id,
                            profile.schema_version, profile.model, profile.effort, profile.source
                     FROM native_sessions session
                     JOIN native_session_associations association
                       ON association.session_id = session.id
                      AND association.associated_until IS NULL
                     JOIN managed_sessions managed ON managed.session_id = session.id
                     LEFT JOIN launch_profiles profile ON profile.id = managed.profile_id
                     WHERE session.id = ?1
                     ORDER BY managed.managed_from DESC LIMIT 1",
                    [session_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, Option<u32>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, Option<String>>(8)?,
                            row.get::<_, Option<String>>(9)?,
                        ))
                    },
                )
                .optional()?;
            let (
                epic_id,
                feature_id,
                work_item_id,
                role,
                tool,
                native_id,
                profile_schema,
                model,
                effort,
                profile_source,
            ) = row.ok_or(AppError::ConversationNotFound)?;
            let tool = parse_tool(&tool)?;
            let role = parse_session_role(&role)?;
            let profile = match (profile_schema, model, effort, profile_source) {
                (Some(schema_version), Some(model), Some(effort), Some(source)) => LaunchProfile {
                    schema_version,
                    tool,
                    model: Some(model),
                    effort: Some(parse_reasoning_effort(&effort)?),
                    role,
                    source: parse_profile_source(&source)?,
                },
                _ => LaunchProfile::legacy_unknown(tool, role),
            };
            Ok(ManagedSessionTarget {
                session_id,
                owner: parse_hierarchy_owner(epic_id, feature_id, work_item_id)?,
                role,
                tool,
                native_id,
                profile,
                checkout,
            })
        })
    }

    pub fn initialise_workspace(
        &mut self,
        command: InitialiseWorkspace,
    ) -> Result<WorkspaceSnapshot, AppError> {
        validate_title(&command.title, "workspace")?;
        if let Some(existing) = self.workspace_id_by_slug(&command.slug)? {
            let snapshot = self.snapshot(existing)?;
            let expected = command
                .planning_store_path
                .canonicalize()
                .map_err(|source| AppError::PlanningStoreIo {
                    operation: "resolving the requested store",
                    path: command.planning_store_path.clone(),
                    source,
                })?;
            let actual = snapshot
                .repositories
                .iter()
                .find(|repository| repository.id == snapshot.workspace.planning_store_repository_id)
                .and_then(|repository| {
                    repository
                        .paths
                        .iter()
                        .find(|path| path.superseded_at.is_none())
                })
                .map(|path| path.path.clone())
                .ok_or_else(|| AppError::Domain("planning store has no current path".to_owned()))?;
            if snapshot.workspace.title != command.title || !paths_equal(&actual, &expected) {
                return Err(AppError::IdempotencyConflict);
            }
            return Ok(snapshot);
        }

        let planning_store = PlanningStore::create_or_link(&command.planning_store_path)?;
        let config_path = PlanningStore::workspace_config_path(&command.slug);
        if !planning_store.root().join(&config_path).exists() {
            planning_store.initialise_workspace(&command.slug, &command.title)?;
        }
        if planning_store.head().is_err()
            || git_path_is_changed(planning_store.root(), &config_path)?
        {
            planning_store.commit_paths(
                [config_path.as_path()],
                &format!("Initialise {} workspace", command.title),
            )?;
        }
        let workspace_id = WorkspaceId::generate();
        let repository_id = RepositoryId::generate();
        let repository_path_id = RepositoryPathId::generate();
        let root = planning_store.root().to_path_buf();
        let git_directory = planning_store.git_directory()?;
        let now = now_text();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO workspaces (
                     id, slug, title, planning_store_repository_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    workspace_id.to_string(),
                    command.slug.as_str(),
                    command.title,
                    repository_id.to_string(),
                    now,
                ],
            )?;
            transaction.execute(
                "INSERT INTO repositories (
                     id, workspace_id, slug, title, git_common_directory, default_branch,
                     is_planning_store, created_at
                 ) VALUES (?1, ?2, 'planning-store', 'Planning store', ?3, 'main', 1, ?4)",
                params![
                    repository_id.to_string(),
                    workspace_id.to_string(),
                    path_text(&git_directory)?,
                    now,
                ],
            )?;
            transaction.execute(
                "INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (?1, ?2, ?3, ?4, NULL)",
                params![
                    repository_path_id.to_string(),
                    repository_id.to_string(),
                    path_text(&root)?,
                    now,
                ],
            )?;
            Ok(())
        })?;
        self.snapshot(workspace_id)
    }

    pub fn register_repository(
        &mut self,
        command: RegisterRepository,
    ) -> Result<Repository, AppError> {
        validate_title(&command.title, "repository")?;
        let git = GitCli;
        let resolved = git.resolve(&command.path)?;
        let discovery = git.discover(&command.path)?;
        let path = resolved.path;
        let common_directory = discovery.common_dir;
        let existing = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT id, workspace_id FROM repositories WHERE git_common_directory = ?1",
                    [path_text(&common_directory)?],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(Into::into)
        })?;
        let repository_id = match existing.as_ref() {
            Some((id, workspace_id)) => {
                if parse_id::<WorkspaceId>(workspace_id)? != command.workspace_id {
                    return Err(AppError::IdempotencyConflict);
                }
                parse_id::<RepositoryId>(id)?
            }
            None => RepositoryId::generate(),
        };
        let now = now_text();
        let branch = resolved
            .branch
            .as_deref()
            .and_then(|value| value.strip_prefix("refs/heads/"))
            .map(str::to_owned);
        self.store.write(|transaction| {
            if existing.is_none() {
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory, default_branch,
                         is_planning_store, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
                    params![
                        repository_id.to_string(),
                        command.workspace_id.to_string(),
                        command.slug.as_str(),
                        command.title,
                        path_text(&common_directory)?,
                        branch,
                        now,
                    ],
                )?;
            }
            let current: Option<(String, String)> = transaction
                .query_row(
                    "SELECT id, path FROM repository_paths
                     WHERE repository_id = ?1 AND observed_until IS NULL",
                    [repository_id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if current
                .as_ref()
                .is_none_or(|(_, current_path)| !paths_equal(Path::new(current_path), &path))
            {
                if let Some((path_id, _)) = current {
                    transaction.execute(
                        "UPDATE repository_paths SET observed_until = ?2 WHERE id = ?1",
                        params![path_id, now],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO repository_paths (
                         id, repository_id, path, observed_from, observed_until
                     ) VALUES (?1, ?2, ?3, ?4, NULL)",
                    params![
                        RepositoryPathId::generate().to_string(),
                        repository_id.to_string(),
                        path_text(&path)?,
                        now,
                    ],
                )?;
            }
            for remote in &discovery.remotes {
                transaction.execute(
                    "INSERT OR IGNORE INTO repository_remotes (
                         repository_id, name, url, observed_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![repository_id.to_string(), remote.name, remote.url, now],
                )?;
            }
            Ok(())
        })?;
        self.repository(repository_id)
    }

    pub fn create_epic(&mut self, command: CreateEpic) -> Result<Epic, AppError> {
        validate_title(&command.title, "Epic")?;
        if let Some(existing) = self.epic_id_by_slug(command.workspace_id, &command.slug)? {
            let epic = self.epic(existing)?;
            if epic.title != command.title {
                return Err(AppError::IdempotencyConflict);
            }
            return Ok(epic);
        }
        let (workspace_slug, planning_repository_id, planning_store_path) =
            self.workspace_planning_store(command.workspace_id)?;
        let planning_store = PlanningStore::create_or_link(&planning_store_path)?;
        let relative_path = PlanningStore::epic_path(&workspace_slug, &command.slug);
        let body = if command.body.trim().is_empty() {
            epic_template(&command.title)
        } else {
            command.body
        };
        let stored = if planning_store.root().join(&relative_path).is_file() {
            let existing = planning_store.read_document(&relative_path)?;
            if existing.front_matter.kind != workboard_core::DocumentKind::Epic
                || existing.front_matter.key != command.slug.as_str()
            {
                return Err(AppError::PlanningDocumentConcurrentEdit(
                    planning_store.root().join(&relative_path),
                ));
            }
            if git_path_is_changed(planning_store.root(), &relative_path)? {
                let commit = planning_store.commit_paths(
                    [relative_path.as_path()],
                    &format!("Create {} epic", command.title),
                )?;
                StoredDocument {
                    observed_commit: Some(commit),
                    ..existing
                }
            } else {
                existing
            }
        } else {
            let front_matter = DocumentFrontMatter {
                id: DocumentId::generate(),
                kind: workboard_core::DocumentKind::Epic,
                key: command.slug.to_string(),
                status: None,
                repositories: self.code_repository_slugs(command.workspace_id)?,
            };
            planning_store.publish_new(
                &relative_path,
                &front_matter,
                &body,
                &format!("Create {} epic", command.title),
            )?
        };
        let epic_id = EpicId::generate();
        let now = now_text();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    epic_id.to_string(),
                    command.workspace_id.to_string(),
                    command.slug.as_str(),
                    command.title,
                    now,
                ],
            )?;
            transaction.execute(
                "INSERT INTO documents (
                     id, repository_id, epic_id, kind, relative_path, content_hash,
                     observed_commit, observed_at
                 ) VALUES (?1, ?2, ?3, 'epic', ?4, ?5, ?6, ?7)",
                params![
                    stored.front_matter.id.to_string(),
                    planning_repository_id.to_string(),
                    epic_id.to_string(),
                    path_text(&stored.relative_path)?,
                    stored.content_hash,
                    stored.observed_commit,
                    now,
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
                    now,
                ],
            )?;
            Ok(())
        })?;
        self.epic(epic_id)
    }

    pub fn workspace_ids(&self) -> Result<Vec<WorkspaceId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare("SELECT id FROM workspaces ORDER BY slug")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.iter().map(|id| parse_id(id)).collect()
        })
    }

    pub fn sole_workspace_id(&self) -> Result<WorkspaceId, AppError> {
        let ids = self.workspace_ids()?;
        match ids.as_slice() {
            [id] => Ok(*id),
            [] => Err(AppError::Domain(
                "no Workspace is configured; run workboard init".to_owned(),
            )),
            _ => Err(AppError::Domain(
                "more than one Workspace exists; select one explicitly".to_owned(),
            )),
        }
    }

    pub fn snapshot(&self, workspace_id: WorkspaceId) -> Result<WorkspaceSnapshot, AppError> {
        let workspace = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT slug, title, planning_store_repository_id
                     FROM workspaces WHERE id = ?1",
                    [workspace_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let (slug, title, planning_store_repository_id) =
            workspace.ok_or_else(|| AppError::Domain("Workspace does not exist".to_owned()))?;
        let workspace = Workspace {
            id: workspace_id,
            slug: parse_slug(slug)?,
            title,
            planning_store_repository_id: parse_id(&planning_store_repository_id)?,
        };
        let repository_ids = self.store.read(|connection| {
            let mut statement = connection
                .prepare("SELECT id FROM repositories WHERE workspace_id = ?1 ORDER BY slug")?;
            statement
                .query_map([workspace_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        let repositories = repository_ids
            .iter()
            .map(|id| self.repository(parse_id(id)?))
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(WorkspaceSnapshot {
            workspace,
            repositories,
            epics: self.epics(workspace_id)?,
            features: self.features(workspace_id)?,
            work_items: self.work_items(workspace_id)?,
            documents: self.documents(workspace_id)?,
            checkouts: self.checkouts(workspace_id)?,
            effective_checkouts: self.effective_checkouts(workspace_id)?,
            sessions: self.native_sessions(workspace_id)?,
            associations: self.native_session_associations(workspace_id)?,
        })
    }

    pub fn backup_database(&self, destination: &Path) -> Result<StorageHealth, AppError> {
        self.store.backup(destination)
    }

    pub fn export_planning_store(
        &self,
        workspace_id: WorkspaceId,
        destination: &Path,
    ) -> Result<(), AppError> {
        let (_, _, path) = self.workspace_planning_store(workspace_id)?;
        PlanningStore::create_or_link(&path)?.export(destination)
    }

    fn workspace_id_by_slug(&self, slug: &Slug) -> Result<Option<WorkspaceId>, AppError> {
        self.store.read(|connection| {
            let id = connection
                .query_row(
                    "SELECT id FROM workspaces WHERE slug = ?1",
                    [slug.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            id.map(|value| parse_id(&value)).transpose()
        })
    }

    fn epic_id_by_slug(
        &self,
        workspace_id: WorkspaceId,
        slug: &Slug,
    ) -> Result<Option<EpicId>, AppError> {
        self.store.read(|connection| {
            let id = connection
                .query_row(
                    "SELECT id FROM epics WHERE workspace_id = ?1 AND slug = ?2",
                    params![workspace_id.to_string(), slug.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            id.map(|value| parse_id(&value)).transpose()
        })
    }

    pub(crate) fn workspace_planning_store(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(Slug, RepositoryId, PathBuf), AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT workspace.slug, repository.id, path.path
                     FROM workspaces workspace
                     JOIN repositories repository
                       ON repository.id = workspace.planning_store_repository_id
                     JOIN repository_paths path
                       ON path.repository_id = repository.id AND path.observed_until IS NULL
                     WHERE workspace.id = ?1",
                    [workspace_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let (slug, repository_id, path) = row.ok_or_else(|| {
                AppError::Domain("Workspace planning store is unavailable".to_owned())
            })?;
            Ok((
                parse_slug(slug)?,
                parse_id(&repository_id)?,
                PathBuf::from(path),
            ))
        })
    }

    pub(crate) fn code_repository_slugs(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<Slug>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT slug FROM repositories
                 WHERE workspace_id = ?1 AND is_planning_store = 0 ORDER BY slug",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter().map(parse_slug).collect()
        })
    }

    fn repository(&self, repository_id: RepositoryId) -> Result<Repository, AppError> {
        let row = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT workspace_id, slug, title, git_common_directory, default_branch
                     FROM repositories WHERE id = ?1",
                    [repository_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(Into::into)
        })?;
        let (workspace_id, slug, title, common_directory, default_branch) =
            row.ok_or_else(|| AppError::Domain("Repository does not exist".to_owned()))?;
        let paths = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, path, observed_from, observed_until
                 FROM repository_paths WHERE repository_id = ?1 ORDER BY observed_from",
            )?;
            let rows = statement
                .query_map([repository_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, path, observed_at, superseded_at)| {
                    Ok(RepositoryPath {
                        id: parse_id(&id)?,
                        path: PathBuf::from(path),
                        observed_at: parse_time(&observed_at)?,
                        superseded_at: superseded_at.as_deref().map(parse_time).transpose()?,
                    })
                })
                .collect()
        })?;
        let remotes = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT name, url FROM repository_remotes
                 WHERE repository_id = ?1 ORDER BY name, url",
            )?;
            statement
                .query_map([repository_id.to_string()], |row| {
                    Ok(RepositoryRemote {
                        name: row.get(0)?,
                        url: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        Ok(Repository {
            id: repository_id,
            workspace_id: parse_id(&workspace_id)?,
            slug: parse_slug(slug)?,
            title,
            git_common_directory: PathBuf::from(common_directory),
            default_branch,
            remotes,
            paths,
        })
    }

    fn epic(&self, epic_id: EpicId) -> Result<Epic, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT epic.workspace_id, epic.slug, epic.title, document.id
                     FROM epics epic
                     JOIN documents document ON document.epic_id = epic.id
                     WHERE epic.id = ?1",
                    [epic_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            let (workspace_id, slug, title, document_id) = row.ok_or_else(|| {
                AppError::Domain("Epic or its document does not exist".to_owned())
            })?;
            Ok(Epic {
                id: epic_id,
                workspace_id: parse_id(&workspace_id)?,
                slug: parse_slug(slug)?,
                title,
                document_id: parse_id(&document_id)?,
            })
        })
    }

    fn epics(&self, workspace_id: WorkspaceId) -> Result<Vec<Epic>, AppError> {
        let ids = self.store.read(|connection| {
            let mut statement =
                connection.prepare("SELECT id FROM epics WHERE workspace_id = ?1 ORDER BY slug")?;
            statement
                .query_map([workspace_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        ids.iter().map(|id| self.epic(parse_id(id)?)).collect()
    }

    fn features(&self, workspace_id: WorkspaceId) -> Result<Vec<Feature>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT feature.id, feature.epic_id, feature.slug, feature.title,
                        feature.workflow_state, document.id
                 FROM features feature
                 JOIN epics epic ON epic.id = feature.epic_id
                 LEFT JOIN documents document ON document.feature_id = feature.id
                 WHERE epic.workspace_id = ?1 ORDER BY epic.slug, feature.slug",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, epic_id, slug, title, state, document_id)| {
                    Ok(Feature {
                        id: parse_id(&id)?,
                        epic_id: parse_id(&epic_id)?,
                        slug: parse_slug(slug)?,
                        title,
                        document_id: document_id.as_deref().map(parse_id).transpose()?,
                        state: parse_workflow_state(&state)?,
                    })
                })
                .collect()
        })
    }

    fn work_items(&self, workspace_id: WorkspaceId) -> Result<Vec<WorkItem>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT item.id, item.feature_id, item.key, item.slug, item.title, item.status,
                        document.id
                 FROM work_items item
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 JOIN documents document ON document.work_item_id = item.id
                 WHERE epic.workspace_id = ?1 ORDER BY epic.slug, feature.slug, item.key",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, feature_id, key, slug, title, status, document_id)| {
                    let id = parse_id::<WorkItemId>(&id)?;
                    let mut repository_statement = connection.prepare(
                        "SELECT repository_id FROM work_item_repositories
                         WHERE work_item_id = ?1 ORDER BY repository_id",
                    )?;
                    let repository_ids = repository_statement
                        .query_map([id.to_string()], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?
                        .iter()
                        .map(|value| parse_id(value))
                        .collect::<Result<Vec<_>, AppError>>()?;
                    Ok(WorkItem {
                        id,
                        feature_id: parse_id(&feature_id)?,
                        key: WorkItemKey::new(key)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                        slug: parse_slug(slug)?,
                        title,
                        status: parse_work_item_status(&status)?,
                        document_id: parse_id(&document_id)?,
                        repository_ids,
                    })
                })
                .collect()
        })
    }

    fn documents(&self, workspace_id: WorkspaceId) -> Result<Vec<MarkdownDocument>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT document.id, document.repository_id, document.epic_id,
                        document.feature_id, document.work_item_id, document.relative_path,
                        document.content_hash, document.observed_commit
                 FROM documents document
                 WHERE document.epic_id IN (SELECT id FROM epics WHERE workspace_id = ?1)
                    OR document.feature_id IN (
                        SELECT feature.id FROM features feature
                        JOIN epics epic ON epic.id = feature.epic_id
                        WHERE epic.workspace_id = ?1
                    )
                    OR document.work_item_id IN (
                        SELECT item.id FROM work_items item
                        JOIN features feature ON feature.id = item.feature_id
                        JOIN epics epic ON epic.id = feature.epic_id
                        WHERE epic.workspace_id = ?1
                    )
                 ORDER BY document.relative_path",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(id, repository_id, epic_id, feature_id, work_item_id, path, hash, commit)| {
                        let owner = match (epic_id, feature_id, work_item_id) {
                            (Some(id), None, None) => HierarchyOwner::Epic(parse_id(&id)?),
                            (None, Some(id), None) => HierarchyOwner::Feature(parse_id(&id)?),
                            (None, None, Some(id)) => HierarchyOwner::WorkItem(parse_id(&id)?),
                            _ => {
                                return Err(AppError::Domain(
                                    "document owner is invalid".to_owned(),
                                ));
                            }
                        };
                        Ok(MarkdownDocument {
                            id: parse_id(&id)?,
                            owner,
                            repository_id: parse_id(&repository_id)?,
                            relative_path: PathBuf::from(path),
                            content_hash: hash,
                            observed_commit: commit,
                        })
                    },
                )
                .collect()
        })
    }

    fn checkouts(&self, workspace_id: WorkspaceId) -> Result<Vec<Checkout>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT checkout.id, checkout.repository_id, checkout.git_worktree_identity,
                        checkout.branch, checkout.head, checkout.availability,
                        checkout.replaces_checkout_id
                 FROM checkouts checkout
                 JOIN repositories repository ON repository.id = checkout.repository_id
                 WHERE repository.workspace_id = ?1 ORDER BY checkout.created_at",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(id, repository_id, identity, branch, head, availability, replaces)| {
                        let id = parse_id::<CheckoutId>(&id)?;
                        let mut path_statement = connection.prepare(
                            "SELECT id, path, observed_from, observed_until
                         FROM checkout_paths WHERE checkout_id = ?1 ORDER BY observed_from",
                        )?;
                        let path_rows = path_statement
                            .query_map([id.to_string()], |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?,
                                    row.get::<_, Option<String>>(3)?,
                                ))
                            })?
                            .collect::<Result<Vec<_>, _>>()?;
                        let paths = path_rows
                            .into_iter()
                            .map(|(path_id, path, observed_from, observed_until)| {
                                Ok(CheckoutPathInterval {
                                    id: parse_id::<CheckoutPathId>(&path_id)?,
                                    checkout_id: id,
                                    path: PathBuf::from(path),
                                    observed_from: parse_time(&observed_from)?,
                                    observed_until: observed_until
                                        .as_deref()
                                        .map(parse_time)
                                        .transpose()?,
                                })
                            })
                            .collect::<Result<Vec<_>, AppError>>()?;
                        Ok(Checkout {
                            id,
                            repository_id: parse_id(&repository_id)?,
                            git_worktree_identity: identity,
                            branch,
                            head,
                            availability: parse_checkout_availability(&availability)?,
                            replaces_checkout_id: replaces.as_deref().map(parse_id).transpose()?,
                            paths,
                        })
                    },
                )
                .collect()
        })
    }

    fn effective_checkouts(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<EffectiveCheckout>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT item.feature_id, effective.work_item_id, effective.repository_id,
                        effective.checkout_id, effective.inherited
                 FROM effective_work_item_checkouts effective
                 JOIN work_items item ON item.id = effective.work_item_id
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE epic.workspace_id = ?1
                 ORDER BY effective.work_item_id, effective.repository_id",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(feature_id, work_item_id, repository_id, checkout_id, inherited)| {
                        Ok(EffectiveCheckout {
                            feature_id: parse_id(&feature_id)?,
                            work_item_id: Some(parse_id(&work_item_id)?),
                            repository_id: parse_id(&repository_id)?,
                            checkout_id: parse_id(&checkout_id)?,
                            inherited: inherited != 0,
                        })
                    },
                )
                .collect()
        })
    }

    fn native_sessions(&self, workspace_id: WorkspaceId) -> Result<Vec<NativeSession>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT session.id, session.provider, session.native_id, session.discovered_at
                 FROM native_sessions session
                 WHERE EXISTS (
                     SELECT 1 FROM native_session_associations association
                     LEFT JOIN epics direct_epic ON direct_epic.id = association.epic_id
                     LEFT JOIN features direct_feature ON direct_feature.id = association.feature_id
                     LEFT JOIN epics feature_epic ON feature_epic.id = direct_feature.epic_id
                     LEFT JOIN work_items direct_item ON direct_item.id = association.work_item_id
                     LEFT JOIN features item_feature ON item_feature.id = direct_item.feature_id
                     LEFT JOIN epics item_epic ON item_epic.id = item_feature.epic_id
                     WHERE association.session_id = session.id
                       AND COALESCE(
                           direct_epic.workspace_id,
                           feature_epic.workspace_id,
                           item_epic.workspace_id
                       ) = ?1
                 )
                 ORDER BY session.discovered_at, session.id",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, provider, native_id, discovered_at)| {
                    Ok(NativeSession {
                        id: parse_id::<ConversationId>(&id)?,
                        native: ConversationRef::new(parse_tool(&provider)?, native_id)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                        discovered_at: parse_time(&discovered_at)?,
                    })
                })
                .collect()
        })
    }

    fn native_session_associations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<NativeSessionAssociation>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT association.id, association.session_id, association.epic_id,
                        association.feature_id, association.work_item_id, association.role,
                        association.associated_from, association.associated_until
                 FROM native_session_associations association
                 LEFT JOIN epics direct_epic ON direct_epic.id = association.epic_id
                 LEFT JOIN features direct_feature ON direct_feature.id = association.feature_id
                 LEFT JOIN epics feature_epic ON feature_epic.id = direct_feature.epic_id
                 LEFT JOIN work_items direct_item ON direct_item.id = association.work_item_id
                 LEFT JOIN features item_feature ON item_feature.id = direct_item.feature_id
                 LEFT JOIN epics item_epic ON item_epic.id = item_feature.epic_id
                 WHERE COALESCE(
                     direct_epic.workspace_id,
                     feature_epic.workspace_id,
                     item_epic.workspace_id
                 ) = ?1
                 ORDER BY association.associated_from, association.id",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(id, session_id, epic_id, feature_id, work_item_id, role, from, until)| {
                        let owner = match (epic_id, feature_id, work_item_id) {
                            (Some(id), None, None) => HierarchyOwner::Epic(parse_id(&id)?),
                            (None, Some(id), None) => HierarchyOwner::Feature(parse_id(&id)?),
                            (None, None, Some(id)) => HierarchyOwner::WorkItem(parse_id(&id)?),
                            _ => {
                                return Err(AppError::Domain(
                                    "native session association owner is invalid".to_owned(),
                                ));
                            }
                        };
                        Ok(NativeSessionAssociation {
                            id: parse_id::<AssociationIntervalId>(&id)?,
                            session_id: parse_id::<ConversationId>(&session_id)?,
                            owner,
                            role: parse_session_role(&role)?,
                            associated_from: parse_time(&from)?,
                            associated_until: until.as_deref().map(parse_time).transpose()?,
                        })
                    },
                )
                .collect()
        })
    }
}

fn validate_title(title: &str, kind: &str) -> Result<(), AppError> {
    if title.trim().is_empty() {
        return Err(AppError::Domain(format!("{kind} title cannot be blank")));
    }
    Ok(())
}

fn missing_assigned_entity(kind: &str) -> AppError {
    AppError::External {
        code: format!("assigned_{kind}_missing"),
        message: format!("the authenticated {kind} is missing from its workspace"),
    }
}

fn epic_template(title: &str) -> String {
    format!("# {title}\n\n## Outcome\n\n## Ordering\n\n## Dependencies\n\n## Feature candidates\n")
}

fn now_text() -> String {
    OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()
}

fn parse_time(value: &str) -> Result<OffsetDateTime, AppError> {
    if let Ok(nanoseconds) = value.parse::<i128>() {
        return OffsetDateTime::from_unix_timestamp_nanos(nanoseconds)
            .map_err(|error| AppError::Domain(error.to_string()));
    }
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| AppError::Domain(error.to_string()))
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

fn parse_slug(value: String) -> Result<Slug, AppError> {
    Slug::new(value).map_err(|error| AppError::Domain(error.to_string()))
}

fn parse_work_item_status(value: &str) -> Result<WorkItemStatus, AppError> {
    match value {
        "backlog" => Ok(WorkItemStatus::Backlog),
        "ready" => Ok(WorkItemStatus::Ready),
        "in_progress" => Ok(WorkItemStatus::InProgress),
        "blocked" => Ok(WorkItemStatus::Blocked),
        "review" => Ok(WorkItemStatus::Review),
        "done" => Ok(WorkItemStatus::Done),
        "cancelled" => Ok(WorkItemStatus::Cancelled),
        _ => Err(AppError::Domain(format!(
            "unknown Work-item status: {value}"
        ))),
    }
}

fn parse_workflow_state(value: &str) -> Result<WorkflowState, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
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

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(AppError::Domain(format!(
            "unknown native provider: {value}"
        ))),
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn session_role_name(role: ManagedSessionRole) -> Result<String, AppError> {
    serde_json::to_value(role)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Domain("managed session role has no wire name".to_owned()))
}

fn parse_session_role(value: &str) -> Result<ManagedSessionRole, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_profile_source(value: &str) -> Result<LaunchProfileSource, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_hierarchy_owner(
    epic_id: Option<String>,
    feature_id: Option<String>,
    work_item_id: Option<String>,
) -> Result<HierarchyOwner, AppError> {
    match (epic_id, feature_id, work_item_id) {
        (Some(id), None, None) => Ok(HierarchyOwner::Epic(parse_id(&id)?)),
        (None, Some(id), None) => Ok(HierarchyOwner::Feature(parse_id(&id)?)),
        (None, None, Some(id)) => Ok(HierarchyOwner::WorkItem(parse_id(&id)?)),
        _ => Err(AppError::Domain(
            "native session association owner is invalid".to_owned(),
        )),
    }
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

fn git_path_is_changed(root: &Path, path: &Path) -> Result<bool, AppError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "--"])
        .arg(path)
        .output()
        .map_err(AppError::GitIo)?;
    if !output.status.success() {
        return Err(AppError::PlanningGit {
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(!output.stdout.is_empty())
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
    use std::process::Command;

    use rusqlite::params;
    use tempfile::TempDir;
    use workboard_core::{
        AssociationIntervalId, ConversationId, DocumentId, FeatureId, Slug, WorkItemId,
    };

    use super::{CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication};

    #[test]
    fn initialisation_and_epic_publication_round_trip_as_one_snapshot() {
        let directory = TempDir::new().expect("temporary directory");
        let planning_path = directory.path().join("planning");
        let planning_store = crate::planning_store::PlanningStore::create_or_link(&planning_path)
            .expect("create planning store");
        for arguments in [
            ["config", "user.name", "Workboard Test"],
            ["config", "user.email", "workboard@example.invalid"],
        ] {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(planning_store.root())
                    .args(arguments)
                    .status()
                    .expect("configure Git")
                    .success()
            );
        }
        let mut application = WorkboardApplication::open(directory.path().join("workboard.sqlite"))
            .expect("open application");
        let snapshot = application
            .initialise_workspace(InitialiseWorkspace {
                slug: Slug::new("concertable").expect("workspace slug"),
                title: "Concertable".to_owned(),
                planning_store_path: planning_path.clone(),
            })
            .expect("initialise workspace");
        let suggested = application
            .preferred_launch_profile(
                snapshot.workspace.id,
                workboard_core::Tool::Claude,
                workboard_core::ManagedSessionRole::WorkItemExecution,
            )
            .expect("suggested launch profile");
        assert_eq!(suggested.model.as_deref(), Some("sonnet"));
        let preferred = workboard_core::LaunchProfile::new(
            workboard_core::Tool::Claude,
            "opus",
            workboard_core::ReasoningEffort::Xhigh,
            workboard_core::ManagedSessionRole::WorkItemExecution,
            workboard_core::LaunchProfileSource::ExplicitOverride,
        )
        .expect("preferred launch profile");
        application
            .remember_launch_profile(
                snapshot.workspace.id,
                &preferred,
                time::OffsetDateTime::now_utc(),
            )
            .expect("remember launch profile");
        let remembered = application
            .preferred_launch_profile(
                snapshot.workspace.id,
                workboard_core::Tool::Claude,
                workboard_core::ManagedSessionRole::WorkItemExecution,
            )
            .expect("remembered launch profile");
        assert_eq!(remembered.model.as_deref(), Some("opus"));
        assert_eq!(
            remembered.source,
            workboard_core::LaunchProfileSource::Preference
        );
        let code_path = directory.path().join("code");
        fs::create_dir(&code_path).expect("create code repository");
        assert!(
            Command::new("git")
                .args(["init", "-b", "main"])
                .arg(&code_path)
                .status()
                .expect("initialise code repository")
                .success()
        );
        fs::write(code_path.join("README.md"), "# Code\n").expect("write code fixture");
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&code_path)
                .args(["add", "README.md"])
                .status()
                .expect("stage code fixture")
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&code_path)
                .args([
                    "-c",
                    "user.name=Workboard Test",
                    "-c",
                    "user.email=workboard@example.invalid",
                    "commit",
                    "-m",
                    "Initial commit",
                ])
                .status()
                .expect("commit code fixture")
                .success()
        );
        let repository = application
            .register_repository(RegisterRepository {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("concertable-code").expect("repository slug"),
                title: "Concertable code".to_owned(),
                path: code_path.clone(),
            })
            .expect("register repository");
        let repeated_repository = application
            .register_repository(RegisterRepository {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("concertable-code").expect("repository slug"),
                title: "Concertable code".to_owned(),
                path: code_path,
            })
            .expect("repeat repository registration");
        assert_eq!(repository.id, repeated_repository.id);
        let observed_at = time::OffsetDateTime::parse(
            "2026-08-28T13:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("checkout timestamp");
        let checkout = application
            .ensure_repository_checkout(repository.id, observed_at)
            .expect("register repository checkout");
        let repeated_checkout = application
            .ensure_repository_checkout(repository.id, observed_at)
            .expect("repeat repository checkout registration");
        assert_eq!(checkout, repeated_checkout);
        let epic = application
            .create_epic(CreateEpic {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("launch").expect("epic slug"),
                title: "Launch".to_owned(),
                body: "# Launch\n\n## Outcome\n\nShip Agent Workboard.".to_owned(),
            })
            .expect("create epic");
        let repeated = application
            .create_epic(CreateEpic {
                workspace_id: snapshot.workspace.id,
                slug: Slug::new("launch").expect("epic slug"),
                title: "Launch".to_owned(),
                body: "ignored on idempotent retry".to_owned(),
            })
            .expect("repeat create epic");
        let feature_id = FeatureId::generate();
        let feature_document_id = DocumentId::generate();
        let work_item_id = WorkItemId::generate();
        let work_item_document_id = DocumentId::generate();
        let checkout_id = checkout.checkout_id;
        let session_id = ConversationId::generate();
        let association_id = AssociationIntervalId::generate();
        let planning_repository_id = snapshot.workspace.planning_store_repository_id;
        let now = super::now_text();
        let hash = "b".repeat(64);
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO features (
                         id, epic_id, slug, title, workflow_state, created_at
                     ) VALUES (?1, ?2, 'availability', 'Availability', 'planned', ?3)",
                    params![feature_id.to_string(), epic.id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, feature_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'feature', ?4, ?5, NULL, ?6)",
                    params![
                        feature_document_id.to_string(),
                        planning_repository_id.to_string(),
                        feature_id.to_string(),
                        "workspaces/concertable/epics/launch/features/availability/FEATURE.md",
                        hash,
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (
                         id, feature_id, key, slug, title, status, created_at
                     ) VALUES (
                         ?1, ?2, 'launch/availability/api', 'api', 'Availability API',
                         'in_progress', ?3
                     )",
                    params![work_item_id.to_string(), feature_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![work_item_id.to_string(), repository.id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, work_item_id, kind, relative_path, content_hash,
                         observed_commit, observed_at
                     ) VALUES (?1, ?2, ?3, 'work_item', ?4, ?5, NULL, ?6)",
                    params![
                        work_item_document_id.to_string(),
                        planning_repository_id.to_string(),
                        work_item_id.to_string(),
                        "workspaces/concertable/epics/launch/features/availability/work-items/api.md",
                        hash,
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        feature_id.to_string(),
                        repository.id.to_string(),
                        checkout_id.to_string(),
                        now,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES (?1, 'codex', 'thread-1', ?2)",
                    params![session_id.to_string(), now],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_associations (
                         id, session_id, work_item_id, role, associated_from
                     ) VALUES (?1, ?2, ?3, 'work_item_execution', ?4)",
                    params![
                        association_id.to_string(),
                        session_id.to_string(),
                        work_item_id.to_string(),
                        now,
                    ],
                )?;
                Ok(())
            })
            .expect("seed snapshot projections");
        let snapshot = application
            .snapshot(snapshot.workspace.id)
            .expect("snapshot");

        assert_eq!(epic.id, repeated.id);
        assert_eq!(snapshot.repositories.len(), 2);
        assert_eq!(snapshot.epics, vec![epic]);
        assert_eq!(snapshot.features.len(), 1);
        assert_eq!(snapshot.work_items.len(), 1);
        assert_eq!(snapshot.documents.len(), 3);
        assert_eq!(snapshot.checkouts.len(), 1);
        assert_eq!(snapshot.checkouts[0].paths.len(), 1);
        assert_eq!(snapshot.effective_checkouts.len(), 1);
        assert!(snapshot.effective_checkouts[0].inherited);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.associations.len(), 1);
        assert!(
            planning_path
                .join("workspaces/concertable/epics/launch/EPIC.md")
                .is_file()
        );
        assert!(
            fs::metadata(application.database_path())
                .expect("database metadata")
                .len()
                > 0
        );
    }
}
