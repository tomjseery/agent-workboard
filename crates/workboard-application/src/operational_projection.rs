use std::path::Path;
use std::str::FromStr;

use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::workspace::WorkboardApplication;

impl WorkboardApplication {
    pub fn client_repository_observability(
        &self,
        workspace_id: core::WorkspaceId,
        repository_id: core::RepositoryId,
    ) -> Result<protocol::RepositoryObservabilityProjection, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let snapshot = self.snapshot(workspace_id)?;
        let repository = snapshot
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| AppError::Domain("Repository does not exist".to_owned()))?;
        let observed_at = repository.paths.iter().map(|path| path.observed_at).max();
        let mut checkout_ids = snapshot
            .checkouts
            .iter()
            .filter(|checkout| checkout.repository_id == repository_id)
            .map(|checkout| checkout_id(checkout.id))
            .collect::<Vec<_>>();
        checkout_ids.sort_by_key(ToString::to_string);
        let mut remote_names = repository
            .remotes
            .iter()
            .map(|remote| remote.name.clone())
            .collect::<Vec<_>>();
        remote_names.sort();
        remote_names.dedup();
        Ok(protocol::RepositoryObservabilityProjection {
            repository: repository_reference(repository),
            display_paths: repository
                .paths
                .iter()
                .map(|path| observed_display_path(&path.path, path.observed_at, path.superseded_at))
                .collect(),
            remote_evidence: evidence(
                if remote_names.is_empty() {
                    protocol::EvidenceState::Missing
                } else {
                    protocol::EvidenceState::Current
                },
                if remote_names.is_empty() {
                    "remote_evidence_missing"
                } else {
                    "remote_names_observed"
                },
                if remote_names.is_empty() {
                    "No remote evidence is currently recorded."
                } else {
                    "Remote names were observed by Workboard."
                },
                observed_at,
            ),
            default_branch_evidence: evidence(
                if repository.default_branch.is_some() {
                    protocol::EvidenceState::Current
                } else {
                    protocol::EvidenceState::Unknown
                },
                if repository.default_branch.is_some() {
                    "default_branch_observed"
                } else {
                    "default_branch_unknown"
                },
                if repository.default_branch.is_some() {
                    "The default branch was observed by Workboard."
                } else {
                    "Workboard has no authoritative default-branch evidence."
                },
                observed_at,
            ),
            remote_names,
            default_branch: repository.default_branch.clone(),
            checkout_ids,
            revision,
            diagnostics: Vec::new(),
        })
    }

    pub fn client_checkout_observability(
        &self,
        workspace_id: core::WorkspaceId,
        requested_id: core::CheckoutId,
    ) -> Result<protocol::CheckoutObservabilityProjection, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let snapshot = self.snapshot(workspace_id)?;
        let checkout = snapshot
            .checkouts
            .iter()
            .find(|item| item.id == requested_id)
            .ok_or_else(|| AppError::Domain("Checkout does not exist".to_owned()))?;
        let repository = snapshot
            .repositories
            .iter()
            .find(|item| item.id == checkout.repository_id)
            .ok_or_else(|| AppError::Domain("Checkout Repository does not exist".to_owned()))?;
        let readiness = self.checkout_readiness_row(requested_id)?;
        let mut bindings = snapshot
            .effective_checkouts
            .iter()
            .filter(|binding| binding.checkout_id == requested_id)
            .map(|binding| protocol::CheckoutBindingProjection {
                feature_id: feature_id(binding.feature_id),
                work_item_id: binding.work_item_id.map(work_item_id),
                purpose_source: if binding.work_item_id.is_none() {
                    protocol::CheckoutPurposeSource::Declared
                } else if binding.inherited {
                    protocol::CheckoutPurposeSource::Inherited
                } else {
                    protocol::CheckoutPurposeSource::Override
                },
            })
            .collect::<Vec<_>>();
        bindings.sort_by_key(|binding| {
            (
                binding.feature_id.to_string(),
                binding.work_item_id.map(|id| id.to_string()),
            )
        });
        let purpose_source = bindings
            .iter()
            .find_map(|binding| binding.work_item_id.map(|_| binding.purpose_source))
            .or_else(|| bindings.first().map(|binding| binding.purpose_source))
            .unwrap_or(protocol::CheckoutPurposeSource::Unknown);
        let replaced_by_checkout_id = snapshot
            .checkouts
            .iter()
            .find(|candidate| candidate.replaces_checkout_id == Some(requested_id))
            .map(|candidate| checkout_id(candidate.id));
        let current = checkout.availability == core::CheckoutAvailability::Available;
        Ok(protocol::CheckoutObservabilityProjection {
            id: checkout_id(checkout.id),
            repository: repository_reference(repository),
            purpose: readiness
                .as_ref()
                .map(|row| row.purpose)
                .unwrap_or(protocol::CheckoutPurpose::Unknown),
            purpose_source,
            branch: checkout.branch.clone(),
            head: checkout.head.clone(),
            isolation_generation: readiness.as_ref().map(|row| row.isolation_generation),
            reconciliation_generation: readiness.as_ref().map(|row| row.reconciliation_generation),
            availability: checkout_availability(checkout.availability),
            display_paths: checkout
                .paths
                .iter()
                .map(|path| {
                    observed_display_path(&path.path, path.observed_from, path.observed_until)
                })
                .collect(),
            replaces_checkout_id: checkout.replaces_checkout_id.map(checkout_id),
            replaced_by_checkout_id,
            bindings,
            session_ids: self.checkout_session_ids(requested_id)?,
            dirty_evidence: evidence(
                protocol::EvidenceState::NotLoaded,
                "dirty_evidence_not_loaded",
                "No authoritative dirty-state observation is loaded.",
                readiness.as_ref().map(|row| row.observed_at),
            ),
            collision_evidence: collision_evidence(
                checkout.availability,
                readiness.as_ref().map(|row| row.observed_at),
            ),
            reconciliation_evidence: evidence(
                if current {
                    protocol::EvidenceState::Current
                } else {
                    protocol::EvidenceState::Conflict
                },
                if current {
                    "checkout_reconciled"
                } else {
                    "checkout_reconciliation_required"
                },
                if current {
                    "The latest recorded checkout generation is available."
                } else {
                    "The checkout is missing, deleted, or replaced."
                },
                readiness.as_ref().map(|row| row.observed_at),
            ),
            revision,
            diagnostics: Vec::new(),
        })
    }

    pub fn client_session_observability(
        &self,
        workspace_id: core::WorkspaceId,
        requested_id: core::ConversationId,
    ) -> Result<protocol::SessionObservabilityProjection, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let row = self
            .managed_session_row(workspace_id, requested_id)?
            .ok_or_else(|| AppError::Domain("Bound session does not exist".to_owned()))?;
        let owner = parse_owner(row.epic_id, row.feature_id, row.work_item_id)?;
        let liveness = liveness(
            row.live_status.as_deref(),
            row.live_observed_at.as_deref(),
            row.live_expires_at.as_deref(),
        )?;
        let restore_state = match (
            &row.restore_added_at,
            &row.restore_removed_at,
            &row.restore_remove_reason,
        ) {
            (None, _, _) => protocol::SessionRestoreState::NotTracked,
            (Some(_), None, _) => protocol::SessionRestoreState::Tracked,
            (Some(_), Some(_), Some(reason)) if reason == "reconciliation_required" => {
                protocol::SessionRestoreState::Conflict
            }
            (Some(_), Some(_), _) => protocol::SessionRestoreState::Removed,
        };
        let checkout_id = parse_optional_id(row.checkout_id.as_deref())?;
        let resumability = match (row.source_count, row.checkout_availability.as_deref()) {
            (0, _) => protocol::SessionResumability::Missing,
            (_, Some("missing" | "deleted" | "replaced")) => protocol::SessionResumability::Missing,
            _ => protocol::SessionResumability::Unknown,
        };
        let role = session_role(&row.role)?;
        let primary_writer = if role != protocol::ManagedSessionRole::WorkItemExecution {
            protocol::PrimaryWriterEvidence::NotApplicable
        } else if let Some(work_item_id) = owner_work_item_id(owner) {
            match self.current_writer_count(work_item_id)? {
                0 => protocol::PrimaryWriterEvidence::Unknown,
                1 => protocol::PrimaryWriterEvidence::ConfirmedPrimary,
                _ => protocol::PrimaryWriterEvidence::Conflict,
            }
        } else {
            protocol::PrimaryWriterEvidence::Conflict
        };
        let mut diagnostics = Vec::new();
        if liveness.stale {
            diagnostics.push(diagnostic(
                "session_evidence_stale",
                "The latest liveness evidence has expired.",
                owner,
            ));
        }
        if primary_writer == protocol::PrimaryWriterEvidence::Conflict {
            diagnostics.push(diagnostic(
                "primary_writer_conflict",
                "More than one current writer is recorded.",
                owner,
            ));
        }
        Ok(protocol::SessionObservabilityProjection {
            id: session_id(requested_id),
            provider: provider(&row.provider)?,
            role,
            owner,
            authoritative_profile: None,
            authoritative_model: None,
            profile_evidence: evidence(
                protocol::EvidenceState::NotLoaded,
                "profile_evidence_not_loaded",
                "No authoritative profile or model evidence is loaded.",
                None,
            ),
            binding_state: binding_state(&row.managed_status, row.managed_until.as_deref()),
            liveness,
            restore_state,
            last_activity_at: last_activity(row.snapshot_json.as_deref()),
            checkout_id,
            resumability,
            primary_writer,
            revision,
            diagnostics,
        })
    }

    pub fn client_recovery_preview(
        &self,
        workspace_id: core::WorkspaceId,
        requested_id: core::ConversationId,
    ) -> Result<protocol::RecoveryPreviewProjection, AppError> {
        let session = self.client_session_observability(workspace_id, requested_id)?;
        let mut conflicts = self.recovery_conflicts(requested_id, session.owner)?;
        let disposition = if !conflicts.is_empty()
            || session.restore_state == protocol::SessionRestoreState::Conflict
        {
            protocol::RecoveryDispositionProjection::Conflict
        } else if matches!(
            session.liveness.state,
            protocol::SessionLiveState::Active | protocol::SessionLiveState::Idle
        ) && !session.liveness.stale
        {
            protocol::RecoveryDispositionProjection::AlreadyLive
        } else if session.restore_state == protocol::SessionRestoreState::NotTracked {
            protocol::RecoveryDispositionProjection::NotLoaded
        } else if session.resumability == protocol::SessionResumability::Missing {
            protocol::RecoveryDispositionProjection::Unresumable
        } else if session.checkout_id.is_some() {
            protocol::RecoveryDispositionProjection::ReadyPresent
        } else {
            protocol::RecoveryDispositionProjection::ReadyRecreate
        };
        if session.resumability == protocol::SessionResumability::Missing {
            conflicts.push(diagnostic(
                "session_unresumable",
                "Required resume evidence is missing.",
                session.owner,
            ));
        }
        Ok(protocol::RecoveryPreviewProjection {
            session_id: session.id,
            disposition,
            conflicts,
            observed_at: timestamp(OffsetDateTime::now_utc()),
            stale: session.liveness.stale,
            revision: session.revision,
        })
    }

    fn checkout_readiness_row(
        &self,
        checkout_id: core::CheckoutId,
    ) -> Result<Option<ReadinessRow>, AppError> {
        self.store.read(|connection| {
            connection.query_row(
                "SELECT purpose, isolation_generation, reconciliation_generation, observed_at FROM checkout_readiness WHERE checkout_id = ?1",
                [checkout_id.to_string()],
                |row| Ok(ReadinessRow {
                    purpose: checkout_purpose(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
                    isolation_generation: row.get::<_, i64>(1)? as u64,
                    reconciliation_generation: row.get::<_, i64>(2)? as u64,
                    observed_at: parse_timestamp_sql(&row.get::<_, String>(3)?)?,
                }),
            ).optional().map_err(Into::into)
        })
    }

    fn checkout_session_ids(
        &self,
        checkout_id: core::CheckoutId,
    ) -> Result<Vec<protocol::SessionId>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT DISTINCT session_id FROM managed_sessions WHERE checkout_id = ?1 ORDER BY session_id",
            )?;
            statement.query_map([checkout_id.to_string()], |row| row.get::<_, String>(0))?
                .map(|value| parse_id::<core::ConversationId>(&value?).map(session_id))
                .collect()
        })
    }

    fn managed_session_row(
        &self,
        workspace_id: core::WorkspaceId,
        session_id: core::ConversationId,
    ) -> Result<Option<ManagedSessionRow>, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT session.provider, managed.checkout_id, managed.role, managed.status,
                    managed.managed_until, association.epic_id, association.feature_id,
                    association.work_item_id, restore.added_at, restore.removed_at,
                    restore.remove_reason, live.status, live.observed_at, live.expires_at,
                    checkout.availability,
                    (SELECT source.snapshot_json FROM native_session_sources source
                     WHERE source.session_id = session.id AND source.missing = 0
                     ORDER BY source.observed_at DESC LIMIT 1),
                    (SELECT COUNT(*) FROM native_session_sources source
                     WHERE source.session_id = session.id AND source.missing = 0)
             FROM native_sessions session
             JOIN managed_sessions managed ON managed.id = (
                 SELECT candidate.id FROM managed_sessions candidate
                 WHERE candidate.session_id = session.id
                 ORDER BY candidate.managed_from DESC LIMIT 1)
             JOIN checkouts checkout ON checkout.id = managed.checkout_id
             JOIN repositories repository ON repository.id = checkout.repository_id
             LEFT JOIN native_session_associations association ON association.id = (
                 SELECT candidate.id FROM native_session_associations candidate
                 WHERE candidate.session_id = session.id
                 ORDER BY candidate.associated_from DESC LIMIT 1)
             LEFT JOIN restore_entries restore ON restore.session_id = session.id
             LEFT JOIN live_observations live ON live.id = (
                 SELECT candidate.id FROM live_observations candidate
                 WHERE candidate.session_id = session.id
                 ORDER BY candidate.observed_at DESC LIMIT 1)
             WHERE session.id = ?1 AND repository.workspace_id = ?2",
                    params![session_id.to_string(), workspace_id.to_string()],
                    |row| {
                        Ok(ManagedSessionRow {
                            provider: row.get(0)?,
                            checkout_id: row.get(1)?,
                            role: row.get(2)?,
                            managed_status: row.get(3)?,
                            managed_until: row.get(4)?,
                            epic_id: row.get(5)?,
                            feature_id: row.get(6)?,
                            work_item_id: row.get(7)?,
                            restore_added_at: row.get(8)?,
                            restore_removed_at: row.get(9)?,
                            restore_remove_reason: row.get(10)?,
                            live_status: row.get(11)?,
                            live_observed_at: row.get(12)?,
                            live_expires_at: row.get(13)?,
                            checkout_availability: row.get(14)?,
                            snapshot_json: row.get(15)?,
                            source_count: row.get::<_, i64>(16)? as usize,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
        })
    }

    fn current_writer_count(&self, work_item_id: protocol::WorkItemId) -> Result<usize, AppError> {
        self.store.read(|connection| connection.query_row(
            "SELECT COUNT(DISTINCT managed.session_id)
             FROM managed_sessions managed
             JOIN native_session_associations association ON association.session_id = managed.session_id
             WHERE managed.managed_until IS NULL AND managed.role = 'work_item_execution'
               AND association.associated_until IS NULL AND association.work_item_id = ?1",
            [work_item_id.to_string()],
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        ).map_err(Into::into))
    }

    fn recovery_conflicts(
        &self,
        session_id: core::ConversationId,
        owner: protocol::OwnerProjection,
    ) -> Result<Vec<protocol::Diagnostic>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT outcome.code FROM recovery_entry_outcomes outcome
                 JOIN recovery_attempts attempt ON attempt.id = outcome.attempt_id
                 WHERE outcome.session_id = ?1 AND outcome.status IN ('conflict', 'failed')
                 ORDER BY outcome.observed_at DESC LIMIT 8",
            )?;
            statement
                .query_map([session_id.to_string()], |row| {
                    let stored_code = row.get::<_, Option<String>>(0)?;
                    let (code, message) = safe_recovery_conflict(stored_code.as_deref());
                    Ok(protocol::Diagnostic {
                        code: code.to_owned(),
                        severity: protocol::ErrorSeverity::Warning,
                        message: message.to_owned(),
                        owner: Some(owner_entity(owner)),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }
}

fn safe_recovery_conflict(code: Option<&str>) -> (&'static str, &'static str) {
    match code {
        Some("unresumable") => (
            "unresumable",
            "Required resume evidence could not be validated.",
        ),
        Some("provider_incompatible") => (
            "provider_incompatible",
            "The recorded provider capability is unavailable.",
        ),
        Some("launch_reconciliation_required") => (
            "launch_reconciliation_required",
            "An earlier recovery launch requires reconciliation.",
        ),
        Some("checkout_collision") => (
            "checkout_collision",
            "Checkout recovery evidence conflicts.",
        ),
        _ => ("recovery_conflict", "Recovery evidence conflicts."),
    }
}

struct ReadinessRow {
    purpose: protocol::CheckoutPurpose,
    isolation_generation: u64,
    reconciliation_generation: u64,
    observed_at: OffsetDateTime,
}

struct ManagedSessionRow {
    provider: String,
    checkout_id: Option<String>,
    role: String,
    managed_status: String,
    managed_until: Option<String>,
    epic_id: Option<String>,
    feature_id: Option<String>,
    work_item_id: Option<String>,
    restore_added_at: Option<String>,
    restore_removed_at: Option<String>,
    restore_remove_reason: Option<String>,
    live_status: Option<String>,
    live_observed_at: Option<String>,
    live_expires_at: Option<String>,
    checkout_availability: Option<String>,
    snapshot_json: Option<String>,
    source_count: usize,
}

fn repository_reference(repository: &core::Repository) -> protocol::RepositoryReference {
    protocol::RepositoryReference {
        id: repository_id(repository.id),
        workspace_id: protocol::WorkspaceId::from_uuid(*repository.workspace_id.as_uuid()),
        slug: repository.slug.to_string(),
        title: repository.title.clone(),
    }
}

fn observed_display_path(
    path: &Path,
    observed_from: OffsetDateTime,
    observed_until: Option<OffsetDateTime>,
) -> protocol::ObservedDisplayPath {
    let names = path
        .components()
        .rev()
        .filter_map(|component| component.as_os_str().to_str())
        .take(2)
        .collect::<Vec<_>>();
    let display_path = names.into_iter().rev().collect::<Vec<_>>().join("/");
    protocol::ObservedDisplayPath {
        display_path: if display_path.is_empty() {
            "<unavailable>".to_owned()
        } else {
            display_path
        },
        state: if observed_until.is_some() {
            protocol::EvidenceState::Historical
        } else {
            protocol::EvidenceState::Current
        },
        observed_from: timestamp(observed_from),
        observed_until: observed_until.map(timestamp),
    }
}

fn evidence(
    state: protocol::EvidenceState,
    code: &str,
    message: &str,
    observed_at: Option<OffsetDateTime>,
) -> protocol::ClassifiedEvidence {
    protocol::ClassifiedEvidence {
        state,
        code: code.to_owned(),
        message: message.to_owned(),
        observed_at: observed_at.map(timestamp),
    }
}

fn collision_evidence(
    availability: core::CheckoutAvailability,
    observed_at: Option<OffsetDateTime>,
) -> protocol::ClassifiedEvidence {
    if availability == core::CheckoutAvailability::Replaced {
        evidence(
            protocol::EvidenceState::Conflict,
            "checkout_replaced",
            "This checkout was replaced by authoritative reconciliation.",
            observed_at,
        )
    } else {
        evidence(
            protocol::EvidenceState::Unknown,
            "collision_evidence_unknown",
            "No authoritative collision scan is loaded.",
            observed_at,
        )
    }
}

fn repository_id(id: core::RepositoryId) -> protocol::RepositoryId {
    protocol::RepositoryId::from_uuid(*id.as_uuid())
}

fn feature_id(id: core::FeatureId) -> protocol::FeatureId {
    protocol::FeatureId::from_uuid(*id.as_uuid())
}

fn work_item_id(id: core::WorkItemId) -> protocol::WorkItemId {
    protocol::WorkItemId::from_uuid(*id.as_uuid())
}

fn checkout_id(id: core::CheckoutId) -> protocol::CheckoutId {
    protocol::CheckoutId::from_uuid(*id.as_uuid())
}

fn session_id(id: core::ConversationId) -> protocol::SessionId {
    protocol::SessionId::from_uuid(*id.as_uuid())
}

fn checkout_availability(value: core::CheckoutAvailability) -> protocol::CheckoutAvailability {
    match value {
        core::CheckoutAvailability::Available => protocol::CheckoutAvailability::Available,
        core::CheckoutAvailability::Missing => protocol::CheckoutAvailability::Missing,
        core::CheckoutAvailability::Deleted => protocol::CheckoutAvailability::Deleted,
        core::CheckoutAvailability::Replaced => protocol::CheckoutAvailability::Replaced,
    }
}

fn checkout_purpose(value: &str) -> Result<protocol::CheckoutPurpose, AppError> {
    match value {
        "feature_integration" => Ok(protocol::CheckoutPurpose::FeatureIntegration),
        "work_item_write" => Ok(protocol::CheckoutPurpose::WorkItemWrite),
        "writer_session" => Ok(protocol::CheckoutPurpose::WriterSession),
        "read_only_shared" => Ok(protocol::CheckoutPurpose::ReadOnlyShared),
        _ => Err(AppError::Domain("invalid checkout purpose".to_owned())),
    }
}

fn provider(value: &str) -> Result<protocol::Provider, AppError> {
    match value {
        "claude" => Ok(protocol::Provider::Claude),
        "codex" => Ok(protocol::Provider::Codex),
        _ => Err(AppError::Domain("invalid session provider".to_owned())),
    }
}

fn session_role(value: &str) -> Result<protocol::ManagedSessionRole, AppError> {
    match value {
        "epic_navigation" => Ok(protocol::ManagedSessionRole::EpicNavigation),
        "feature_planning" => Ok(protocol::ManagedSessionRole::FeaturePlanning),
        "work_item_execution" => Ok(protocol::ManagedSessionRole::WorkItemExecution),
        "debugging" => Ok(protocol::ManagedSessionRole::Debugging),
        "review" => Ok(protocol::ManagedSessionRole::Review),
        _ => Err(AppError::Domain("invalid managed session role".to_owned())),
    }
}

fn parse_owner(
    epic_id: Option<String>,
    feature_value: Option<String>,
    work_item_value: Option<String>,
) -> Result<protocol::OwnerProjection, AppError> {
    match (epic_id, feature_value, work_item_value) {
        (Some(id), None, None) => parse_id::<core::EpicId>(&id)
            .map(|id| protocol::OwnerProjection::Epic(protocol::EpicId::from_uuid(*id.as_uuid()))),
        (None, Some(id), None) => parse_id::<core::FeatureId>(&id)
            .map(|id| protocol::OwnerProjection::Feature(feature_id(id))),
        (None, None, Some(id)) => parse_id::<core::WorkItemId>(&id)
            .map(|id| protocol::OwnerProjection::WorkItem(work_item_id(id))),
        _ => Err(AppError::Domain(
            "managed session owner is unavailable".to_owned(),
        )),
    }
}

fn owner_work_item_id(owner: protocol::OwnerProjection) -> Option<protocol::WorkItemId> {
    match owner {
        protocol::OwnerProjection::WorkItem(id) => Some(id),
        _ => None,
    }
}

fn owner_entity(owner: protocol::OwnerProjection) -> protocol::EntityRef {
    match owner {
        protocol::OwnerProjection::Epic(id) => protocol::EntityRef::Epic(id),
        protocol::OwnerProjection::Feature(id) => protocol::EntityRef::Feature(id),
        protocol::OwnerProjection::WorkItem(id) => protocol::EntityRef::WorkItem(id),
    }
}

fn binding_state(status: &str, managed_until: Option<&str>) -> protocol::SessionBindingState {
    if managed_until.is_some() || status == "stopped" {
        protocol::SessionBindingState::Stopped
    } else {
        match status {
            "bound" | "adopted" => protocol::SessionBindingState::Current,
            "pending" => protocol::SessionBindingState::Pending,
            _ => protocol::SessionBindingState::ReconciliationRequired,
        }
    }
}

fn liveness(
    status: Option<&str>,
    observed_at: Option<&str>,
    expires_at: Option<&str>,
) -> Result<protocol::SessionLivenessProjection, AppError> {
    let observed = observed_at.map(parse_timestamp).transpose()?;
    let expires = expires_at.map(parse_timestamp).transpose()?;
    let stale = expires.is_some_and(|value| value < OffsetDateTime::now_utc());
    let state = if stale {
        protocol::SessionLiveState::Unknown
    } else {
        match status {
            Some("active") => protocol::SessionLiveState::Active,
            Some("idle") => protocol::SessionLiveState::Idle,
            Some("stopped") => protocol::SessionLiveState::Stopped,
            Some("unknown") => protocol::SessionLiveState::Unknown,
            Some("system_error") => protocol::SessionLiveState::SystemError,
            Some("not_loaded") | None => protocol::SessionLiveState::NotLoaded,
            Some(_) => protocol::SessionLiveState::SystemError,
        }
    };
    let evidence_state = if stale {
        protocol::EvidenceState::Stale
    } else if status.is_none() {
        protocol::EvidenceState::NotLoaded
    } else {
        protocol::EvidenceState::Current
    };
    Ok(protocol::SessionLivenessProjection {
        state,
        stale,
        observed_at: observed.map(timestamp),
        expires_at: expires.map(timestamp),
        evidence: evidence(
            evidence_state,
            if stale {
                "liveness_evidence_stale"
            } else if status.is_none() {
                "liveness_not_loaded"
            } else {
                "liveness_observed"
            },
            if stale {
                "The latest liveness evidence has expired; current state is unknown."
            } else if status.is_none() {
                "No authoritative liveness evidence is loaded."
            } else {
                "The liveness state is backed by current Workboard evidence."
            },
            observed,
        ),
    })
}

fn last_activity(snapshot_json: Option<&str>) -> Option<String> {
    snapshot_json
        .and_then(|snapshot| serde_json::from_str::<Value>(snapshot).ok())
        .and_then(|value| {
            value
                .get("last_activity_at")
                .or_else(|| value.get("lastActivityAt"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

fn diagnostic(code: &str, message: &str, owner: protocol::OwnerProjection) -> protocol::Diagnostic {
    protocol::Diagnostic {
        code: code.to_owned(),
        severity: protocol::ErrorSeverity::Warning,
        message: message.to_owned(),
        owner: Some(owner_entity(owner)),
    }
}

fn parse_optional_id<T>(value: Option<&str>) -> Result<Option<T>, AppError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.map(parse_id).transpose()
}

fn parse_id<T>(value: &str) -> Result<T, AppError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error: T::Err| AppError::Domain(error.to_string()))
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, AppError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| AppError::Domain(error.to_string()))
}

fn parse_timestamp_sql(value: &str) -> rusqlite::Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(to_sql_error)
}

fn to_sql_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .expect("RFC 3339 timestamps always format")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liveness_states_remain_distinct_and_absence_is_not_stopped() {
        let observed_at = "2999-08-31T12:00:00Z";
        let expires_at = "2999-08-31T12:05:00Z";
        let cases = [
            ("active", protocol::SessionLiveState::Active),
            ("idle", protocol::SessionLiveState::Idle),
            ("stopped", protocol::SessionLiveState::Stopped),
            ("unknown", protocol::SessionLiveState::Unknown),
            ("system_error", protocol::SessionLiveState::SystemError),
            ("not_loaded", protocol::SessionLiveState::NotLoaded),
        ];
        for (status, expected) in cases {
            assert_eq!(
                liveness(Some(status), Some(observed_at), Some(expires_at))
                    .expect("liveness")
                    .state,
                expected
            );
        }
        assert_eq!(
            liveness(None, None, None).expect("missing liveness").state,
            protocol::SessionLiveState::NotLoaded
        );
    }

    #[test]
    fn stale_liveness_becomes_unknown_without_erasing_observed_state() {
        let projection = liveness(
            Some("active"),
            Some("2020-01-01T12:00:00Z"),
            Some("2020-01-01T12:05:00Z"),
        )
        .expect("stale liveness");
        assert_eq!(projection.state, protocol::SessionLiveState::Unknown);
        assert!(projection.stale);
        assert_eq!(projection.evidence.state, protocol::EvidenceState::Stale);
        assert_eq!(
            projection.observed_at.as_deref(),
            Some("2020-01-01T12:00:00Z")
        );
    }

    #[test]
    fn paths_preserve_current_and_historical_evidence_without_exposing_roots() {
        let current = observed_display_path(
            Path::new("C:/private/repos/current"),
            OffsetDateTime::UNIX_EPOCH,
            None,
        );
        let historical = observed_display_path(
            Path::new("C:/private/worktrees/replaced"),
            OffsetDateTime::UNIX_EPOCH,
            Some(OffsetDateTime::UNIX_EPOCH),
        );
        assert_eq!(current.display_path, "repos/current");
        assert_eq!(current.state, protocol::EvidenceState::Current);
        assert_eq!(historical.display_path, "worktrees/replaced");
        assert_eq!(historical.state, protocol::EvidenceState::Historical);
    }

    #[test]
    fn checkout_collision_classification_preserves_replacement_uncertainty() {
        assert_eq!(
            collision_evidence(core::CheckoutAvailability::Replaced, None).state,
            protocol::EvidenceState::Conflict
        );
        for availability in [
            core::CheckoutAvailability::Available,
            core::CheckoutAvailability::Missing,
            core::CheckoutAvailability::Deleted,
        ] {
            assert_eq!(
                collision_evidence(availability, None).state,
                protocol::EvidenceState::Unknown
            );
        }
    }
}
