use rusqlite::{OptionalExtension, params};
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::workspace::WorkboardApplication;

struct DetailRow {
    work_item: core::WorkItemId,
    feature: core::FeatureId,
    epic: core::EpicId,
    key: String,
    slug: String,
    title: String,
    status: core::WorkItemStatus,
    feature_slug: String,
    feature_title: String,
    workflow_state: core::WorkflowState,
    outcome_design_summary: String,
    content_hash: String,
    content_revision: u64,
}

impl WorkboardApplication {
    pub fn client_work_item_detail(
        &self,
        workspace_id: core::WorkspaceId,
        work_item_id: core::WorkItemId,
    ) -> Result<protocol::WorkItemDetailProjection, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let row = self.work_item_detail_row(workspace_id, work_item_id)?;
        let snapshot = self.snapshot(workspace_id)?;
        let item = snapshot
            .work_items
            .iter()
            .find(|item| item.id == work_item_id)
            .ok_or_else(|| AppError::Domain("Work item does not exist".to_owned()))?;
        let mut repositories = item
            .repository_ids
            .iter()
            .filter_map(|id| {
                snapshot
                    .repositories
                    .iter()
                    .find(|repository| repository.id == *id)
            })
            .map(|repository| protocol::RepositoryReference {
                id: protocol::RepositoryId::from_uuid(*repository.id.as_uuid()),
                workspace_id: protocol::WorkspaceId::from_uuid(*workspace_id.as_uuid()),
                slug: repository.slug.to_string(),
                title: repository.title.clone(),
            })
            .collect::<Vec<_>>();
        repositories.sort_by(|left, right| left.slug.cmp(&right.slug));

        let prerequisites = self.work_item_prerequisites(workspace_id, work_item_id)?;
        let blockers = prerequisites
            .iter()
            .filter(|dependency| !is_complete(dependency.status))
            .map(|dependency| protocol::WorkItemBlockerProjection {
                code: "dependency_incomplete".to_owned(),
                message: format!(
                    "{} is {}.",
                    dependency.title,
                    status_label(dependency.status)
                ),
                prerequisite: Some(work_item_reference(dependency)),
            })
            .collect::<Vec<_>>();
        let dependency_readiness = dependency_readiness(item.status, blockers.is_empty());
        let checkpoint_history = self.work_item_checkpoints(work_item_id)?;
        let next_action =
            checkpoint_history
                .last()
                .map(|checkpoint| protocol::WorkItemNextActionProjection {
                    kind: checkpoint.next_action,
                    recorded_at: checkpoint.recorded_at.clone(),
                });
        let review_delivery_state = match next_action.as_ref().map(|next| next.kind) {
            Some(protocol::WorkItemNextActionKind::Review) => {
                protocol::ReviewDeliveryState::ReviewRequested
            }
            Some(protocol::WorkItemNextActionKind::Delivery) => {
                protocol::ReviewDeliveryState::DeliveryRequested
            }
            _ => protocol::ReviewDeliveryState::NotRequested,
        };

        let checkout_ids = self.work_item_checkout_ids(work_item_id)?;
        let checkouts = checkout_ids
            .into_iter()
            .map(|id| self.client_checkout_observability(workspace_id, id))
            .collect::<Result<Vec<_>, _>>()?;
        let session_ids = self.work_item_session_ids(work_item_id)?;
        let sessions = session_ids
            .into_iter()
            .map(|id| self.client_session_observability(workspace_id, id))
            .collect::<Result<Vec<_>, _>>()?;
        let session_actions = session_action_inputs(&sessions);

        let structured_evidence = protocol::ClassifiedEvidence {
            state: protocol::EvidenceState::NotLoaded,
            code: "structured_checkpoint_unavailable".to_owned(),
            message: "Structured checkpoint evidence is unavailable while the accepted checkpoint contract remains opaque.".to_owned(),
            observed_at: None,
        };
        let mut diagnostics = Vec::new();
        if row.workflow_state == core::WorkflowState::ReconciliationRequired {
            diagnostics.push(protocol::Diagnostic {
                code: "work_item_reconciliation_required".to_owned(),
                severity: protocol::ErrorSeverity::Error,
                message: "This Work item requires authoritative reconciliation outside Desktop."
                    .to_owned(),
                owner: Some(protocol::EntityRef::WorkItem(
                    protocol::WorkItemId::from_uuid(*work_item_id.as_uuid()),
                )),
            });
        }

        Ok(protocol::WorkItemDetailProjection {
            work_item: protocol::WorkItemReference {
                id: protocol::WorkItemId::from_uuid(*row.work_item.as_uuid()),
                feature_id: protocol::FeatureId::from_uuid(*row.feature.as_uuid()),
                key: row.key,
                slug: row.slug,
                title: row.title,
            },
            feature: protocol::FeatureReference {
                id: protocol::FeatureId::from_uuid(*row.feature.as_uuid()),
                epic_id: protocol::EpicId::from_uuid(*row.epic.as_uuid()),
                slug: row.feature_slug,
                title: row.feature_title,
            },
            outcome_design_summary: row.outcome_design_summary,
            current_state: protocol::DurableWorkItemSection {
                entries: Vec::new(),
                evidence: structured_evidence.clone(),
            },
            dependency_readiness,
            blockers,
            decisions: protocol::DurableWorkItemSection {
                entries: Vec::new(),
                evidence: structured_evidence.clone(),
            },
            verification: protocol::DurableWorkItemSection {
                entries: Vec::new(),
                evidence: structured_evidence,
            },
            next_action,
            review_delivery_state,
            workflow_state: workflow_state(row.workflow_state),
            status: work_item_status(row.status),
            repositories,
            checkouts,
            revision,
            content_revision: row.content_revision,
            content_hash: row.content_hash,
            checkpoint_history,
            sessions,
            diagnostics,
            available_actions: work_item_actions(revision, &session_actions),
        })
    }

    fn work_item_detail_row(
        &self,
        workspace_id: core::WorkspaceId,
        work_item_id: core::WorkItemId,
    ) -> Result<DetailRow, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT item.id, item.feature_id, feature.epic_id, item.key, item.slug,
                            item.title, item.status, feature.slug, feature.title,
                            COALESCE(run.current_state, feature.workflow_state),
                            COALESCE(json_extract(proposed.value, '$.proposal.body'), ''),
                            document.content_hash,
                            COALESCE((SELECT MAX(revision) FROM document_revisions WHERE document_id = document.id), 1)
                     FROM work_items item
                     JOIN features feature ON feature.id = item.feature_id
                     JOIN epics epic ON epic.id = feature.epic_id
                     JOIN documents document ON document.work_item_id = item.id
                     LEFT JOIN workflow_runs run ON run.id = (
                         SELECT candidate.id FROM workflow_runs candidate
                         WHERE candidate.work_item_id = item.id
                         ORDER BY candidate.started_at DESC LIMIT 1
                     )
                     LEFT JOIN feature_planning_proposals proposal ON proposal.feature_id = feature.id
                     LEFT JOIN json_each(proposal.proposal_json, '$.work_items') proposed
                       ON json_extract(proposed.value, '$.work_item_id') = item.id
                     WHERE epic.workspace_id = ?1 AND item.id = ?2",
                    params![workspace_id.to_string(), work_item_id.to_string()],
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
                            row.get::<_, i64>(12)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| AppError::Domain("Work item does not exist".to_owned()))
                .and_then(|row| {
                    Ok(DetailRow {
                        work_item: parse_id(&row.0)?,
                        feature: parse_id(&row.1)?,
                        epic: parse_id(&row.2)?,
                        key: row.3,
                        slug: row.4,
                        title: row.5,
                        status: parse_enum(&row.6)?,
                        feature_slug: row.7,
                        feature_title: row.8,
                        workflow_state: parse_enum(&row.9)?,
                        outcome_design_summary: row.10,
                        content_hash: row.11,
                        content_revision: row.12.max(1) as u64,
                    })
                })
        })
    }

    fn work_item_prerequisites(
        &self,
        workspace_id: core::WorkspaceId,
        work_item_id: core::WorkItemId,
    ) -> Result<Vec<core::WorkItem>, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        let dependency_ids = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT depends_on_work_item_id FROM work_item_dependencies WHERE work_item_id = ?1 ORDER BY depends_on_work_item_id",
            )?;
            statement
                .query_map([work_item_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })?;
        dependency_ids
            .into_iter()
            .map(|id| {
                let id: core::WorkItemId = parse_id(&id)?;
                snapshot
                    .work_items
                    .iter()
                    .find(|item| item.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Domain("Work item dependency does not exist".to_owned())
                    })
            })
            .collect()
    }

    fn work_item_checkpoints(
        &self,
        work_item_id: core::WorkItemId,
    ) -> Result<Vec<protocol::WorkItemCheckpointProjection>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, session_id, next_action_kind, summary, recorded_at
                 FROM work_item_checkpoints WHERE work_item_id = ?1 ORDER BY recorded_at, id",
            )?;
            let rows = statement
                .query_map([work_item_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|row| {
                    Ok(protocol::WorkItemCheckpointProjection {
                        id: parse_id(&row.0)?,
                        session_id: parse_id(&row.1)?,
                        next_action: parse_enum(&row.2)?,
                        summary: row.3,
                        recorded_at: row.4,
                    })
                })
                .collect()
        })
    }

    fn work_item_checkout_ids(
        &self,
        work_item_id: core::WorkItemId,
    ) -> Result<Vec<core::CheckoutId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT DISTINCT checkout_id FROM effective_work_item_checkouts WHERE work_item_id = ?1 ORDER BY checkout_id",
            )?;
            statement
                .query_map([work_item_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| parse_id(&row?))
                .collect()
        })
    }

    fn work_item_session_ids(
        &self,
        work_item_id: core::WorkItemId,
    ) -> Result<Vec<core::ConversationId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT DISTINCT association.session_id
                 FROM native_session_associations association
                 JOIN managed_sessions managed ON managed.session_id = association.session_id
                 WHERE association.work_item_id = ?1 ORDER BY association.session_id",
            )?;
            statement
                .query_map([work_item_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| parse_id(&row?))
                .collect()
        })
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

fn parse_enum<T>(value: &str) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn work_item_reference(item: &core::WorkItem) -> protocol::WorkItemReference {
    protocol::WorkItemReference {
        id: protocol::WorkItemId::from_uuid(*item.id.as_uuid()),
        feature_id: protocol::FeatureId::from_uuid(*item.feature_id.as_uuid()),
        key: item.key.to_string(),
        slug: item.slug.to_string(),
        title: item.title.clone(),
    }
}

fn is_complete(status: core::WorkItemStatus) -> bool {
    matches!(
        status,
        core::WorkItemStatus::Done | core::WorkItemStatus::Cancelled
    )
}

fn dependency_readiness(
    status: core::WorkItemStatus,
    dependencies_complete: bool,
) -> protocol::DependencyReadiness {
    if is_complete(status) {
        protocol::DependencyReadiness::Complete
    } else if status == core::WorkItemStatus::Blocked {
        protocol::DependencyReadiness::Blocked
    } else if dependencies_complete {
        protocol::DependencyReadiness::Ready
    } else {
        protocol::DependencyReadiness::Waiting
    }
}

fn work_item_status(status: core::WorkItemStatus) -> protocol::WorkItemStatus {
    parse_shared(status)
}

fn workflow_state(state: core::WorkflowState) -> protocol::WorkflowState {
    parse_shared(state)
}

fn parse_shared<T, U>(value: T) -> U
where
    T: serde::Serialize,
    U: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::to_value(value).expect("shared value serialization"))
        .expect("shared value contract")
}

fn status_label(status: core::WorkItemStatus) -> &'static str {
    match status {
        core::WorkItemStatus::Backlog => "in backlog",
        core::WorkItemStatus::Ready => "ready",
        core::WorkItemStatus::InProgress => "in progress",
        core::WorkItemStatus::Blocked => "blocked",
        core::WorkItemStatus::Review => "in review",
        core::WorkItemStatus::Done => "done",
        core::WorkItemStatus::Cancelled => "cancelled",
    }
}

struct SessionActionInputs {
    has_resumable: bool,
    has_live: bool,
}

fn session_action_inputs(
    sessions: &[protocol::SessionObservabilityProjection],
) -> SessionActionInputs {
    SessionActionInputs {
        has_resumable: sessions.iter().any(|session| {
            matches!(
                session.resumability,
                protocol::SessionResumability::Validated
                    | protocol::SessionResumability::PreflightPassed
            ) && session.liveness.state != protocol::SessionLiveState::Active
        }),
        has_live: sessions
            .iter()
            .any(|session| session.liveness.state == protocol::SessionLiveState::Active),
    }
}

fn work_item_actions(
    revision: u64,
    sessions: &SessionActionInputs,
) -> Vec<protocol::AvailableAction> {
    let unavailable = |code: &str, message: &str| {
        Some(protocol::UnavailableReason {
            code: code.to_owned(),
            message: message.to_owned(),
        })
    };
    [
        protocol::CommandCode::CheckpointWorkItem,
        protocol::CommandCode::StartSession,
        protocol::CommandCode::ResumeSession,
        protocol::CommandCode::FocusSession,
        protocol::CommandCode::FollowUpSession,
        protocol::CommandCode::RecoverSession,
    ]
    .into_iter()
    .map(|code| {
        let unavailable_reason = match code {
            protocol::CommandCode::CheckpointWorkItem => unavailable(
                "structured_checkpoint_unavailable",
                "Structured checkpoint editing is unavailable because the daemon has not accepted a revision-checked atomic structured checkpoint operation.",
            ),
            protocol::CommandCode::StartSession if sessions.has_live => unavailable(
                "writer_session_active",
                "A session is already writing in this Work item's checkout. Resume it, or close it before starting another.",
            ),
            protocol::CommandCode::StartSession => None,
            protocol::CommandCode::ResumeSession if sessions.has_resumable => None,
            protocol::CommandCode::ResumeSession if sessions.has_live => unavailable(
                "session_already_live",
                "The bound session is already running. Workboard will not launch a duplicate.",
            ),
            protocol::CommandCode::ResumeSession => unavailable(
                "no_resumable_session",
                "This Work item has no session with validated resume evidence.",
            ),
            protocol::CommandCode::FocusSession => unavailable(
                "session_focus_unavailable",
                "Focusing a running session is unavailable; Workboard cannot yet activate a terminal window.",
            ),
            protocol::CommandCode::FollowUpSession => unavailable(
                "session_follow_up_unavailable",
                "Sending a follow-up is unavailable; Workboard cannot yet deliver a prompt to a live session.",
            ),
            protocol::CommandCode::RecoverSession => unavailable(
                "session_recovery_unavailable",
                "Recovery is unavailable from Desktop; it must preview before executing.",
            ),
            _ => unavailable(
                "upstream_capability_not_accepted",
                "the authoritative Workboard operation has not been accepted",
            ),
        };
        protocol::AvailableAction {
            code,
            available: unavailable_reason.is_none(),
            unavailable_reason,
            expected_revision: Some(revision),
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::TempDir;
    use workboard_client_protocol as protocol;
    use workboard_core::{DocumentId, EpicId, FeatureId, RepositoryId, WorkItemId, WorkspaceId};

    use crate::workspace::WorkboardApplication;

    #[test]
    fn detail_is_revisioned_and_read_only_when_only_the_opaque_checkpoint_contract_exists() {
        let directory = TempDir::new().expect("temporary directory");
        let mut application = WorkboardApplication::open(directory.path().join("workboard.sqlite"))
            .expect("open application");
        let workspace_id = WorkspaceId::generate();
        let repository_id = RepositoryId::generate();
        let epic_id = EpicId::generate();
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        let epic_document_id = DocumentId::generate();
        let work_item_document_id = DocumentId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at) VALUES (?1, 'workspace', 'Workspace', ?2, '2026-08-31T12:00:00Z')",
                    params![workspace_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO repositories (id, workspace_id, slug, title, git_common_directory, default_branch, is_planning_store, created_at) VALUES (?1, ?2, 'planning', 'Planning', 'fixture-common-directory', 'main', 1, '2026-08-31T12:00:00Z')",
                    params![repository_id.to_string(), workspace_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at) VALUES (?1, ?2, 'epic', 'Epic', '2026-08-31T12:00:00Z')",
                    params![epic_id.to_string(), workspace_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at) VALUES (?1, ?2, 'feature', 'Feature', 'work_item_active', '2026-08-31T12:00:00Z')",
                    params![feature_id.to_string(), epic_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (id, feature_id, key, slug, title, status, created_at) VALUES (?1, ?2, 'epic/feature/item', 'item', 'Work item', 'in_progress', '2026-08-31T12:00:00Z')",
                    params![work_item_id.to_string(), feature_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id) VALUES (?1, ?2)",
                    params![work_item_id.to_string(), repository_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO documents (id, repository_id, epic_id, kind, relative_path, content_hash, observed_at) VALUES (?1, ?2, ?3, 'epic', 'epic.md', ?4, '2026-08-31T12:00:00Z')",
                    params![epic_document_id.to_string(), repository_id.to_string(), epic_id.to_string(), "a".repeat(64)],
                )?;
                transaction.execute(
                    "INSERT INTO documents (id, repository_id, work_item_id, kind, relative_path, content_hash, observed_at) VALUES (?1, ?2, ?3, 'work_item', 'item.md', ?4, '2026-08-31T12:00:00Z')",
                    params![work_item_document_id.to_string(), repository_id.to_string(), work_item_id.to_string(), "b".repeat(64)],
                )?;
                transaction.execute(
                    "UPDATE workspace_projection_revisions SET revision = 9 WHERE workspace_id = ?1",
                    [workspace_id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed detail fixture");

        let detail = application
            .client_work_item_detail(workspace_id, work_item_id)
            .expect("project Work-item detail");
        assert_eq!(detail.revision, 9);
        assert_eq!(detail.content_revision, 1);
        assert_eq!(detail.content_hash, "b".repeat(64));
        assert!(detail.checkpoint_history.is_empty());
        assert!(detail.sessions.is_empty());
        assert!(detail.checkouts.is_empty());
        let reason = |code: protocol::CommandCode| {
            detail
                .available_actions
                .iter()
                .find(|action| action.code == code)
                .expect("advertised action")
                .unavailable_reason
                .as_ref()
                .map(|reason| reason.code.as_str())
        };
        assert!(
            detail
                .available_actions
                .iter()
                .all(|action| action.unavailable_reason.is_none() == action.available)
        );
        assert_eq!(
            detail
                .available_actions
                .iter()
                .filter(|action| action.available)
                .map(|action| action.code)
                .collect::<Vec<_>>(),
            vec![protocol::CommandCode::StartSession],
            "a Work item with no bound session offers Start and nothing else"
        );
        assert_eq!(
            reason(protocol::CommandCode::ResumeSession),
            Some("no_resumable_session")
        );
        assert_eq!(
            reason(protocol::CommandCode::CheckpointWorkItem),
            Some("structured_checkpoint_unavailable")
        );
        assert_eq!(
            reason(protocol::CommandCode::FocusSession),
            Some("session_focus_unavailable")
        );
        assert_eq!(
            reason(protocol::CommandCode::FollowUpSession),
            Some("session_follow_up_unavailable")
        );
        assert_eq!(
            reason(protocol::CommandCode::RecoverSession),
            Some("session_recovery_unavailable")
        );
    }
}
