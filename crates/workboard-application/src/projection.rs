use rusqlite::OptionalExtension;
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::workspace::WorkboardApplication;

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

pub fn core_workspace_id(id: protocol::WorkspaceId) -> core::WorkspaceId {
    core::WorkspaceId::from_uuid(*id.as_uuid())
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
                queries: vec![protocol::ReadQueryCode::BoardSnapshot],
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
}
