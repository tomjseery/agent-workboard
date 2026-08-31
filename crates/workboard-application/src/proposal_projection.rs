use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::planning_workflow::{FeatureProposal, ProposedWorkItem};
use crate::workspace::WorkboardApplication;

#[derive(Deserialize)]
struct StoredProposal {
    proposal: FeatureProposal,
    work_items: Vec<StoredWorkItem>,
}

#[derive(Deserialize)]
struct StoredWorkItem {
    work_item_id: core::WorkItemId,
    proposal: ProposedWorkItem,
}

struct ProposalRow {
    feature_id: core::FeatureId,
    epic_id: core::EpicId,
    slug: String,
    title: String,
    workflow_state: core::WorkflowState,
    proposal_json: String,
    submitted_at: String,
    revision: u64,
    generation: u64,
}

impl WorkboardApplication {
    pub fn client_feature_proposal(
        &self,
        workspace_id: core::WorkspaceId,
        feature_id: core::FeatureId,
    ) -> Result<protocol::FeatureProposalProjection, AppError> {
        let row = self.proposal_row(workspace_id, feature_id)?;
        let stored: StoredProposal = serde_json::from_str(&row.proposal_json)?;
        let repository_lookup = self.proposal_repository_lookup(workspace_id)?;
        let proposal_hash = format!("{:x}", Sha256::digest(row.proposal_json.as_bytes()));
        let planner_sessions = self.planner_sessions(feature_id)?;
        let mut repository_ids = HashSet::new();
        let work_items = stored
            .work_items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let repositories = item
                    .proposal
                    .repository_ids
                    .iter()
                    .filter_map(|id| {
                        repository_ids.insert(*id);
                        repository_lookup.get(id).cloned()
                    })
                    .collect();
                protocol::ProposedWorkItemProjection {
                    id: protocol::WorkItemId::from_uuid(*item.work_item_id.as_uuid()),
                    slug: item.proposal.slug.to_string(),
                    title: item.proposal.title,
                    body: item.proposal.body,
                    repositories,
                    dependencies: item
                        .proposal
                        .dependencies
                        .into_iter()
                        .map(|slug| slug.to_string())
                        .collect(),
                    position: index + 1,
                }
            })
            .collect::<Vec<_>>();
        let mut repositories = repository_ids
            .into_iter()
            .filter_map(|id| repository_lookup.get(&id).cloned())
            .collect::<Vec<_>>();
        repositories.sort_by(|left, right| left.slug.cmp(&right.slug));
        let changed_since_previous = row.generation > 1;
        let mut warnings = Vec::new();
        if changed_since_previous {
            warnings.push(protocol::ProposalWarningProjection {
                code: "proposal_changed".to_owned(),
                severity: protocol::ErrorSeverity::Warning,
                message: "This proposal replaces an earlier submitted generation.".to_owned(),
            });
        }
        if planner_sessions.is_empty() {
            warnings.push(protocol::ProposalWarningProjection {
                code: "planner_not_bound".to_owned(),
                severity: protocol::ErrorSeverity::Warning,
                message: "No planner session is currently bound to this Feature.".to_owned(),
            });
        }
        if repositories.len() > 1 {
            warnings.push(protocol::ProposalWarningProjection {
                code: "cross_repository_scope".to_owned(),
                severity: protocol::ErrorSeverity::Info,
                message: "This proposal spans multiple repositories.".to_owned(),
            });
        }
        let diagnostics = (row.workflow_state == core::WorkflowState::ReconciliationRequired)
            .then(|| protocol::Diagnostic {
                code: "publication_reconciliation_required".to_owned(),
                severity: protocol::ErrorSeverity::Error,
                message: "Publication requires authoritative reconciliation outside Desktop."
                    .to_owned(),
                owner: Some(protocol::EntityRef::Feature(
                    protocol::FeatureId::from_uuid(*row.feature_id.as_uuid()),
                )),
            })
            .into_iter()
            .collect();
        Ok(protocol::FeatureProposalProjection {
            feature: protocol::FeatureReference {
                id: protocol::FeatureId::from_uuid(*row.feature_id.as_uuid()),
                epic_id: protocol::EpicId::from_uuid(*row.epic_id.as_uuid()),
                slug: row.slug,
                title: row.title,
            },
            generation: row.generation,
            revision: row.revision,
            proposal_hash,
            submitted_at: row.submitted_at,
            changed_since_previous,
            feature_body: stored.proposal.feature_body,
            work_items,
            repositories,
            verification_gates: stored.proposal.verification,
            warnings,
            planner_sessions,
            diagnostics,
            workflow_state: workflow_state(row.workflow_state),
            available_actions: proposal_actions(row.revision),
        })
    }

    pub fn client_approval_queue(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<protocol::ApprovalQueueProjection, AppError> {
        let mut revision = self.projection_revision(workspace_id)?;
        let feature_ids = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT proposal.feature_id
                 FROM feature_planning_proposals proposal
                 JOIN features feature ON feature.id = proposal.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE epic.workspace_id = ?1 AND proposal.status <> 'published'",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|id| parse_id(&id))
                .collect::<Result<Vec<_>, _>>()
        })?;
        let mut proposals = feature_ids
            .into_iter()
            .map(|id| self.client_feature_proposal(workspace_id, id))
            .collect::<Result<Vec<_>, _>>()?;
        revision = proposals
            .iter()
            .fold(revision, |current, proposal| current.max(proposal.revision));
        proposals.sort_by(|left, right| {
            workflow_rank(left.workflow_state)
                .cmp(&workflow_rank(right.workflow_state))
                .then_with(|| left.submitted_at.cmp(&right.submitted_at))
                .then_with(|| {
                    left.feature
                        .id
                        .to_string()
                        .cmp(&right.feature.id.to_string())
                })
        });
        let total_count = proposals.len();
        let entries = proposals
            .into_iter()
            .enumerate()
            .map(|(index, proposal)| protocol::ApprovalQueueItemProjection {
                feature: proposal.feature,
                generation: proposal.generation,
                revision: proposal.revision,
                proposal_hash: proposal.proposal_hash,
                submitted_at: proposal.submitted_at,
                changed_since_previous: proposal.changed_since_previous,
                workflow_state: proposal.workflow_state,
                repositories: proposal.repositories,
                warning_count: proposal.warnings.len(),
                planner_count: proposal.planner_sessions.len(),
                available_actions: proposal.available_actions,
                position: index + 1,
                total_count,
            })
            .collect();
        Ok(protocol::ApprovalQueueProjection { entries, revision })
    }

    fn proposal_row(
        &self,
        workspace_id: core::WorkspaceId,
        feature_id: core::FeatureId,
    ) -> Result<ProposalRow, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT feature.id, feature.epic_id, feature.slug, feature.title,
                            feature.workflow_state, proposal.proposal_json, proposal.submitted_at,
                            COALESCE(MAX(event.sequence), 1),
                            COALESCE(SUM(CASE WHEN event.to_state = 'proposal_ready' THEN 1 ELSE 0 END), 1)
                     FROM feature_planning_proposals proposal
                     JOIN features feature ON feature.id = proposal.feature_id
                     JOIN epics epic ON epic.id = feature.epic_id
                     LEFT JOIN workflow_events event ON event.run_id = proposal.workflow_run_id
                     WHERE epic.workspace_id = ?1 AND feature.id = ?2
                     GROUP BY feature.id, feature.epic_id, feature.slug, feature.title,
                              feature.workflow_state, proposal.proposal_json, proposal.submitted_at",
                    params![workspace_id.to_string(), feature_id.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| AppError::Domain("Feature proposal does not exist".to_owned()))
                .and_then(
                    |(feature, epic, slug, title, state, proposal_json, submitted_at, revision, generation)| {
                        Ok(ProposalRow {
                            feature_id: parse_id(&feature)?,
                            epic_id: parse_id(&epic)?,
                            slug,
                            title,
                            workflow_state: parse_workflow_state(&state)?,
                            proposal_json,
                            submitted_at,
                            revision: revision.max(1) as u64,
                            generation: generation.max(1) as u64,
                        })
                    },
                )
        })
    }

    fn proposal_repository_lookup(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<HashMap<core::RepositoryId, protocol::RepositoryReference>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection
                .prepare("SELECT id, slug, title FROM repositories WHERE workspace_id = ?1")?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, slug, title)| {
                    let id: core::RepositoryId = parse_id(&id)?;
                    Ok((
                        id,
                        protocol::RepositoryReference {
                            id: protocol::RepositoryId::from_uuid(*id.as_uuid()),
                            workspace_id: protocol::WorkspaceId::from_uuid(*workspace_id.as_uuid()),
                            slug,
                            title,
                        },
                    ))
                })
                .collect()
        })
    }

    fn planner_sessions(
        &self,
        feature_id: core::FeatureId,
    ) -> Result<Vec<protocol::PlannerSessionProjection>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT session.id, session.provider, association.role,
                        association.associated_until,
                        observation.status, observation.observed_at
                 FROM native_session_associations association
                 JOIN native_sessions session ON session.id = association.session_id
                 LEFT JOIN live_observations observation ON observation.id = (
                     SELECT candidate.id FROM live_observations candidate
                     WHERE candidate.session_id = session.id
                     ORDER BY candidate.observed_at DESC LIMIT 1
                 )
                 WHERE association.feature_id = ?1 AND association.role = 'feature_planning'
                 ORDER BY association.associated_from, session.id",
            )?;
            let rows = statement
                .query_map([feature_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(id, provider, role, ended, live, observed)| {
                    Ok(protocol::PlannerSessionProjection {
                        id: protocol::SessionId::from_uuid(
                            *parse_id::<core::ConversationId>(&id)?.as_uuid(),
                        ),
                        provider: parse_provider(&provider)?,
                        role: parse_role(&role)?,
                        binding_state: if ended.is_some() {
                            protocol::SessionBindingState::Stopped
                        } else {
                            protocol::SessionBindingState::Current
                        },
                        live_state: parse_live_state(live.as_deref()),
                        last_activity_at: observed,
                    })
                })
                .collect()
        })
    }
}

fn proposal_actions(revision: u64) -> Vec<protocol::AvailableAction> {
    [
        protocol::CommandCode::ApproveFeature,
        protocol::CommandCode::RequestFeatureRevision,
        protocol::CommandCode::RejectFeature,
    ]
    .into_iter()
    .map(|code| protocol::AvailableAction {
        code,
        available: false,
        unavailable_reason: Some(protocol::UnavailableReason {
            code: "publication_policy_unavailable".to_owned(),
            message: "Desktop approval actions are unavailable until the daemon accepts the typed publication policy.".to_owned(),
        }),
        expected_revision: Some(revision),
    })
    .collect()
}

fn workflow_rank(state: protocol::WorkflowState) -> usize {
    match state {
        protocol::WorkflowState::AwaitingApproval => 1,
        protocol::WorkflowState::ReconciliationRequired => 2,
        protocol::WorkflowState::Publishing => 3,
        protocol::WorkflowState::PlanningActive => 4,
        _ => 5,
    }
}

fn workflow_state(state: core::WorkflowState) -> protocol::WorkflowState {
    serde_json::from_value(serde_json::to_value(state).expect("workflow state serialization"))
        .expect("shared workflow state")
}

fn parse_workflow_state(value: &str) -> Result<core::WorkflowState, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_provider(value: &str) -> Result<protocol::Provider, AppError> {
    match value {
        "claude" => Ok(protocol::Provider::Claude),
        "codex" => Ok(protocol::Provider::Codex),
        _ => Err(AppError::Domain("planner provider is invalid".to_owned())),
    }
}

fn parse_role(value: &str) -> Result<protocol::ManagedSessionRole, AppError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_live_state(value: Option<&str>) -> protocol::SessionLiveState {
    match value {
        Some("active") => protocol::SessionLiveState::Active,
        Some("idle") => protocol::SessionLiveState::Idle,
        Some("stopped") => protocol::SessionLiveState::Stopped,
        Some("system_error") => protocol::SessionLiveState::SystemError,
        Some(_) => protocol::SessionLiveState::Unknown,
        None => protocol::SessionLiveState::NotLoaded,
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

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::TempDir;
    use workboard_core::{
        AssociationIntervalId, ConversationId, DocumentId, EpicId, FeatureId, RepositoryId,
        WorkItemId, WorkflowEventId, WorkflowRunId, WorkspaceId,
    };

    use crate::workspace::WorkboardApplication;

    #[test]
    fn proposal_fixtures_cover_empty_changed_cross_repository_and_planner_cardinality() {
        let directory = TempDir::new().expect("temporary directory");
        let mut application = WorkboardApplication::open(directory.path().join("workboard.sqlite"))
            .expect("open application");
        let workspace_id = WorkspaceId::generate();
        let planning_repository_id = RepositoryId::generate();
        let repository_ids = [RepositoryId::generate(), RepositoryId::generate()];
        let epic_id = EpicId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
                     VALUES (?1, 'fixture', 'Fixture', ?2, '2026-08-31T12:00:00Z')",
                    params![workspace_id.to_string(), planning_repository_id.to_string()],
                )?;
                for (id, slug, title, planning) in [
                    (planning_repository_id, "planning", "Planning", 1),
                    (repository_ids[0], "service-a", "Service A", 0),
                    (repository_ids[1], "service-b", "Service B", 0),
                ] {
                    transaction.execute(
                        "INSERT INTO repositories (
                             id, workspace_id, slug, title, git_common_directory,
                             default_branch, is_planning_store, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'main', ?6, '2026-08-31T12:00:00Z')",
                        params![
                            id.to_string(),
                            workspace_id.to_string(),
                            slug,
                            title,
                            format!("C:/fixture/{slug}/.git"),
                            planning,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO epics (id, workspace_id, slug, title, created_at)
                     VALUES (?1, ?2, 'delivery', 'Delivery', '2026-08-31T12:00:00Z')",
                    params![epic_id.to_string(), workspace_id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed Workspace");

        assert!(
            application
                .client_approval_queue(workspace_id)
                .expect("empty queue")
                .entries
                .is_empty()
        );

        let changed_feature = seed_proposal(
            &mut application,
            epic_id,
            &repository_ids,
            "changed-proposal",
            "awaiting_approval",
            2,
            true,
        );
        assert_eq!(
            application
                .client_approval_queue(workspace_id)
                .expect("one proposal queue")
                .entries
                .len(),
            1
        );
        let reconciliation_feature = seed_proposal(
            &mut application,
            epic_id,
            &repository_ids,
            "reconciliation",
            "reconciliation_required",
            1,
            false,
        );
        let sessions = [ConversationId::generate(), ConversationId::generate()];
        for (index, session_id) in sessions.into_iter().enumerate() {
            application
                .store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                         VALUES (?1, ?2, ?3, '2026-08-31T12:00:00Z')",
                        params![
                            session_id.to_string(),
                            if index == 0 { "codex" } else { "claude" },
                            format!("planner-{index}"),
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO native_session_associations (
                             id, session_id, feature_id, role, associated_from
                         ) VALUES (?1, ?2, ?3, 'feature_planning', '2026-08-31T12:00:00Z')",
                        params![
                            AssociationIntervalId::generate().to_string(),
                            session_id.to_string(),
                            changed_feature.to_string(),
                        ],
                    )?;
                    Ok(())
                })
                .expect("seed planner session");
            if index == 0 {
                assert_eq!(
                    application
                        .client_feature_proposal(workspace_id, changed_feature)
                        .expect("one planner proposal")
                        .planner_sessions
                        .len(),
                    1
                );
            }
        }

        let changed = application
            .client_feature_proposal(workspace_id, changed_feature)
            .expect("changed proposal");
        assert_eq!(changed.generation, 2);
        assert!(changed.changed_since_previous);
        assert_eq!(changed.proposal_hash.len(), 64);
        assert_eq!(changed.repositories.len(), 2);
        assert_eq!(changed.work_items.len(), 2);
        assert_eq!(changed.work_items[1].dependencies, ["foundation"]);
        assert_eq!(changed.verification_gates.len(), 2);
        assert_eq!(changed.planner_sessions.len(), 2);
        assert!(changed.feature_body.contains("<script>"));
        assert!(
            changed
                .warnings
                .iter()
                .any(|warning| warning.code == "proposal_changed")
        );
        assert!(
            changed
                .available_actions
                .iter()
                .all(|action| !action.available)
        );
        assert!(changed.available_actions.iter().all(|action| {
            action
                .unavailable_reason
                .as_ref()
                .is_some_and(|reason| reason.code == "publication_policy_unavailable")
        }));

        let reconciliation = application
            .client_feature_proposal(workspace_id, reconciliation_feature)
            .expect("reconciliation proposal");
        assert!(reconciliation.planner_sessions.is_empty());
        assert!(
            reconciliation
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "publication_reconciliation_required")
        );

        let queue = application
            .client_approval_queue(workspace_id)
            .expect("approval queue");
        assert_eq!(queue.entries.len(), 2);
        assert_eq!(queue.revision, 2);
        assert_eq!(
            queue.entries[0].feature.id.as_uuid(),
            changed_feature.as_uuid()
        );
        assert_eq!(queue.entries[0].position, 1);
        assert_eq!(queue.entries[1].total_count, 2);
    }

    fn seed_proposal(
        application: &mut WorkboardApplication,
        epic_id: EpicId,
        repository_ids: &[RepositoryId; 2],
        slug: &str,
        state: &str,
        generations: usize,
        hostile: bool,
    ) -> FeatureId {
        let feature_id = FeatureId::generate();
        let run_id = WorkflowRunId::generate();
        let first_work_item = WorkItemId::generate();
        let second_work_item = WorkItemId::generate();
        let body = if hostile {
            "# Long proposal\n<script>alert('no')</script>\n[jump](javascript:alert(1))\n"
                .repeat(200)
        } else {
            "# Reconciliation proposal".to_owned()
        };
        let proposal = serde_json::json!({
            "feature_document_id": DocumentId::generate(),
            "proposal": {
                "feature_body": body,
                "work_items": [
                    { "slug": "foundation", "title": "Foundation", "body": "Foundation body", "repository_ids": [repository_ids[0]], "dependencies": [] },
                    { "slug": "delivery", "title": "Delivery", "body": "Delivery body", "repository_ids": repository_ids, "dependencies": ["foundation"] }
                ],
                "expected_epic_content_hash": "a".repeat(64),
                "expected_repository_head": "b".repeat(40),
                "verification": ["Focused checks pass", "Full suite passes"],
                "first_work_item_slug": "foundation"
            },
            "work_items": [
                { "work_item_id": first_work_item, "document_id": DocumentId::generate(), "proposal": { "slug": "foundation", "title": "Foundation", "body": "Foundation body", "repository_ids": [repository_ids[0]], "dependencies": [] } },
                { "work_item_id": second_work_item, "document_id": DocumentId::generate(), "proposal": { "slug": "delivery", "title": "Delivery", "body": "Delivery body", "repository_ids": repository_ids, "dependencies": ["foundation"] } }
            ]
        })
        .to_string();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '2026-08-31T12:00:00Z')",
                    params![
                        feature_id.to_string(),
                        epic_id.to_string(),
                        slug,
                        slug,
                        state
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO workflow_runs (id, feature_id, current_state, started_at)
                     VALUES (?1, ?2, ?3, '2026-08-31T12:00:00Z')",
                    params![run_id.to_string(), feature_id.to_string(), state],
                )?;
                for sequence in 1..=generations {
                    transaction.execute(
                        "INSERT INTO workflow_events (
                             id, run_id, sequence, from_state, to_state, actor,
                             occurred_at, payload_json
                         ) VALUES (?1, ?2, ?3, 'planning_active', 'proposal_ready',
                                   'integration', '2026-08-31T12:00:00Z', '{}')",
                        params![
                            WorkflowEventId::generate().to_string(),
                            run_id.to_string(),
                            sequence as i64,
                        ],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO feature_planning_proposals (
                         feature_id, workflow_run_id, idempotency_key, proposal_json,
                         status, submitted_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, '2026-08-31T12:00:00Z')",
                    params![
                        feature_id.to_string(),
                        run_id.to_string(),
                        format!("proposal-{feature_id}"),
                        proposal,
                        if state == "reconciliation_required" {
                            "publishing"
                        } else {
                            state
                        },
                    ],
                )?;
                Ok(())
            })
            .expect("seed proposal");
        feature_id
    }
}
