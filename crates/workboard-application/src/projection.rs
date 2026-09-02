use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use time::OffsetDateTime;
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::workspace::WorkboardApplication;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartClientSession {
    pub workspace_id: core::WorkspaceId,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub request_id: protocol::RequestId,
    pub work_item_id: core::WorkItemId,
    pub repository_id: Option<core::RepositoryId>,
    pub tool: core::Tool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionCommandOutcome {
    pub detail: Box<protocol::WorkItemDetailProjection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProposalCommandOutcome {
    pub proposal: protocol::FeatureProposalProjection,
    pub partial_outcomes: Vec<protocol::PartialOutcome>,
}

fn proposal_event(
    workspace_id: core::WorkspaceId,
    revision: u64,
    feature_id: core::FeatureId,
    request_id: protocol::RequestId,
    partial_outcomes: Vec<protocol::PartialOutcome>,
) -> protocol::EventEnvelope {
    protocol::EventEnvelope {
        protocol_version: protocol::CURRENT_PROTOCOL_VERSION,
        event_version: 1,
        workspace_id: wire_workspace_id(workspace_id),
        sequence: revision,
        event_id: protocol::EventId::generate(),
        occurred_at: OffsetDateTime::now_utc(),
        owner: protocol::EntityRef::Feature(wire_feature_id(feature_id)),
        entity_revision: revision,
        kind: protocol::EventKind::ProposalChanged,
        payload: None,
        invalidation_scope: Some(protocol::InvalidationScope {
            queries: vec![
                protocol::ReadQueryCode::FeatureProposal,
                protocol::ReadQueryCode::ApprovalQueue,
                protocol::ReadQueryCode::Board,
                protocol::ReadQueryCode::Attention,
                protocol::ReadQueryCode::WorkspaceHierarchy,
            ],
            owners: vec![protocol::EntityRef::Feature(wire_feature_id(feature_id))],
        }),
        operation_correlation_id: request_id,
        partial_outcomes,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayResult {
    Events(Vec<protocol::EventEnvelope>),
    Resync(protocol::ResyncRequirement),
}

impl WorkboardApplication {
    pub fn client_workspaces(&self) -> Result<Vec<protocol::WorkspaceReference>, AppError> {
        self.workspace_ids()?
            .into_iter()
            .map(|id| {
                let snapshot = self.snapshot(id)?;
                Ok(protocol::WorkspaceReference {
                    id: wire_workspace_id(id),
                    slug: snapshot.workspace.slug.to_string(),
                    title: snapshot.workspace.title,
                })
            })
            .collect()
    }

    pub fn projection_revision(&self, workspace_id: core::WorkspaceId) -> Result<u64, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT revision FROM workspace_projection_revisions WHERE workspace_id = ?1",
                    [workspace_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .map(|revision| revision as u64)
                .ok_or_else(|| AppError::Domain("Workspace does not exist".to_owned()))
        })
    }

    pub fn oldest_replayable_sequence(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<u64, AppError> {
        self.store.read(|connection| {
            let oldest = connection.query_row(
                "SELECT MIN(sequence) FROM client_events WHERE workspace_id = ?1",
                [workspace_id.to_string()],
                |row| row.get::<_, Option<i64>>(0),
            )?;
            Ok(oldest
                .map(|sequence| sequence as u64)
                .unwrap_or(self.projection_revision(workspace_id)? + 1))
        })
    }

    pub fn client_workspace_summary(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<protocol::WorkspaceSummary, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        Ok(protocol::WorkspaceSummary {
            workspace: protocol::WorkspaceReference {
                id: wire_workspace_id(snapshot.workspace.id),
                slug: snapshot.workspace.slug.to_string(),
                title: snapshot.workspace.title,
            },
            repository_count: snapshot.repositories.len(),
            epic_count: snapshot.epics.len(),
            feature_count: snapshot.features.len(),
            work_item_count: snapshot.work_items.len(),
            session_count: snapshot.sessions.len(),
        })
    }

    pub fn client_hierarchy_children(
        &self,
        workspace_id: core::WorkspaceId,
        parent: protocol::HierarchyRef,
    ) -> Result<protocol::HierarchyChildren, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        let expected_workspace = protocol::HierarchyRef::Workspace(wire_workspace_id(workspace_id));
        let children = match parent {
            protocol::HierarchyRef::Workspace(_) if parent == expected_workspace => snapshot
                .repositories
                .iter()
                .map(|repository| {
                    protocol::HierarchyNode::Repository(protocol::RepositoryReference {
                        id: repository_id(repository.id),
                        workspace_id: wire_workspace_id(repository.workspace_id),
                        slug: repository.slug.to_string(),
                        title: repository.title.clone(),
                    })
                })
                .chain(snapshot.epics.iter().map(|epic| {
                    protocol::HierarchyNode::Epic(protocol::EpicReference {
                        id: epic_id(epic.id),
                        workspace_id: wire_workspace_id(epic.workspace_id),
                        slug: epic.slug.to_string(),
                        title: epic.title.clone(),
                    })
                }))
                .collect(),
            protocol::HierarchyRef::Workspace(_) => {
                return Err(AppError::Domain(
                    "hierarchy parent belongs to a different Workspace".to_owned(),
                ));
            }
            protocol::HierarchyRef::Epic(id) => {
                let id = core::EpicId::from_uuid(*id.as_uuid());
                if !snapshot.epics.iter().any(|epic| epic.id == id) {
                    return Err(AppError::Domain("Epic does not exist".to_owned()));
                }
                snapshot
                    .features
                    .iter()
                    .filter(|feature| feature.epic_id == id)
                    .map(|feature| {
                        protocol::HierarchyNode::Feature(protocol::FeatureReference {
                            id: feature_id(feature.id),
                            epic_id: epic_id(feature.epic_id),
                            slug: feature.slug.to_string(),
                            title: feature.title.clone(),
                        })
                    })
                    .collect()
            }
            protocol::HierarchyRef::Feature(id) => {
                let id = core::FeatureId::from_uuid(*id.as_uuid());
                if !snapshot.features.iter().any(|feature| feature.id == id) {
                    return Err(AppError::Domain("Feature does not exist".to_owned()));
                }
                snapshot
                    .work_items
                    .iter()
                    .filter(|item| item.feature_id == id)
                    .map(|item| {
                        protocol::HierarchyNode::WorkItem(protocol::WorkItemReference {
                            id: work_item_id(item.id),
                            feature_id: feature_id(item.feature_id),
                            key: item.key.to_string(),
                            slug: item.slug.to_string(),
                            title: item.title.clone(),
                        })
                    })
                    .collect()
            }
            protocol::HierarchyRef::WorkItem(id) => {
                let id = core::WorkItemId::from_uuid(*id.as_uuid());
                if !snapshot.work_items.iter().any(|item| item.id == id) {
                    return Err(AppError::Domain("Work item does not exist".to_owned()));
                }
                Vec::new()
            }
        };
        Ok(protocol::HierarchyChildren { parent, children })
    }

    pub fn client_board_snapshot(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<protocol::BoardSnapshot, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        Ok(map_snapshot(snapshot))
    }

    pub fn client_workspace_hierarchy(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<protocol::WorkspaceHierarchy, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        let mut feature_repositories =
            HashMap::<core::FeatureId, HashSet<core::RepositoryId>>::new();
        for item in &snapshot.work_items {
            feature_repositories
                .entry(item.feature_id)
                .or_default()
                .extend(item.repository_ids.iter().copied());
        }
        let feature_epics = snapshot
            .features
            .iter()
            .map(|feature| (feature.id, feature.epic_id))
            .collect::<HashMap<_, _>>();
        let mut epic_repositories = HashMap::<core::EpicId, HashSet<core::RepositoryId>>::new();
        for (feature_id, repositories) in &feature_repositories {
            if let Some(epic_id) = feature_epics.get(feature_id) {
                epic_repositories
                    .entry(*epic_id)
                    .or_default()
                    .extend(repositories);
            }
        }
        let focused_entity = snapshot
            .work_items
            .iter()
            .find(|item| item.status == core::WorkItemStatus::InProgress)
            .map(|item| protocol::EntityRef::WorkItem(work_item_id(item.id)))
            .or_else(|| {
                snapshot
                    .features
                    .iter()
                    .find(|feature| feature.state == core::WorkflowState::PlanningActive)
                    .map(|feature| protocol::EntityRef::Feature(feature_id(feature.id)))
            });
        let recent_entities = snapshot
            .work_items
            .iter()
            .rev()
            .take(8)
            .map(|item| protocol::EntityRef::WorkItem(work_item_id(item.id)))
            .collect();
        Ok(protocol::WorkspaceHierarchy {
            workspace: protocol::WorkspaceReference {
                id: wire_workspace_id(snapshot.workspace.id),
                slug: snapshot.workspace.slug.to_string(),
                title: snapshot.workspace.title,
            },
            repositories: snapshot
                .repositories
                .into_iter()
                .map(|repository| protocol::RepositoryReference {
                    id: repository_id(repository.id),
                    workspace_id: wire_workspace_id(repository.workspace_id),
                    slug: repository.slug.to_string(),
                    title: repository.title,
                })
                .collect(),
            epics: snapshot
                .epics
                .into_iter()
                .map(|epic| protocol::HierarchyEpic {
                    repository_ids: sorted_repository_ids(
                        epic_repositories.remove(&epic.id).unwrap_or_default(),
                    ),
                    epic: protocol::EpicReference {
                        id: epic_id(epic.id),
                        workspace_id: wire_workspace_id(epic.workspace_id),
                        slug: epic.slug.to_string(),
                        title: epic.title,
                    },
                })
                .collect(),
            features: map_hierarchy_features(snapshot.features, &mut feature_repositories),
            work_items: snapshot
                .work_items
                .into_iter()
                .map(|item| protocol::HierarchyWorkItem {
                    repository_ids: item.repository_ids.into_iter().map(repository_id).collect(),
                    status: work_item_status(item.status),
                    work_item: protocol::WorkItemReference {
                        id: work_item_id(item.id),
                        feature_id: feature_id(item.feature_id),
                        key: item.key.to_string(),
                        slug: item.slug.to_string(),
                        title: item.title,
                    },
                })
                .collect(),
            recent_entities,
            focused_entity,
        })
    }

    pub fn client_board_views(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<Vec<protocol::BoardViewDefinition>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare("SELECT id, workspace_id, title, filters_json, grouping_json, sort_json, density_json, revision FROM board_view_definitions WHERE workspace_id = ?1 ORDER BY title, id")?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?))
                })?
                .collect::<Result<Vec<BoardViewRow>, _>>()?;
            rows.into_iter().map(board_view_from_row).collect()
        })
    }

    pub fn client_board_view(
        &self,
        workspace_id: core::WorkspaceId,
        view_id: protocol::BoardViewId,
    ) -> Result<protocol::BoardViewDefinition, AppError> {
        self.store.read(|connection| {
            let row = connection
                .query_row(
                    "SELECT id, workspace_id, title, filters_json, grouping_json, sort_json, density_json, revision FROM board_view_definitions WHERE workspace_id = ?1 AND id = ?2",
                    params![workspace_id.to_string(), view_id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::Domain("Saved view does not exist".to_owned()))?;
            board_view_from_row(row)
        })
    }

    pub fn save_client_board_view(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        request_id: protocol::RequestId,
        definition: protocol::BoardViewDefinition,
    ) -> Result<protocol::BoardViewDefinition, AppError> {
        validate_board_view(workspace_id, &definition)?;
        let projected = self.store.write_projected(
            workspace_id,
            expected_revision,
            idempotency_key,
            "save_board_view",
            |transaction| {
                for repository_id in &definition.filters.repository_ids {
                    let belongs = transaction.query_row(
                        "SELECT EXISTS(SELECT 1 FROM repositories WHERE id = ?1 AND workspace_id = ?2)",
                        params![repository_id.to_string(), workspace_id.to_string()],
                        |row| row.get::<_, bool>(0),
                    )?;
                    if !belongs {
                        return Err(AppError::Domain("saved view repository filter is outside the Workspace".to_owned()));
                    }
                }
                let current = transaction
                    .query_row(
                        "SELECT revision FROM board_view_definitions WHERE id = ?1 AND workspace_id = ?2",
                        params![definition.id.to_string(), workspace_id.to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?;
                match current {
                    None if definition.revision == 0 => {}
                    Some(current) if current as u64 == definition.revision => {}
                    _ => {
                        return Err(AppError::External {
                            code: "stale_board_view_revision".to_owned(),
                            message: "the saved view revision is stale".to_owned(),
                        });
                    }
                }
                let mut saved = definition.clone();
                saved.revision += 1;
                transaction.execute(
                    "INSERT INTO board_view_definitions (id, workspace_id, title, filters_json, grouping_json, sort_json, density_json, revision) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO UPDATE SET title = excluded.title, filters_json = excluded.filters_json, grouping_json = excluded.grouping_json, sort_json = excluded.sort_json, density_json = excluded.density_json, revision = excluded.revision",
                    params![
                        saved.id.to_string(),
                        workspace_id.to_string(),
                        saved.title,
                        serde_json::to_string(&saved.filters)?,
                        serde_json::to_string(&saved.grouping)?,
                        serde_json::to_string(&saved.sort)?,
                        serde_json::to_string(&saved.density)?,
                        saved.revision as i64,
                    ],
                )?;
                Ok(saved)
            },
            |revision, saved| protocol::EventEnvelope {
                protocol_version: protocol::CURRENT_PROTOCOL_VERSION,
                event_version: 1,
                workspace_id: wire_workspace_id(workspace_id),
                sequence: revision,
                event_id: protocol::EventId::generate(),
                occurred_at: OffsetDateTime::now_utc(),
                owner: protocol::EntityRef::Workspace(wire_workspace_id(workspace_id)),
                entity_revision: saved.revision,
                kind: protocol::EventKind::BoardViewSaved,
                payload: Some(protocol::EventPayload::BoardViewSaved { view: saved.clone() }),
                invalidation_scope: Some(protocol::InvalidationScope {
                    queries: vec![protocol::ReadQueryCode::BoardViews, protocol::ReadQueryCode::BoardView],
                    owners: vec![protocol::EntityRef::Workspace(wire_workspace_id(workspace_id))],
                }),
                operation_correlation_id: request_id,
                partial_outcomes: Vec::new(),
            },
        )?;
        Ok(projected.value)
    }

    pub fn approve_client_feature(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        request_id: protocol::RequestId,
        feature_id: core::FeatureId,
    ) -> Result<ProposalCommandOutcome, AppError> {
        let approved_at = OffsetDateTime::now_utc();
        self.planning_workflows()
            .approve_proposal(feature_id, approved_at)?;
        let publication = self
            .planning_workflows()
            .publish_approved(feature_id, approved_at);
        let partial_outcomes = match &publication {
            Ok(_) => Vec::new(),
            Err(error) => vec![protocol::PartialOutcome {
                owner: Some(protocol::EntityRef::Feature(wire_feature_id(feature_id))),
                code: error.code().to_owned(),
                succeeded: false,
                message: error.to_string(),
                reconciliation_required: true,
                evidence: Vec::new(),
            }],
        };
        self.record_proposal_command(
            workspace_id,
            expected_revision,
            idempotency_key,
            "approve_feature",
            request_id,
            feature_id,
            partial_outcomes.clone(),
        )?;
        Ok(ProposalCommandOutcome {
            proposal: self.client_feature_proposal(workspace_id, feature_id)?,
            partial_outcomes,
        })
    }

    pub fn request_client_feature_revision(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        request_id: protocol::RequestId,
        feature_id: core::FeatureId,
        feedback: &str,
    ) -> Result<ProposalCommandOutcome, AppError> {
        self.planning_workflows().request_proposal_revision(
            feature_id,
            feedback,
            OffsetDateTime::now_utc(),
        )?;
        self.record_proposal_command(
            workspace_id,
            expected_revision,
            idempotency_key,
            "request_feature_revision",
            request_id,
            feature_id,
            Vec::new(),
        )?;
        Ok(ProposalCommandOutcome {
            proposal: self.client_feature_proposal(workspace_id, feature_id)?,
            partial_outcomes: Vec::new(),
        })
    }

    pub fn reject_client_feature(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        request_id: protocol::RequestId,
        feature_id: core::FeatureId,
        reason: &str,
    ) -> Result<ProposalCommandOutcome, AppError> {
        self.planning_workflows()
            .reject_proposal(feature_id, reason, OffsetDateTime::now_utc())?;
        self.record_proposal_command(
            workspace_id,
            expected_revision,
            idempotency_key,
            "reject_feature",
            request_id,
            feature_id,
            Vec::new(),
        )?;
        Ok(ProposalCommandOutcome {
            proposal: self.client_feature_proposal(workspace_id, feature_id)?,
            partial_outcomes: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn record_proposal_command(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        operation: &'static str,
        request_id: protocol::RequestId,
        feature_id: core::FeatureId,
        partial_outcomes: Vec<protocol::PartialOutcome>,
    ) -> Result<(), AppError> {
        self.store.write_projected(
            workspace_id,
            expected_revision,
            idempotency_key,
            operation,
            |_| Ok(()),
            |revision, ()| {
                proposal_event(
                    workspace_id,
                    revision,
                    feature_id,
                    request_id,
                    partial_outcomes.clone(),
                )
            },
        )?;
        Ok(())
    }

    pub fn start_client_session(
        &mut self,
        command: StartClientSession,
    ) -> Result<SessionCommandOutcome, AppError> {
        let StartClientSession {
            workspace_id,
            expected_revision,
            idempotency_key,
            request_id,
            work_item_id,
            repository_id,
            tool,
        } = command;
        let idempotency_key = idempotency_key.as_str();
        let now = OffsetDateTime::now_utc();
        let snapshot = self.snapshot(workspace_id)?;
        let work_item = snapshot
            .work_items
            .iter()
            .find(|item| item.id == work_item_id)
            .ok_or(AppError::WorkItemNotFound)?
            .clone();
        let repository_id = match repository_id {
            Some(requested) => {
                if !work_item.repository_ids.contains(&requested) {
                    return Err(AppError::WorkItemRepositoryMismatch);
                }
                requested
            }
            None => match work_item.repository_ids.as_slice() {
                [only] => *only,
                _ => {
                    return Err(AppError::CheckoutReconciliation {
                        code: "launch_repository_selection_required".to_owned(),
                        message: "the Work item targets multiple repositories; select the launch repository".to_owned(),
                    });
                }
            },
        };
        let repository_slug = snapshot
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .map(|repository| repository.slug.to_string())
            .ok_or_else(|| {
                AppError::Domain("the launch repository is not in this Workspace".to_owned())
            })?;
        let capability = crate::session_launch::CapabilityLaunchInputs::for_managed_launch(
            self.database_path().to_path_buf(),
            tool,
            &repository_slug,
        )?;
        let readiness = self.checkout_service().prepare_work_item(
            crate::checkout::PrepareWorkItemCheckout {
                work_item_id,
                repository_id,
                idempotency_key: format!("{idempotency_key}:checkout"),
                observed_at: now,
            },
        )?;
        let prepared =
            self.session_launch()
                .begin(crate::session_launch::BeginManagedSessionLaunch {
                    owner: core::HierarchyOwner::WorkItem(work_item_id),
                    role: core::ManagedSessionRole::WorkItemExecution,
                    tool,
                    mode: core::ManagedLaunchMode::New,
                    checkout_id: readiness.checkout_id,
                    working_directory: readiness.path,
                    title: work_item.title.clone(),
                    terminal_window: Some(format!("workboard-feature-{}", work_item.feature_id)),
                    terminal_executable: crate::native_launch::default_terminal_executable(),
                    native_executable: crate::native_launch::default_native_executable(tool),
                    idempotency_key: idempotency_key.to_owned(),
                    created_at: now,
                    expires_at: now + time::Duration::minutes(2),
                    resume_context: None,
                    profile: core::LaunchProfile::suggested(
                        tool,
                        core::ManagedSessionRole::WorkItemExecution,
                    ),
                    initial_prompt: Some(crate::workflow_operations::work_item_bootstrap_prompt(
                        work_item_id,
                    )),
                    capability,
                })?;
        self.session_launch()
            .execute(&prepared, &crate::native_launch::SystemLaunchExecutor)?;
        self.record_session_command(
            workspace_id,
            expected_revision,
            &format!("{idempotency_key}:launched"),
            "start_session",
            request_id,
            core::HierarchyOwner::WorkItem(work_item_id),
        )?;
        Ok(SessionCommandOutcome {
            detail: Box::new(self.client_work_item_detail(workspace_id, work_item_id)?),
        })
    }

    pub fn resume_client_session(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        request_id: protocol::RequestId,
        session_id: core::ConversationId,
    ) -> Result<SessionCommandOutcome, AppError> {
        let now = OffsetDateTime::now_utc();
        let target = self.managed_session_target(session_id)?;
        let core::HierarchyOwner::WorkItem(work_item_id) = target.owner else {
            return Err(AppError::ConversationNotResumable(
                "only Work-item sessions can be resumed from Desktop".to_owned(),
            ));
        };
        self.checkout_service()
            .reconcile_registered_checkout(target.checkout.checkout_id, now)?;
        let context = self.native_sources().resume_context(
            session_id,
            target.checkout.path.clone(),
            target.checkout.title.clone(),
        )?;
        let repository_slug = self
            .snapshot(workspace_id)?
            .repositories
            .iter()
            .find(|repository| repository.id == target.checkout.repository_id)
            .map(|repository| repository.slug.to_string())
            .ok_or_else(|| {
                AppError::Domain("the launch repository is not in this Workspace".to_owned())
            })?;
        let capability = crate::session_launch::CapabilityLaunchInputs::for_managed_launch(
            self.database_path().to_path_buf(),
            target.tool,
            &repository_slug,
        )?;
        let terminal_window = self
            .snapshot(workspace_id)?
            .work_items
            .iter()
            .find(|item| item.id == work_item_id)
            .map_or_else(
                || format!("workboard-work-item-{work_item_id}"),
                |item| format!("workboard-feature-{}", item.feature_id),
            );
        let prepared =
            self.session_launch()
                .begin(crate::session_launch::BeginManagedSessionLaunch {
                    owner: target.owner,
                    role: target.role,
                    tool: target.tool,
                    mode: core::ManagedLaunchMode::Resume(target.native_id),
                    checkout_id: target.checkout.checkout_id,
                    working_directory: target.checkout.path,
                    title: target.checkout.title,
                    terminal_window: Some(terminal_window),
                    terminal_executable: crate::native_launch::default_terminal_executable(),
                    native_executable: crate::native_launch::default_native_executable(target.tool),
                    idempotency_key: idempotency_key.to_owned(),
                    created_at: now,
                    expires_at: now + time::Duration::minutes(2),
                    resume_context: Some(context),
                    profile: core::LaunchProfile::suggested(target.tool, target.role),
                    initial_prompt: None,
                    capability,
                })?;
        self.session_launch()
            .execute(&prepared, &crate::native_launch::SystemLaunchExecutor)?;
        self.record_session_command(
            workspace_id,
            expected_revision,
            &format!("{idempotency_key}:resumed"),
            "resume_session",
            request_id,
            target.owner,
        )?;
        Ok(SessionCommandOutcome {
            detail: Box::new(self.client_work_item_detail(workspace_id, work_item_id)?),
        })
    }

    fn record_session_command(
        &mut self,
        workspace_id: core::WorkspaceId,
        expected_revision: u64,
        idempotency_key: &str,
        operation: &'static str,
        request_id: protocol::RequestId,
        owner: core::HierarchyOwner,
    ) -> Result<(), AppError> {
        let entity = match owner {
            core::HierarchyOwner::Epic(id) => {
                protocol::EntityRef::Epic(protocol::EpicId::from_uuid(*id.as_uuid()))
            }
            core::HierarchyOwner::Feature(id) => {
                protocol::EntityRef::Feature(protocol::FeatureId::from_uuid(*id.as_uuid()))
            }
            core::HierarchyOwner::WorkItem(id) => {
                protocol::EntityRef::WorkItem(protocol::WorkItemId::from_uuid(*id.as_uuid()))
            }
            core::HierarchyOwner::Workspace(id) => {
                protocol::EntityRef::Workspace(wire_workspace_id(id))
            }
        };
        self.store.write_projected(
            workspace_id,
            expected_revision,
            idempotency_key,
            operation,
            |_| Ok(()),
            |revision, ()| protocol::EventEnvelope {
                protocol_version: protocol::CURRENT_PROTOCOL_VERSION,
                event_version: 1,
                workspace_id: wire_workspace_id(workspace_id),
                sequence: revision,
                event_id: protocol::EventId::generate(),
                occurred_at: OffsetDateTime::now_utc(),
                owner: entity,
                entity_revision: revision,
                kind: protocol::EventKind::WorkItemChanged,
                payload: None,
                invalidation_scope: Some(protocol::InvalidationScope {
                    queries: vec![
                        protocol::ReadQueryCode::WorkItemDetail,
                        protocol::ReadQueryCode::SessionObservability,
                        protocol::ReadQueryCode::RecoveryPreview,
                        protocol::ReadQueryCode::Board,
                        protocol::ReadQueryCode::Attention,
                    ],
                    owners: vec![entity],
                }),
                operation_correlation_id: request_id,
                partial_outcomes: Vec::new(),
            },
        )?;
        Ok(())
    }

    pub fn replay_client_events(
        &self,
        workspace_id: core::WorkspaceId,
        daemon_instance_id: protocol::DaemonInstanceId,
        cursor: protocol::EventCursor,
        version: u32,
        limit: usize,
    ) -> Result<ReplayResult, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let oldest = self.oldest_replayable_sequence(workspace_id)?;
        let scope = || protocol::ResyncRequirement {
            reason: protocol::ResyncReason::Gap,
            workspace_id: wire_workspace_id(workspace_id),
            authoritative_revision: revision,
            oldest_replayable_sequence: oldest,
            required_queries: vec![
                protocol::ReadQueryCode::WorkspaceSummary,
                protocol::ReadQueryCode::HierarchyChildren,
                protocol::ReadQueryCode::WorkspaceHierarchy,
                protocol::ReadQueryCode::BoardViews,
                protocol::ReadQueryCode::BoardView,
                protocol::ReadQueryCode::Board,
                protocol::ReadQueryCode::Attention,
                protocol::ReadQueryCode::ApprovalQueue,
                protocol::ReadQueryCode::FeatureProposal,
                protocol::ReadQueryCode::RepositoryObservability,
                protocol::ReadQueryCode::CheckoutObservability,
                protocol::ReadQueryCode::SessionObservability,
                protocol::ReadQueryCode::RecoveryPreview,
                protocol::ReadQueryCode::BoardSnapshot,
            ],
        };
        if cursor.daemon_instance_id != daemon_instance_id {
            let mut requirement = scope();
            requirement.reason = protocol::ResyncReason::DaemonRestarted;
            return Ok(ReplayResult::Resync(requirement));
        }
        if cursor.sequence > revision {
            return Ok(ReplayResult::Resync(scope()));
        }
        if cursor.sequence + 1 < oldest {
            let mut requirement = scope();
            requirement.reason = protocol::ResyncReason::CursorExpired;
            return Ok(ReplayResult::Resync(requirement));
        }
        let limit = limit.min(protocol::MAX_REPLAY_EVENTS);
        let mut events = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT event_json FROM client_events
                 WHERE workspace_id = ?1 AND sequence > ?2
                 ORDER BY sequence LIMIT ?3",
            )?;
            let rows = statement
                .query_map(
                    rusqlite::params![
                        workspace_id.to_string(),
                        cursor.sequence as i64,
                        limit as i64
                    ],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|value| serde_json::from_str(&value).map_err(Into::into))
                .collect::<Result<Vec<protocol::EventEnvelope>, AppError>>()
        })?;
        for event in &mut events {
            event.protocol_version = version;
        }
        Ok(ReplayResult::Events(events))
    }
}

type BoardViewRow = (String, String, String, String, String, String, String, i64);

fn board_view_from_row(row: BoardViewRow) -> Result<protocol::BoardViewDefinition, AppError> {
    Ok(protocol::BoardViewDefinition {
        id: row
            .0
            .parse()
            .map_err(|error: uuid::Error| AppError::Domain(error.to_string()))?,
        workspace_id: row
            .1
            .parse()
            .map_err(|error: uuid::Error| AppError::Domain(error.to_string()))?,
        title: row.2,
        filters: serde_json::from_str(&row.3)?,
        grouping: serde_json::from_str(&row.4)?,
        sort: serde_json::from_str(&row.5)?,
        density: serde_json::from_str(&row.6)?,
        revision: row.7 as u64,
    })
}

fn validate_board_view(
    workspace_id: core::WorkspaceId,
    definition: &protocol::BoardViewDefinition,
) -> Result<(), AppError> {
    if definition.workspace_id != wire_workspace_id(workspace_id) {
        return Err(AppError::Domain(
            "saved view belongs to a different Workspace".to_owned(),
        ));
    }
    let title = definition.title.trim();
    if title.is_empty() || title.len() > 200 || title.chars().any(char::is_control) {
        return Err(AppError::Domain("saved view title is invalid".to_owned()));
    }
    if definition
        .filters
        .query
        .as_ref()
        .is_some_and(|query| query.len() > 200 || query.chars().any(char::is_control))
    {
        return Err(AppError::Domain("saved view query is invalid".to_owned()));
    }
    if definition.filters.repository_ids.len() > 100
        || definition.filters.statuses.len() > 7
        || definition.grouping.lanes.len() > 32
    {
        return Err(AppError::Domain(
            "saved view collection is too large".to_owned(),
        ));
    }
    let repository_ids = definition
        .filters
        .repository_ids
        .iter()
        .collect::<HashSet<_>>();
    if repository_ids.len() != definition.filters.repository_ids.len() {
        return Err(AppError::Domain(
            "saved view repository filters contain duplicates".to_owned(),
        ));
    }
    let lane_keys = definition
        .grouping
        .lanes
        .iter()
        .map(|lane| lane.key.as_str())
        .collect::<BTreeSet<_>>();
    if lane_keys.len() != definition.grouping.lanes.len()
        || definition
            .grouping
            .lanes
            .iter()
            .any(|lane| lane.key.trim().is_empty() || lane.title.trim().is_empty())
    {
        return Err(AppError::Domain("saved view lanes are invalid".to_owned()));
    }
    Ok(())
}

fn map_hierarchy_features(
    features: Vec<core::Feature>,
    feature_repositories: &mut HashMap<core::FeatureId, HashSet<core::RepositoryId>>,
) -> Vec<protocol::HierarchyFeature> {
    features
        .into_iter()
        .map(|feature| protocol::HierarchyFeature {
            repository_ids: sorted_repository_ids(
                feature_repositories.remove(&feature.id).unwrap_or_default(),
            ),
            feature: protocol::FeatureReference {
                id: feature_id(feature.id),
                epic_id: epic_id(feature.epic_id),
                slug: feature.slug.to_string(),
                title: feature.title,
            },
        })
        .collect()
}

pub fn core_workspace_id(id: protocol::WorkspaceId) -> core::WorkspaceId {
    core::WorkspaceId::from_uuid(*id.as_uuid())
}

fn sorted_repository_ids(ids: HashSet<core::RepositoryId>) -> Vec<protocol::RepositoryId> {
    let mut ids = ids.into_iter().map(repository_id).collect::<Vec<_>>();
    ids.sort_by_key(ToString::to_string);
    ids
}

fn map_snapshot(snapshot: core::WorkspaceSnapshot) -> protocol::BoardSnapshot {
    protocol::BoardSnapshot {
        workspace: protocol::WorkspaceProjection {
            id: wire_workspace_id(snapshot.workspace.id),
            slug: snapshot.workspace.slug.to_string(),
            title: snapshot.workspace.title,
            planning_store_repository_id: repository_id(
                snapshot.workspace.planning_store_repository_id,
            ),
        },
        repositories: snapshot
            .repositories
            .into_iter()
            .map(|repository| protocol::RepositoryProjection {
                id: repository_id(repository.id),
                workspace_id: wire_workspace_id(repository.workspace_id),
                slug: repository.slug.to_string(),
                title: repository.title,
                git_common_directory: repository.git_common_directory.to_string_lossy().into(),
                default_branch: repository.default_branch,
                remotes: repository
                    .remotes
                    .into_iter()
                    .map(|remote| protocol::RepositoryRemoteProjection {
                        name: remote.name,
                        url: remote.url,
                    })
                    .collect(),
                paths: repository
                    .paths
                    .into_iter()
                    .map(|path| protocol::RepositoryPathProjection {
                        id: protocol::RepositoryPathId::from_uuid(*path.id.as_uuid()),
                        path: path.path.to_string_lossy().into(),
                        observed_at: path.observed_at,
                        superseded_at: path.superseded_at,
                    })
                    .collect(),
            })
            .collect(),
        epics: snapshot
            .epics
            .into_iter()
            .map(|epic| protocol::EpicProjection {
                id: epic_id(epic.id),
                workspace_id: wire_workspace_id(epic.workspace_id),
                slug: epic.slug.to_string(),
                title: epic.title,
                document_id: document_id(epic.document_id),
            })
            .collect(),
        features: snapshot
            .features
            .into_iter()
            .map(|feature| protocol::FeatureProjection {
                id: feature_id(feature.id),
                epic_id: epic_id(feature.epic_id),
                slug: feature.slug.to_string(),
                title: feature.title,
                document_id: feature.document_id.map(document_id),
                state: workflow_state(feature.state),
            })
            .collect(),
        work_items: snapshot
            .work_items
            .into_iter()
            .map(|item| protocol::WorkItemProjection {
                id: work_item_id(item.id),
                feature_id: feature_id(item.feature_id),
                key: item.key.to_string(),
                slug: item.slug.to_string(),
                title: item.title,
                status: work_item_status(item.status),
                document_id: document_id(item.document_id),
                repository_ids: item.repository_ids.into_iter().map(repository_id).collect(),
            })
            .collect(),
        documents: snapshot
            .documents
            .into_iter()
            .map(|document| protocol::DocumentProjection {
                id: document_id(document.id),
                owner: owner(document.owner),
                repository_id: repository_id(document.repository_id),
                relative_path: document.relative_path.to_string_lossy().into(),
                content_hash: document.content_hash,
                observed_commit: document.observed_commit,
            })
            .collect(),
        checkouts: snapshot
            .checkouts
            .into_iter()
            .map(|checkout| protocol::CheckoutProjection {
                id: checkout_id(checkout.id),
                repository_id: repository_id(checkout.repository_id),
                git_worktree_identity: checkout.git_worktree_identity,
                branch: checkout.branch,
                head: checkout.head,
                availability: checkout_availability(checkout.availability),
                replaces_checkout_id: checkout.replaces_checkout_id.map(checkout_id),
                paths: checkout
                    .paths
                    .into_iter()
                    .map(|path| protocol::CheckoutPathProjection {
                        id: protocol::CheckoutPathId::from_uuid(*path.id.as_uuid()),
                        checkout_id: checkout_id(path.checkout_id),
                        path: path.path.to_string_lossy().into(),
                        observed_from: path.observed_from,
                        observed_until: path.observed_until,
                    })
                    .collect(),
            })
            .collect(),
        effective_checkouts: snapshot
            .effective_checkouts
            .into_iter()
            .map(|checkout| protocol::EffectiveCheckoutProjection {
                feature_id: feature_id(checkout.feature_id),
                work_item_id: checkout.work_item_id.map(work_item_id),
                repository_id: repository_id(checkout.repository_id),
                checkout_id: checkout_id(checkout.checkout_id),
                inherited: checkout.inherited,
            })
            .collect(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| protocol::SessionProjection {
                id: session_id(session.id),
                native: protocol::NativeSessionReference {
                    tool: provider(session.native.tool()),
                    native_id: session.native.native_id().to_owned(),
                },
                discovered_at: session.discovered_at,
            })
            .collect(),
        associations: snapshot
            .associations
            .into_iter()
            .map(|association| protocol::SessionAssociationProjection {
                id: protocol::AssociationId::from_uuid(*association.id.as_uuid()),
                session_id: session_id(association.session_id),
                owner: owner(association.owner),
                role: role(association.role),
                associated_from: association.associated_from,
                associated_until: association.associated_until,
            })
            .collect(),
    }
}

fn wire_feature_id(id: core::FeatureId) -> protocol::FeatureId {
    protocol::FeatureId::from_uuid(*id.as_uuid())
}

fn wire_workspace_id(id: core::WorkspaceId) -> protocol::WorkspaceId {
    protocol::WorkspaceId::from_uuid(*id.as_uuid())
}

fn repository_id(id: core::RepositoryId) -> protocol::RepositoryId {
    protocol::RepositoryId::from_uuid(*id.as_uuid())
}

fn epic_id(id: core::EpicId) -> protocol::EpicId {
    protocol::EpicId::from_uuid(*id.as_uuid())
}

fn feature_id(id: core::FeatureId) -> protocol::FeatureId {
    protocol::FeatureId::from_uuid(*id.as_uuid())
}

fn work_item_id(id: core::WorkItemId) -> protocol::WorkItemId {
    protocol::WorkItemId::from_uuid(*id.as_uuid())
}

fn session_id(id: core::ConversationId) -> protocol::SessionId {
    protocol::SessionId::from_uuid(*id.as_uuid())
}

fn checkout_id(id: core::CheckoutId) -> protocol::CheckoutId {
    protocol::CheckoutId::from_uuid(*id.as_uuid())
}

fn document_id(id: core::DocumentId) -> protocol::DocumentId {
    protocol::DocumentId::from_uuid(*id.as_uuid())
}

fn owner(owner: core::HierarchyOwner) -> protocol::OwnerProjection {
    match owner {
        core::HierarchyOwner::Epic(id) => protocol::OwnerProjection::Epic(epic_id(id)),
        core::HierarchyOwner::Feature(id) => protocol::OwnerProjection::Feature(feature_id(id)),
        core::HierarchyOwner::WorkItem(id) => protocol::OwnerProjection::WorkItem(work_item_id(id)),
        core::HierarchyOwner::Workspace(id) => {
            protocol::OwnerProjection::Workspace(wire_workspace_id(id))
        }
    }
}

fn provider(tool: core::Tool) -> protocol::Provider {
    match tool {
        core::Tool::Claude => protocol::Provider::Claude,
        core::Tool::Codex => protocol::Provider::Codex,
    }
}

fn role(role: core::ManagedSessionRole) -> protocol::ManagedSessionRole {
    match role {
        core::ManagedSessionRole::WorkspacePlanning => {
            protocol::ManagedSessionRole::WorkspacePlanning
        }
        core::ManagedSessionRole::EpicNavigation => protocol::ManagedSessionRole::EpicNavigation,
        core::ManagedSessionRole::FeaturePlanning => protocol::ManagedSessionRole::FeaturePlanning,
        core::ManagedSessionRole::WorkItemExecution => {
            protocol::ManagedSessionRole::WorkItemExecution
        }
        core::ManagedSessionRole::Debugging => protocol::ManagedSessionRole::Debugging,
        core::ManagedSessionRole::Review => protocol::ManagedSessionRole::Review,
    }
}

fn work_item_status(status: core::WorkItemStatus) -> protocol::WorkItemStatus {
    match status {
        core::WorkItemStatus::Backlog => protocol::WorkItemStatus::Backlog,
        core::WorkItemStatus::Ready => protocol::WorkItemStatus::Ready,
        core::WorkItemStatus::InProgress => protocol::WorkItemStatus::InProgress,
        core::WorkItemStatus::Blocked => protocol::WorkItemStatus::Blocked,
        core::WorkItemStatus::Review => protocol::WorkItemStatus::Review,
        core::WorkItemStatus::Done => protocol::WorkItemStatus::Done,
        core::WorkItemStatus::Cancelled => protocol::WorkItemStatus::Cancelled,
    }
}

fn checkout_availability(
    availability: core::CheckoutAvailability,
) -> protocol::CheckoutAvailability {
    match availability {
        core::CheckoutAvailability::Available => protocol::CheckoutAvailability::Available,
        core::CheckoutAvailability::Missing => protocol::CheckoutAvailability::Missing,
        core::CheckoutAvailability::Deleted => protocol::CheckoutAvailability::Deleted,
        core::CheckoutAvailability::Replaced => protocol::CheckoutAvailability::Replaced,
    }
}

fn workflow_state(state: core::WorkflowState) -> protocol::WorkflowState {
    match state {
        core::WorkflowState::Draft => protocol::WorkflowState::Draft,
        core::WorkflowState::WorktreePending => protocol::WorkflowState::WorktreePending,
        core::WorkflowState::PlanningLaunchPending => {
            protocol::WorkflowState::PlanningLaunchPending
        }
        core::WorkflowState::PlanningActive => protocol::WorkflowState::PlanningActive,
        core::WorkflowState::ProposalReady => protocol::WorkflowState::ProposalReady,
        core::WorkflowState::AwaitingApproval => protocol::WorkflowState::AwaitingApproval,
        core::WorkflowState::Publishing => protocol::WorkflowState::Publishing,
        core::WorkflowState::Planned => protocol::WorkflowState::Planned,
        core::WorkflowState::WorkItemLaunchPending => {
            protocol::WorkflowState::WorkItemLaunchPending
        }
        core::WorkflowState::WorkItemActive => protocol::WorkflowState::WorkItemActive,
        core::WorkflowState::ReconciliationRequired => {
            protocol::WorkflowState::ReconciliationRequired
        }
        core::WorkflowState::Blocked => protocol::WorkflowState::Blocked,
        core::WorkflowState::Paused => protocol::WorkflowState::Paused,
        core::WorkflowState::Completed => protocol::WorkflowState::Completed,
        core::WorkflowState::Cancelled => protocol::WorkflowState::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use serde_json::json;
    use tempfile::TempDir;
    use time::OffsetDateTime;

    use super::*;
    use crate::storage::SqliteStore;

    fn seed_workspace(store: &mut SqliteStore) -> core::WorkspaceId {
        let workspace_id = core::WorkspaceId::generate();
        let repository_id = core::RepositoryId::generate();
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
                     VALUES (?1, 'workspace', 'Workspace', ?2, '2026-08-30T00:00:00Z')",
                    params![workspace_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory, default_branch,
                         is_planning_store, created_at
                     ) VALUES (
                         ?1, ?2, 'planning', 'Planning', 'C:/planning/.git', 'main', 1,
                         '2026-08-30T00:00:00Z'
                     )",
                    params![repository_id.to_string(), workspace_id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed Workspace");
        workspace_id
    }

    fn event(
        workspace_id: core::WorkspaceId,
        revision: u64,
        correlation_id: protocol::RequestId,
        partial_outcomes: Vec<protocol::PartialOutcome>,
    ) -> protocol::EventEnvelope {
        let workspace_id = protocol::WorkspaceId::from_uuid(*workspace_id.as_uuid());
        protocol::EventEnvelope {
            protocol_version: protocol::CURRENT_PROTOCOL_VERSION,
            event_version: 1,
            workspace_id,
            sequence: revision,
            event_id: protocol::EventId::generate(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            owner: protocol::EntityRef::Workspace(workspace_id),
            entity_revision: revision,
            kind: protocol::EventKind::ProjectionChanged,
            payload: Some(protocol::EventPayload::ProjectionChanged {
                entity: protocol::EntityRef::Workspace(workspace_id),
            }),
            invalidation_scope: Some(protocol::InvalidationScope {
                queries: vec![
                    protocol::ReadQueryCode::Board,
                    protocol::ReadQueryCode::Attention,
                    protocol::ReadQueryCode::BoardSnapshot,
                ],
                owners: Vec::new(),
            }),
            operation_correlation_id: correlation_id,
            partial_outcomes,
        }
    }

    #[test]
    fn projected_write_is_atomic_idempotent_revision_checked_and_restart_replayable() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        let workspace_id = seed_workspace(&mut store);
        let correlation_id = protocol::RequestId::generate();
        let first = store
            .write_projected::<serde_json::Value>(
                workspace_id,
                0,
                "rename-1",
                "rename_workspace",
                |transaction| {
                    transaction.execute(
                        "UPDATE workspaces SET title = 'Renamed' WHERE id = ?1",
                        [workspace_id.to_string()],
                    )?;
                    Ok(json!({ "title": "Renamed" }))
                },
                |revision, _| event(workspace_id, revision, correlation_id, Vec::new()),
            )
            .expect("projected write");
        assert_eq!(first.event.sequence, 1);
        assert!(!first.replayed);
        let retried = store
            .write_projected::<serde_json::Value>(
                workspace_id,
                0,
                "rename-1",
                "rename_workspace",
                |_| panic!("idempotent retry must not execute"),
                |_, _| panic!("idempotent retry must not publish"),
            )
            .expect("idempotent retry");
        assert!(retried.replayed);
        assert_eq!(retried.value, first.value);
        let stale = store
            .write_projected::<serde_json::Value>(
                workspace_id,
                0,
                "rename-2",
                "rename_workspace",
                |_| Ok(json!(null)),
                |revision, _| event(workspace_id, revision, correlation_id, Vec::new()),
            )
            .expect_err("stale revision");
        assert_eq!(stale.code(), "stale_revision");
        let interrupted = store.write_projected::<serde_json::Value>(
            workspace_id,
            1,
            "rename-3",
            "rename_workspace",
            |transaction| {
                transaction.execute(
                    "UPDATE workspaces SET title = 'Never committed' WHERE id = ?1",
                    [workspace_id.to_string()],
                )?;
                Err(AppError::InjectedStorageInterruption)
            },
            |revision, _| event(workspace_id, revision, correlation_id, Vec::new()),
        );
        assert_eq!(
            interrupted.expect_err("rollback").code(),
            "injected_storage_interruption"
        );
        drop(store);
        let application = WorkboardApplication::open(&path).expect("restart application");
        assert_eq!(
            application
                .projection_revision(workspace_id)
                .expect("revision"),
            1
        );
        let cursor = protocol::EventCursor {
            daemon_instance_id: protocol::DaemonInstanceId::generate(),
            sequence: 0,
        };
        let replay = application
            .replay_client_events(
                workspace_id,
                cursor.daemon_instance_id,
                cursor,
                protocol::CURRENT_PROTOCOL_VERSION,
                10,
            )
            .expect("replay");
        let ReplayResult::Events(events) = replay else {
            panic!("events expected");
        };
        assert_eq!(events, vec![first.event]);
        assert_eq!(
            application
                .client_workspace_summary(workspace_id)
                .expect("summary")
                .workspace
                .title,
            "Renamed"
        );
    }

    #[test]
    fn partial_outcomes_remain_failures_in_the_committed_event() {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let workspace_id = seed_workspace(&mut store);
        let partial = protocol::PartialOutcome {
            owner: Some(protocol::EntityRef::Workspace(
                protocol::WorkspaceId::from_uuid(*workspace_id.as_uuid()),
            )),
            code: "planning_commit_failed".to_owned(),
            succeeded: false,
            message: "planning publication requires reconciliation".to_owned(),
            reconciliation_required: true,
            evidence: Vec::new(),
        };
        let write = store
            .write_projected(
                workspace_id,
                0,
                "partial-1",
                "partial_operation",
                |_| Ok(json!({ "status": "partial" })),
                |revision, _| {
                    event(
                        workspace_id,
                        revision,
                        protocol::RequestId::generate(),
                        vec![partial.clone()],
                    )
                },
            )
            .expect("partial write");
        assert_eq!(write.event.partial_outcomes, vec![partial]);
        assert!(!write.event.partial_outcomes[0].succeeded);
        assert!(write.event.partial_outcomes[0].reconciliation_required);
    }

    #[test]
    fn hierarchy_keeps_one_workspace_and_stable_cross_repository_identity_at_scale() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        let workspace_id = seed_workspace(&mut store);
        let epic_id = core::EpicId::generate();
        let feature_id = core::FeatureId::generate();
        let work_item_id = core::WorkItemId::generate();
        let repository_ids = (1..100)
            .map(|_| core::RepositoryId::generate())
            .collect::<Vec<_>>();
        store
            .write(|transaction| {
                for (index, repository_id) in repository_ids.iter().enumerate() {
                    transaction.execute(
                        "INSERT INTO repositories (
                             id, workspace_id, slug, title, git_common_directory, default_branch,
                             is_planning_store, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'main', 0, '2026-08-30T00:00:00Z')",
                        params![
                            repository_id.to_string(),
                            workspace_id.to_string(),
                            format!("service-{index:03}"),
                            format!("Service {index:03}"),
                            format!("C:/services/{index:03}/.git"),
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'platform', 'Platform', '2026-08-30T00:00:00Z')",
                    params![epic_id.to_string(), workspace_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'delivery', 'Delivery', 'planning_active', '2026-08-30T00:00:00Z')",
                    params![feature_id.to_string(), epic_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (id, feature_id, key, slug, title, status, created_at)
                     VALUES (?1, ?2, 'platform/delivery/desktop-ui', 'desktop-ui', 'Desktop UI', 'in_progress', '2026-08-30T00:00:00Z')",
                    params![work_item_id.to_string(), feature_id.to_string()],
                )?;
                for repository_id in &repository_ids {
                    transaction.execute(
                        "INSERT INTO work_item_repositories (work_item_id, repository_id)
                         VALUES (?1, ?2)",
                        params![work_item_id.to_string(), repository_id.to_string()],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, feature_id, work_item_id, kind,
                         relative_path, content_hash, observed_commit, observed_at
                     ) VALUES (
                         ?1, (SELECT planning_store_repository_id FROM workspaces WHERE id = ?2),
                         ?3, NULL, NULL, 'epic', 'plans/platform/EPIC.md',
                         '0000000000000000000000000000000000000000000000000000000000000000',
                         NULL, '2026-08-30T00:00:00Z'
                     )",
                    params![
                        core::DocumentId::generate().to_string(),
                        workspace_id.to_string(),
                        epic_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, feature_id, work_item_id, kind,
                         relative_path, content_hash, observed_commit, observed_at
                     ) VALUES (
                         ?1, (SELECT planning_store_repository_id FROM workspaces WHERE id = ?2),
                         NULL, ?3, NULL, 'feature', 'plans/platform/delivery/FEATURE.md',
                         '0000000000000000000000000000000000000000000000000000000000000000',
                         NULL, '2026-08-30T00:00:00Z'
                     )",
                    params![
                        core::DocumentId::generate().to_string(),
                        workspace_id.to_string(),
                        feature_id.to_string(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO documents (
                         id, repository_id, epic_id, feature_id, work_item_id, kind,
                         relative_path, content_hash, observed_commit, observed_at
                     ) VALUES (
                         ?1, (SELECT planning_store_repository_id FROM workspaces WHERE id = ?2),
                         NULL, NULL, ?3, 'work_item',
                         'plans/platform/delivery/WI-4.md',
                         '0000000000000000000000000000000000000000000000000000000000000000',
                         NULL, '2026-08-30T00:00:00Z'
                     )",
                    params![
                        core::DocumentId::generate().to_string(),
                        workspace_id.to_string(),
                        work_item_id.to_string(),
                    ],
                )?;
                Ok(())
            })
            .expect("seed hierarchy");
        drop(store);

        let application = WorkboardApplication::open(&path).expect("open application");
        let hierarchy = application
            .client_workspace_hierarchy(workspace_id)
            .expect("Workspace hierarchy");
        assert_eq!(hierarchy.repositories.len(), 100);
        assert_eq!(hierarchy.epics.len(), 1);
        assert_eq!(hierarchy.features.len(), 1);
        assert_eq!(hierarchy.work_items.len(), 1);
        assert_eq!(hierarchy.features[0].repository_ids.len(), 99);
        assert_eq!(hierarchy.epics[0].repository_ids.len(), 99);
        assert_eq!(
            hierarchy.focused_entity,
            Some(protocol::EntityRef::WorkItem(
                protocol::WorkItemId::from_uuid(*work_item_id.as_uuid())
            ))
        );
        let board = application
            .client_board(
                workspace_id,
                protocol::BoardQuery {
                    cursor: None,
                    limit: 100,
                    query: Some("Desktop".to_owned()),
                    repository_ids: vec![protocol::RepositoryId::from_uuid(
                        *repository_ids[0].as_uuid(),
                    )],
                    feature_ids: Vec::new(),
                    statuses: vec![protocol::WorkItemStatus::InProgress],
                    lane_keys: vec!["in_progress".to_owned()],
                    sort: protocol::BoardViewSort {
                        field: protocol::BoardViewSortField::Key,
                        direction: protocol::BoardViewSortDirection::Ascending,
                    },
                },
            )
            .expect("board projection");
        assert_eq!(board.cards.len(), 1);
        assert_eq!(
            board.cards[0].work_item.id.as_uuid(),
            work_item_id.as_uuid()
        );
        assert_eq!(board.cards[0].repositories.len(), 99);
        assert_eq!(
            board.cards[0].dependency_readiness,
            protocol::DependencyReadiness::Ready
        );
        let attention = application
            .client_attention(
                workspace_id,
                protocol::AttentionQuery {
                    cursor: None,
                    limit: 100,
                    repository_ids: Vec::new(),
                    reason_codes: Vec::new(),
                },
            )
            .expect("attention projection");
        assert_eq!(attention.entries.len(), 1);
        assert_eq!(
            attention.entries[0].reasons[0].code,
            protocol::AttentionReasonCode::CheckpointDue
        );
        let workspace_count = application
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM workspaces", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("Workspace count");
        assert_eq!(workspace_count, 1);
    }

    #[test]
    fn saved_views_are_workspace_owned_revisioned_and_idempotent() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        let workspace_id = seed_workspace(&mut store);
        let core_repository_id = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT id FROM repositories WHERE workspace_id = ?1",
                        [workspace_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .map(|id| id.parse::<core::RepositoryId>().expect("repository ID"))
                    .map_err(Into::into)
            })
            .expect("repository");
        drop(store);
        let mut application = WorkboardApplication::open(&path).expect("open application");
        let view_id = protocol::BoardViewId::generate();
        let definition = protocol::BoardViewDefinition {
            id: view_id,
            workspace_id: wire_workspace_id(workspace_id),
            title: "Focused delivery".to_owned(),
            filters: protocol::BoardViewFilters {
                query: Some("desktop".to_owned()),
                repository_ids: vec![repository_id(core_repository_id)],
                statuses: vec![protocol::WorkItemStatus::InProgress],
            },
            grouping: protocol::BoardViewGrouping {
                kind: protocol::BoardViewGroupingKind::Repository,
                lanes: vec![protocol::BoardViewLaneDefinition {
                    key: "active".to_owned(),
                    title: "Active".to_owned(),
                }],
            },
            sort: protocol::BoardViewSort {
                field: protocol::BoardViewSortField::Key,
                direction: protocol::BoardViewSortDirection::Ascending,
            },
            density: protocol::BoardViewDensity::Compact,
            revision: 0,
        };
        let request_id = protocol::RequestId::generate();
        let saved = application
            .save_client_board_view(
                workspace_id,
                0,
                "save-view-1",
                request_id,
                definition.clone(),
            )
            .expect("save view");
        assert_eq!(saved.revision, 1);
        let replayed = application
            .save_client_board_view(workspace_id, 0, "save-view-1", request_id, definition)
            .expect("replay save");
        assert_eq!(replayed, saved);
        assert_eq!(
            application
                .client_board_view(workspace_id, view_id)
                .expect("saved view"),
            saved
        );
        assert_eq!(
            application
                .client_board_views(workspace_id)
                .expect("saved views"),
            vec![saved.clone()]
        );
        let event = application
            .replay_client_events(
                workspace_id,
                protocol::DaemonInstanceId::generate(),
                protocol::EventCursor {
                    daemon_instance_id: protocol::DaemonInstanceId::generate(),
                    sequence: 0,
                },
                protocol::CURRENT_PROTOCOL_VERSION,
                10,
            )
            .expect("event replay");
        let ReplayResult::Resync(_) = event else {
            panic!("a foreign daemon cursor must require resync");
        };
        let stored_event = application
            .store
            .read(|connection| {
                connection.query_row(
                    "SELECT event_json FROM client_events WHERE workspace_id = ?1 AND sequence = 1",
                    [workspace_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .expect("stored event");
        let stored_event =
            serde_json::from_str::<protocol::EventEnvelope>(&stored_event).expect("event JSON");
        assert_eq!(stored_event.kind, protocol::EventKind::BoardViewSaved);
        assert_eq!(
            stored_event
                .invalidation_scope
                .expect("invalidation")
                .queries,
            vec![
                protocol::ReadQueryCode::BoardViews,
                protocol::ReadQueryCode::BoardView
            ]
        );
        let workspace_count = application
            .store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM workspaces", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(Into::into)
            })
            .expect("Workspace count");
        assert_eq!(workspace_count, 1);
    }
}
