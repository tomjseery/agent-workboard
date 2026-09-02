use std::collections::{BTreeMap, HashMap, HashSet};
use workboard_client_protocol as protocol;
use workboard_core as core;

use crate::AppError;
use crate::workspace::WorkboardApplication;

#[derive(Default)]
struct ProjectionEvidence {
    dependencies: HashMap<core::WorkItemId, Vec<core::WorkItemId>>,
    proposal_statuses: HashMap<core::FeatureId, String>,
    checkpointed: HashSet<core::WorkItemId>,
    interrupted: HashSet<core::WorkItemId>,
    recovery_conflicts: HashSet<core::WorkItemId>,
    live_statuses: HashMap<core::ConversationId, String>,
    revisions: HashMap<core::WorkItemId, u64>,
}

struct CardSeed {
    card: protocol::BoardCardProjection,
    group_key: String,
}

impl WorkboardApplication {
    pub fn client_board(
        &self,
        workspace_id: core::WorkspaceId,
        query: protocol::BoardQuery,
    ) -> Result<protocol::BoardPage, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let mut cards = self.board_cards(workspace_id, revision)?;
        apply_board_filters(&mut cards, &query);
        sort_cards(&mut cards, query.sort);
        assign_lane_positions(&mut cards);
        let lanes = board_lanes(&cards, &query.lane_keys);
        let offset = parse_cursor(query.cursor.as_deref(), "board", revision)?;
        let limit = page_limit(query.limit)?;
        let total_count = cards.len();
        let cards = cards
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|seed| seed.card)
            .collect::<Vec<_>>();
        let next_offset = offset.saturating_add(cards.len());
        Ok(protocol::BoardPage {
            lanes,
            cards,
            next_cursor: (next_offset < total_count)
                .then(|| format!("board:{revision}:{next_offset}")),
            total_count,
            revision,
        })
    }

    pub fn client_attention(
        &self,
        workspace_id: core::WorkspaceId,
        query: protocol::AttentionQuery,
    ) -> Result<protocol::AttentionPage, AppError> {
        let revision = self.projection_revision(workspace_id)?;
        let snapshot = self.snapshot(workspace_id)?;
        let evidence = self.projection_evidence(workspace_id)?;
        let repository_lookup = repository_lookup(&snapshot);
        let mut entries = Vec::new();
        let cards = self.board_cards_from(&snapshot, &evidence, revision)?;
        for seed in cards {
            if seed.card.attention_reasons.is_empty() {
                continue;
            }
            entries.push(protocol::AttentionEntryProjection {
                owner: protocol::EntityRef::WorkItem(work_item_id(seed.card.work_item.id)),
                title: seed.card.work_item.title.clone(),
                subtitle: seed.card.work_item.key.clone(),
                repositories: seed.card.repositories.clone(),
                reasons: seed.card.attention_reasons.clone(),
                revision: seed.card.revision,
                available_actions: seed.card.available_actions.clone(),
                card: Some(seed.card),
                position: 0,
                total_count: 0,
            });
        }
        let feature_repositories = feature_repositories(&snapshot, &repository_lookup);
        for feature in &snapshot.features {
            let mut reasons = feature_attention_reasons(feature, &evidence);
            if reasons.is_empty() {
                continue;
            }
            reasons.sort_by_key(|reason| reason.rank);
            entries.push(protocol::AttentionEntryProjection {
                owner: protocol::EntityRef::Feature(feature_id(feature.id)),
                title: feature.title.clone(),
                subtitle: feature.slug.to_string(),
                repositories: feature_repositories
                    .get(&feature.id)
                    .cloned()
                    .unwrap_or_default(),
                card: None,
                reasons,
                revision,
                available_actions: unavailable_actions(revision),
                position: 0,
                total_count: 0,
            });
        }
        entries.retain(|entry| {
            (query.repository_ids.is_empty()
                || entry
                    .repositories
                    .iter()
                    .any(|repository| query.repository_ids.contains(&repository.id)))
                && (query.reason_codes.is_empty()
                    || entry
                        .reasons
                        .iter()
                        .any(|reason| query.reason_codes.contains(&reason.code)))
        });
        entries.sort_by(|left, right| {
            left.reasons[0]
                .rank
                .cmp(&right.reasons[0].rank)
                .then_with(|| left.subtitle.cmp(&right.subtitle))
                .then_with(|| entity_key(&left.owner).cmp(&entity_key(&right.owner)))
        });
        let total_count = entries.len();
        for (position, entry) in entries.iter_mut().enumerate() {
            entry.position = position + 1;
            entry.total_count = total_count;
        }
        let offset = parse_cursor(query.cursor.as_deref(), "attention", revision)?;
        let limit = page_limit(query.limit)?;
        let entries = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset.saturating_add(entries.len());
        Ok(protocol::AttentionPage {
            entries,
            next_cursor: (next_offset < total_count)
                .then(|| format!("attention:{revision}:{next_offset}")),
            total_count,
            revision,
        })
    }

    fn board_cards(
        &self,
        workspace_id: core::WorkspaceId,
        revision: u64,
    ) -> Result<Vec<CardSeed>, AppError> {
        let snapshot = self.snapshot(workspace_id)?;
        let evidence = self.projection_evidence(workspace_id)?;
        self.board_cards_from(&snapshot, &evidence, revision)
    }

    fn board_cards_from(
        &self,
        snapshot: &core::WorkspaceSnapshot,
        evidence: &ProjectionEvidence,
        revision: u64,
    ) -> Result<Vec<CardSeed>, AppError> {
        let repositories = repository_lookup(snapshot);
        let features = snapshot
            .features
            .iter()
            .map(|feature| (feature.id, feature))
            .collect::<HashMap<_, _>>();
        let work_items = snapshot
            .work_items
            .iter()
            .map(|item| (item.id, item))
            .collect::<HashMap<_, _>>();
        let sessions = snapshot
            .sessions
            .iter()
            .map(|session| (session.id, session))
            .collect::<HashMap<_, _>>();
        let mut session_ids = HashMap::<core::WorkItemId, Vec<core::ConversationId>>::new();
        for association in snapshot
            .associations
            .iter()
            .filter(|association| association.associated_until.is_none())
        {
            if let core::HierarchyOwner::WorkItem(id) = association.owner {
                session_ids
                    .entry(id)
                    .or_default()
                    .push(association.session_id);
            }
        }
        let mut seeds = Vec::with_capacity(snapshot.work_items.len());
        for item in &snapshot.work_items {
            let feature = features
                .get(&item.feature_id)
                .ok_or_else(|| AppError::Domain("Work item Feature does not exist".to_owned()))?;
            let blockers = evidence
                .dependencies
                .get(&item.id)
                .into_iter()
                .flatten()
                .filter_map(|id| work_items.get(id).copied())
                .filter(|dependency| !is_complete(dependency.status))
                .collect::<Vec<_>>();
            let dependency_readiness = dependency_readiness(item.status, blockers.is_empty());
            let group_key = if blockers.is_empty() {
                format!("feature:{}:ready", item.feature_id)
            } else {
                let mut ids = blockers
                    .iter()
                    .map(|dependency| dependency.id.to_string())
                    .collect::<Vec<_>>();
                ids.sort();
                format!("feature:{}:{}", item.feature_id, ids.join("+"))
            };
            let session_summary = session_summary(
                session_ids
                    .get(&item.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &sessions,
                evidence,
            );
            let mut card_session_ids = session_ids.get(&item.id).cloned().unwrap_or_default();
            card_session_ids.sort_by_key(ToString::to_string);
            let mut checkout_ids = snapshot
                .effective_checkouts
                .iter()
                .filter(|checkout| checkout.work_item_id == Some(item.id))
                .map(|checkout| checkout_id(checkout.checkout_id))
                .collect::<Vec<_>>();
            checkout_ids.sort_by_key(ToString::to_string);
            let mut reasons =
                work_item_attention_reasons(item, dependency_readiness, &session_summary, evidence);
            reasons.sort_by_key(|reason| reason.rank);
            let mut repository_scope = item
                .repository_ids
                .iter()
                .filter_map(|id| repositories.get(id).cloned())
                .collect::<Vec<_>>();
            repository_scope.sort_by(|left, right| left.slug.cmp(&right.slug));
            seeds.push(CardSeed {
                group_key,
                card: protocol::BoardCardProjection {
                    work_item: work_item_reference(item),
                    feature: protocol::FeatureReference {
                        id: feature_id(feature.id),
                        epic_id: epic_id(feature.epic_id),
                        slug: feature.slug.to_string(),
                        title: feature.title.clone(),
                    },
                    status: work_item_status(item.status),
                    lane_key: status_key(item.status).to_owned(),
                    lane_position: 0,
                    lane_count: 0,
                    dependency_readiness,
                    blocked_by: blockers
                        .into_iter()
                        .map(|dependency| protocol::BlockedByEvidence {
                            work_item: work_item_reference(dependency),
                            status: work_item_status(dependency.status),
                        })
                        .collect(),
                    parallel_readiness: protocol::ParallelReadiness {
                        group_key: String::new(),
                        ready_count: 0,
                        waiting_count: 0,
                    },
                    repositories: repository_scope,
                    session_summary,
                    checkout_ids,
                    session_ids: card_session_ids.into_iter().map(session_id).collect(),
                    attention_reasons: reasons,
                    revision: evidence
                        .revisions
                        .get(&item.id)
                        .copied()
                        .unwrap_or(revision),
                    available_actions: unavailable_actions(revision),
                },
            });
        }
        apply_parallel_readiness(&mut seeds);
        Ok(seeds)
    }

    fn projection_evidence(
        &self,
        workspace_id: core::WorkspaceId,
    ) -> Result<ProjectionEvidence, AppError> {
        self.store.read(|connection| {
            let mut evidence = ProjectionEvidence::default();
            let scope = workspace_id.to_string();
            let mut dependencies = connection.prepare(
                "SELECT dependency.work_item_id, dependency.dependency_work_item_id
                 FROM work_item_dependencies dependency
                 JOIN work_items item ON item.id = dependency.work_item_id
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE epic.workspace_id = ?1",
            )?;
            for row in dependencies.query_map([&scope], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })? {
                let (item, dependency) = row?;
                evidence
                    .dependencies
                    .entry(parse_work_item_id(&item)?)
                    .or_default()
                    .push(parse_work_item_id(&dependency)?);
            }
            let mut proposals = connection.prepare(
                "SELECT proposal.feature_id, proposal.status
                 FROM feature_planning_proposals proposal
                 JOIN features feature ON feature.id = proposal.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE epic.workspace_id = ?1",
            )?;
            for row in proposals.query_map([&scope], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })? {
                let (feature, status) = row?;
                evidence.proposal_statuses.insert(parse_feature_id(&feature)?, status);
            }
            collect_work_item_ids(connection, &scope, "SELECT DISTINCT checkpoint.work_item_id FROM work_item_checkpoints checkpoint JOIN work_items item ON item.id = checkpoint.work_item_id JOIN features feature ON feature.id = item.feature_id JOIN epics epic ON epic.id = feature.epic_id WHERE epic.workspace_id = ?1", &mut evidence.checkpointed)?;
            collect_work_item_ids(connection, &scope, "SELECT DISTINCT request.work_item_id FROM managed_session_requests request JOIN work_items item ON item.id = request.work_item_id JOIN features feature ON feature.id = item.feature_id JOIN epics epic ON epic.id = feature.epic_id WHERE epic.workspace_id = ?1 AND request.status = 'failed'", &mut evidence.interrupted)?;
            collect_work_item_ids(connection, &scope, "SELECT DISTINCT readiness.owner_id FROM checkout_readiness readiness JOIN work_items item ON item.id = readiness.owner_id JOIN features feature ON feature.id = item.feature_id JOIN epics epic ON epic.id = feature.epic_id WHERE epic.workspace_id = ?1 AND readiness.owner_kind = 'work_item' AND readiness.availability <> 'available'", &mut evidence.recovery_conflicts)?;
            let mut live = connection.prepare(
                "SELECT association.session_id,
                        CASE
                            WHEN observation.id IS NULL OR
                                 strftime('%s', observation.expires_at) <= strftime('%s', 'now')
                            THEN 'unknown'
                            ELSE observation.status
                        END
                 FROM native_session_associations association
                 LEFT JOIN live_observations observation ON observation.id = (
                     SELECT candidate.id FROM live_observations candidate
                     WHERE candidate.session_id = association.session_id
                     ORDER BY candidate.observed_at DESC LIMIT 1
                 )
                 JOIN work_items item ON item.id = association.work_item_id
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 WHERE epic.workspace_id = ?1 AND association.associated_until IS NULL",
            )?;
            for row in live.query_map([&scope], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })? {
                let (session, status) = row?;
                evidence
                    .live_statuses
                    .insert(parse_conversation_id(&session)?, status);
            }
            let mut revisions = connection.prepare(
                "SELECT item.id, COALESCE(MAX(revision.revision), 1)
                 FROM work_items item
                 JOIN features feature ON feature.id = item.feature_id
                 JOIN epics epic ON epic.id = feature.epic_id
                 JOIN documents document ON document.work_item_id = item.id
                 LEFT JOIN document_revisions revision ON revision.document_id = document.id
                 WHERE epic.workspace_id = ?1 GROUP BY item.id",
            )?;
            for row in revisions.query_map([&scope], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })? {
                let (item, revision) = row?;
                evidence.revisions.insert(parse_work_item_id(&item)?, revision as u64);
            }
            Ok(evidence)
        })
    }
}

fn collect_work_item_ids(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    sql: &str,
    target: &mut HashSet<core::WorkItemId>,
) -> Result<(), AppError> {
    let mut statement = connection.prepare(sql)?;
    for row in statement.query_map([workspace_id], |row| row.get::<_, String>(0))? {
        target.insert(parse_work_item_id(&row?)?);
    }
    Ok(())
}

fn apply_board_filters(cards: &mut Vec<CardSeed>, query: &protocol::BoardQuery) {
    let text = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);
    cards.retain(|seed| {
        let card = &seed.card;
        query.lane_keys.is_empty() || query.lane_keys.contains(&card.lane_key)
    });
    cards.retain(|seed| {
        let card = &seed.card;
        query.repository_ids.is_empty()
            || card
                .repositories
                .iter()
                .any(|repository| query.repository_ids.contains(&repository.id))
    });
    cards.retain(|seed| {
        query.feature_ids.is_empty() || query.feature_ids.contains(&seed.card.feature.id)
    });
    cards.retain(|seed| query.statuses.is_empty() || query.statuses.contains(&seed.card.status));
    if let Some(text) = text {
        cards.retain(|seed| {
            let card = &seed.card;
            card.work_item.title.to_lowercase().contains(&text)
                || card.work_item.key.to_lowercase().contains(&text)
                || card.work_item.slug.to_lowercase().contains(&text)
                || card.feature.title.to_lowercase().contains(&text)
        });
    }
}

fn sort_cards(cards: &mut [CardSeed], sort: protocol::BoardViewSort) {
    cards.sort_by(|left, right| {
        status_position(left.card.status)
            .cmp(&status_position(right.card.status))
            .then_with(|| {
                let ordering = match sort.field {
                    protocol::BoardViewSortField::Title => {
                        left.card.work_item.title.cmp(&right.card.work_item.title)
                    }
                    protocol::BoardViewSortField::Key => {
                        left.card.work_item.key.cmp(&right.card.work_item.key)
                    }
                };
                match sort.direction {
                    protocol::BoardViewSortDirection::Ascending => ordering,
                    protocol::BoardViewSortDirection::Descending => ordering.reverse(),
                }
            })
            .then_with(|| {
                left.card
                    .work_item
                    .id
                    .to_string()
                    .cmp(&right.card.work_item.id.to_string())
            })
    });
}

fn assign_lane_positions(cards: &mut [CardSeed]) {
    let mut counts = BTreeMap::<String, usize>::new();
    for seed in cards.iter() {
        *counts.entry(seed.card.lane_key.clone()).or_default() += 1;
    }
    let mut positions = HashMap::<String, usize>::new();
    for seed in cards {
        let position = positions.entry(seed.card.lane_key.clone()).or_default();
        *position += 1;
        seed.card.lane_position = *position;
        seed.card.lane_count = counts[&seed.card.lane_key];
    }
}

fn apply_parallel_readiness(seeds: &mut [CardSeed]) {
    let mut groups = HashMap::<String, (usize, usize)>::new();
    for seed in seeds.iter() {
        let counts = groups.entry(seed.group_key.clone()).or_default();
        if seed.card.dependency_readiness == protocol::DependencyReadiness::Ready {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }
    }
    for seed in seeds {
        let (ready_count, waiting_count) = groups[&seed.group_key];
        seed.card.parallel_readiness = protocol::ParallelReadiness {
            group_key: seed.group_key.clone(),
            ready_count,
            waiting_count,
        };
    }
}

fn board_lanes(cards: &[CardSeed], requested: &[String]) -> Vec<protocol::BoardLaneProjection> {
    let mut counts = HashMap::<String, usize>::new();
    for seed in cards {
        *counts.entry(seed.card.lane_key.clone()).or_default() += 1;
    }
    let keys = if requested.is_empty() {
        vec![
            "backlog",
            "ready",
            "in_progress",
            "blocked",
            "review",
            "done",
            "cancelled",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    } else {
        requested.to_vec()
    };
    keys.into_iter()
        .enumerate()
        .map(|(position, key)| protocol::BoardLaneProjection {
            title: lane_title(&key),
            total_count: counts.get(&key).copied().unwrap_or_default(),
            key,
            position: position + 1,
        })
        .collect()
}

fn session_summary(
    ids: &[core::ConversationId],
    sessions: &HashMap<core::ConversationId, &core::NativeSession>,
    evidence: &ProjectionEvidence,
) -> protocol::SessionSummary {
    let mut providers = Vec::new();
    let mut active = 0;
    let mut idle = 0;
    let mut unknown = 0;
    for id in ids {
        if let Some(session) = sessions.get(id) {
            let provider = match session.native.tool() {
                core::Tool::Claude => protocol::Provider::Claude,
                core::Tool::Codex => protocol::Provider::Codex,
            };
            if !providers.contains(&provider) {
                providers.push(provider);
            }
        }
        match evidence.live_statuses.get(id).map(String::as_str) {
            Some("active") => active += 1,
            Some("idle") => idle += 1,
            _ => unknown += 1,
        }
    }
    protocol::SessionSummary {
        total: ids.len(),
        active,
        idle,
        unknown,
        providers,
    }
}

fn work_item_attention_reasons(
    item: &core::WorkItem,
    readiness: protocol::DependencyReadiness,
    sessions: &protocol::SessionSummary,
    evidence: &ProjectionEvidence,
) -> Vec<protocol::AttentionReason> {
    let mut codes = Vec::new();
    if item.status == core::WorkItemStatus::Blocked
        || matches!(
            readiness,
            protocol::DependencyReadiness::Blocked | protocol::DependencyReadiness::Waiting
        )
    {
        codes.push(protocol::AttentionReasonCode::Blocked);
    }
    if item.status == core::WorkItemStatus::InProgress && !evidence.checkpointed.contains(&item.id)
    {
        codes.push(protocol::AttentionReasonCode::CheckpointDue);
    }
    if evidence.interrupted.contains(&item.id) {
        codes.push(protocol::AttentionReasonCode::InterruptedOperation);
    }
    if evidence.recovery_conflicts.contains(&item.id) {
        codes.push(protocol::AttentionReasonCode::RecoveryConflict);
    }
    if sessions.unknown > 0 {
        codes.push(protocol::AttentionReasonCode::StaleOrUnknownSession);
    }
    codes.into_iter().map(attention_reason).collect()
}

fn feature_attention_reasons(
    feature: &core::Feature,
    evidence: &ProjectionEvidence,
) -> Vec<protocol::AttentionReason> {
    let mut codes = Vec::new();
    match evidence
        .proposal_statuses
        .get(&feature.id)
        .map(String::as_str)
    {
        Some("awaiting_approval") => codes.push(protocol::AttentionReasonCode::ApprovalRequired),
        Some("rejected") => codes.push(protocol::AttentionReasonCode::RevisionRequested),
        _ => {}
    }
    if feature.state == core::WorkflowState::ReconciliationRequired {
        codes.push(protocol::AttentionReasonCode::ReconciliationRequired);
    }
    codes.into_iter().map(attention_reason).collect()
}

fn attention_reason(code: protocol::AttentionReasonCode) -> protocol::AttentionReason {
    let (rank, message) = match code {
        protocol::AttentionReasonCode::ApprovalRequired => (1, "Approval required"),
        protocol::AttentionReasonCode::RevisionRequested => (2, "Revision requested"),
        protocol::AttentionReasonCode::ReconciliationRequired => (3, "Reconciliation required"),
        protocol::AttentionReasonCode::Blocked => {
            (4, "Blocked by authoritative dependency evidence")
        }
        protocol::AttentionReasonCode::CheckpointDue => (5, "Checkpoint evidence is due"),
        protocol::AttentionReasonCode::InterruptedOperation => (6, "An operation was interrupted"),
        protocol::AttentionReasonCode::RecoveryConflict => (7, "Recovery evidence conflicts"),
        protocol::AttentionReasonCode::StaleOrUnknownSession => {
            (8, "Session evidence is stale or unknown")
        }
    };
    protocol::AttentionReason {
        code,
        rank,
        message: message.to_owned(),
    }
}

fn unavailable_actions(revision: u64) -> Vec<protocol::AvailableAction> {
    protocol::CommandCode::ALL
        .into_iter()
        .map(|code| protocol::AvailableAction {
            code,
            available: false,
            unavailable_reason: Some(protocol::UnavailableReason {
                code: "upstream_capability_not_accepted".to_owned(),
                message: "the authoritative Workboard operation has not been accepted".to_owned(),
            }),
            expected_revision: Some(revision),
        })
        .collect()
}

fn repository_lookup(
    snapshot: &core::WorkspaceSnapshot,
) -> HashMap<core::RepositoryId, protocol::RepositoryReference> {
    snapshot
        .repositories
        .iter()
        .map(|repository| {
            (
                repository.id,
                protocol::RepositoryReference {
                    id: repository_id(repository.id),
                    workspace_id: workspace_id(repository.workspace_id),
                    slug: repository.slug.to_string(),
                    title: repository.title.clone(),
                },
            )
        })
        .collect()
}

fn feature_repositories(
    snapshot: &core::WorkspaceSnapshot,
    repositories: &HashMap<core::RepositoryId, protocol::RepositoryReference>,
) -> HashMap<core::FeatureId, Vec<protocol::RepositoryReference>> {
    let mut ids = HashMap::<core::FeatureId, HashSet<core::RepositoryId>>::new();
    for item in &snapshot.work_items {
        ids.entry(item.feature_id)
            .or_default()
            .extend(item.repository_ids.iter().copied());
    }
    ids.into_iter()
        .map(|(feature, ids)| {
            let mut scope = ids
                .into_iter()
                .filter_map(|id| repositories.get(&id).cloned())
                .collect::<Vec<_>>();
            scope.sort_by(|left, right| left.slug.cmp(&right.slug));
            (feature, scope)
        })
        .collect()
}

fn work_item_reference(item: &core::WorkItem) -> protocol::WorkItemReference {
    protocol::WorkItemReference {
        id: work_item_id(item.id),
        feature_id: feature_id(item.feature_id),
        key: item.key.to_string(),
        slug: item.slug.to_string(),
        title: item.title.clone(),
    }
}

fn parse_cursor(cursor: Option<&str>, kind: &str, revision: u64) -> Result<usize, AppError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let valid_kind = parts.next() == Some(kind);
    let valid_revision = parts.next().and_then(|value| value.parse::<u64>().ok()) == Some(revision);
    let offset = parts.next().and_then(|value| value.parse::<usize>().ok());
    if !valid_kind || !valid_revision || parts.next().is_some() {
        return Err(AppError::External {
            code: "stale_projection_cursor".to_owned(),
            message: "the projection cursor is stale or invalid".to_owned(),
        });
    }
    offset.ok_or_else(|| AppError::Domain("projection cursor offset is invalid".to_owned()))
}

fn page_limit(limit: usize) -> Result<usize, AppError> {
    if (1..=protocol::MAX_QUERY_PAGE_ITEMS).contains(&limit) {
        Ok(limit)
    } else {
        Err(AppError::Domain(
            "projection page limit must be between 1 and 500".to_owned(),
        ))
    }
}

fn entity_key(owner: &protocol::EntityRef) -> String {
    match owner {
        protocol::EntityRef::Workspace(id) => id.to_string(),
        protocol::EntityRef::Repository(id) => id.to_string(),
        protocol::EntityRef::Epic(id) => id.to_string(),
        protocol::EntityRef::Feature(id) => id.to_string(),
        protocol::EntityRef::WorkItem(id) => id.to_string(),
        protocol::EntityRef::Session(id) => id.to_string(),
    }
}

fn status_key(status: core::WorkItemStatus) -> &'static str {
    match status {
        core::WorkItemStatus::Backlog => "backlog",
        core::WorkItemStatus::Ready => "ready",
        core::WorkItemStatus::InProgress => "in_progress",
        core::WorkItemStatus::Blocked => "blocked",
        core::WorkItemStatus::Review => "review",
        core::WorkItemStatus::Done => "done",
        core::WorkItemStatus::Cancelled => "cancelled",
    }
}
fn lane_title(key: &str) -> String {
    match key {
        "backlog" => "Backlog",
        "ready" => "Ready",
        "in_progress" => "In progress",
        "blocked" => "Blocked",
        "review" => "Review",
        "done" => "Done",
        "cancelled" => "Cancelled",
        other => other,
    }
    .to_owned()
}
fn status_position(status: protocol::WorkItemStatus) -> usize {
    match status {
        protocol::WorkItemStatus::Backlog => 0,
        protocol::WorkItemStatus::Ready => 1,
        protocol::WorkItemStatus::InProgress => 2,
        protocol::WorkItemStatus::Blocked => 3,
        protocol::WorkItemStatus::Review => 4,
        protocol::WorkItemStatus::Done => 5,
        protocol::WorkItemStatus::Cancelled => 6,
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
fn parse_uuid(value: &str) -> Result<uuid::Uuid, AppError> {
    uuid::Uuid::parse_str(value)
        .map_err(|error| AppError::Domain(format!("invalid identity: {error}")))
}
fn parse_work_item_id(value: &str) -> Result<core::WorkItemId, AppError> {
    Ok(core::WorkItemId::from_uuid(parse_uuid(value)?))
}
fn parse_feature_id(value: &str) -> Result<core::FeatureId, AppError> {
    Ok(core::FeatureId::from_uuid(parse_uuid(value)?))
}
fn parse_conversation_id(value: &str) -> Result<core::ConversationId, AppError> {
    Ok(core::ConversationId::from_uuid(parse_uuid(value)?))
}
fn workspace_id(id: core::WorkspaceId) -> protocol::WorkspaceId {
    protocol::WorkspaceId::from_uuid(*id.as_uuid())
}
fn repository_id(id: core::RepositoryId) -> protocol::RepositoryId {
    protocol::RepositoryId::from_uuid(*id.as_uuid())
}

fn checkout_id(id: core::CheckoutId) -> protocol::CheckoutId {
    protocol::CheckoutId::from_uuid(*id.as_uuid())
}

fn session_id(id: core::ConversationId) -> protocol::SessionId {
    protocol::SessionId::from_uuid(*id.as_uuid())
}
fn epic_id(id: core::EpicId) -> protocol::EpicId {
    protocol::EpicId::from_uuid(*id.as_uuid())
}
fn feature_id(id: core::FeatureId) -> protocol::FeatureId {
    protocol::FeatureId::from_uuid(*id.as_uuid())
}
fn work_item_id(id: impl IntoWorkItemId) -> protocol::WorkItemId {
    protocol::WorkItemId::from_uuid(*id.into_work_item_id().as_uuid())
}

trait IntoWorkItemId {
    fn into_work_item_id(self) -> core::WorkItemId;
}
impl IntoWorkItemId for core::WorkItemId {
    fn into_work_item_id(self) -> core::WorkItemId {
        self
    }
}
impl IntoWorkItemId for protocol::WorkItemId {
    fn into_work_item_id(self) -> core::WorkItemId {
        core::WorkItemId::from_uuid(*self.as_uuid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_id<T>(prefix: u128, index: usize, convert: impl FnOnce(uuid::Uuid) -> T) -> T {
        convert(uuid::Uuid::from_u128((prefix << 120) + index as u128 + 1))
    }

    fn seed(index: usize, status: protocol::WorkItemStatus, group: &str) -> CardSeed {
        let workspace_id = wire_id(2, 0, protocol::WorkspaceId::from_uuid);
        let repository = |offset| protocol::RepositoryReference {
            id: wire_id(3, (index + offset) % 100, protocol::RepositoryId::from_uuid),
            workspace_id,
            slug: format!("service-{:03}", (index + offset) % 100),
            title: format!("Service {}", (index + offset) % 100),
        };
        let feature = index / 10;
        let work_item = protocol::WorkItemReference {
            id: wire_id(6, index, protocol::WorkItemId::from_uuid),
            feature_id: wire_id(5, feature, protocol::FeatureId::from_uuid),
            key: format!("F{feature:04}/WI{}", index % 10),
            slug: format!("work-item-{index}"),
            title: format!("Work item {index}"),
        };
        CardSeed {
            group_key: group.to_owned(),
            card: protocol::BoardCardProjection {
                work_item,
                feature: protocol::FeatureReference {
                    id: wire_id(5, feature, protocol::FeatureId::from_uuid),
                    epic_id: wire_id(4, feature / 10, protocol::EpicId::from_uuid),
                    slug: format!("feature-{feature}"),
                    title: format!("Feature {feature}"),
                },
                status,
                lane_key: match status {
                    protocol::WorkItemStatus::Backlog => "backlog",
                    protocol::WorkItemStatus::Ready => "ready",
                    protocol::WorkItemStatus::InProgress => "in_progress",
                    protocol::WorkItemStatus::Blocked => "blocked",
                    protocol::WorkItemStatus::Review => "review",
                    protocol::WorkItemStatus::Done => "done",
                    protocol::WorkItemStatus::Cancelled => "cancelled",
                }
                .to_owned(),
                lane_position: 0,
                lane_count: 0,
                dependency_readiness: if status == protocol::WorkItemStatus::Ready {
                    protocol::DependencyReadiness::Ready
                } else {
                    protocol::DependencyReadiness::Waiting
                },
                blocked_by: Vec::new(),
                parallel_readiness: protocol::ParallelReadiness {
                    group_key: String::new(),
                    ready_count: 0,
                    waiting_count: 0,
                },
                repositories: vec![repository(0), repository(17)],
                session_summary: protocol::SessionSummary {
                    total: 1,
                    active: usize::from(index.is_multiple_of(2)),
                    idle: 0,
                    unknown: usize::from(!index.is_multiple_of(2)),
                    providers: vec![if index.is_multiple_of(2) {
                        protocol::Provider::Claude
                    } else {
                        protocol::Provider::Codex
                    }],
                },
                checkout_ids: vec![wire_id(7, index, protocol::CheckoutId::from_uuid)],
                session_ids: vec![wire_id(8, index, protocol::SessionId::from_uuid)],
                attention_reasons: vec![attention_reason(match index % 8 {
                    0 => protocol::AttentionReasonCode::ApprovalRequired,
                    1 => protocol::AttentionReasonCode::RevisionRequested,
                    2 => protocol::AttentionReasonCode::ReconciliationRequired,
                    3 => protocol::AttentionReasonCode::Blocked,
                    4 => protocol::AttentionReasonCode::CheckpointDue,
                    5 => protocol::AttentionReasonCode::InterruptedOperation,
                    6 => protocol::AttentionReasonCode::RecoveryConflict,
                    _ => protocol::AttentionReasonCode::StaleOrUnknownSession,
                })],
                revision: index as u64 + 1,
                available_actions: Vec::new(),
            },
        }
    }

    #[test]
    fn dependency_dag_and_parallel_groups_remain_authoritative() {
        assert_eq!(
            dependency_readiness(core::WorkItemStatus::Ready, true),
            protocol::DependencyReadiness::Ready
        );
        assert_eq!(
            dependency_readiness(core::WorkItemStatus::Ready, false),
            protocol::DependencyReadiness::Waiting
        );
        assert_eq!(
            dependency_readiness(core::WorkItemStatus::Blocked, true),
            protocol::DependencyReadiness::Blocked
        );
        assert_eq!(
            dependency_readiness(core::WorkItemStatus::Done, false),
            protocol::DependencyReadiness::Complete
        );
        let mut seeds = vec![
            seed(0, protocol::WorkItemStatus::Ready, "parallel-a"),
            seed(1, protocol::WorkItemStatus::Ready, "parallel-a"),
            seed(2, protocol::WorkItemStatus::Blocked, "parallel-a"),
        ];
        apply_parallel_readiness(&mut seeds);
        assert!(
            seeds
                .iter()
                .all(|seed| seed.card.parallel_readiness.ready_count == 2)
        );
        assert!(
            seeds
                .iter()
                .all(|seed| seed.card.parallel_readiness.waiting_count == 1)
        );
    }

    #[test]
    fn attention_codes_have_one_stable_daemon_order() {
        let codes = [
            protocol::AttentionReasonCode::ApprovalRequired,
            protocol::AttentionReasonCode::RevisionRequested,
            protocol::AttentionReasonCode::ReconciliationRequired,
            protocol::AttentionReasonCode::Blocked,
            protocol::AttentionReasonCode::CheckpointDue,
            protocol::AttentionReasonCode::InterruptedOperation,
            protocol::AttentionReasonCode::RecoveryConflict,
            protocol::AttentionReasonCode::StaleOrUnknownSession,
        ];
        assert_eq!(
            codes.map(attention_reason).map(|reason| reason.rank),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn feature_scope_narrows_a_board_without_collapsing_status_or_repository_scope() {
        let statuses = [
            protocol::WorkItemStatus::Backlog,
            protocol::WorkItemStatus::Ready,
            protocol::WorkItemStatus::InProgress,
            protocol::WorkItemStatus::Blocked,
            protocol::WorkItemStatus::Review,
            protocol::WorkItemStatus::Done,
            protocol::WorkItemStatus::Cancelled,
        ];
        let seeds = || {
            (0..100)
                .map(|index| {
                    seed(
                        index,
                        statuses[index % statuses.len()],
                        &format!("feature-{}-parallel", index / 10),
                    )
                })
                .collect::<Vec<_>>()
        };
        let query =
            |feature_ids: Vec<protocol::FeatureId>,
             repository_ids: Vec<protocol::RepositoryId>,
             statuses: Vec<protocol::WorkItemStatus>| protocol::BoardQuery {
                cursor: None,
                limit: 200,
                query: None,
                repository_ids,
                feature_ids,
                statuses,
                lane_keys: Vec::new(),
                sort: protocol::BoardViewSort {
                    field: protocol::BoardViewSortField::Key,
                    direction: protocol::BoardViewSortDirection::Ascending,
                },
            };

        let feature = wire_id(5, 3, protocol::FeatureId::from_uuid);
        let mut scoped = seeds();
        apply_board_filters(&mut scoped, &query(vec![feature], Vec::new(), Vec::new()));
        assert_eq!(scoped.len(), 10);
        assert!(scoped.iter().all(|seed| seed.card.feature.id == feature));

        let repository = scoped[0].card.repositories[0].id;
        let mut intersected = seeds();
        apply_board_filters(
            &mut intersected,
            &query(vec![feature], vec![repository], Vec::new()),
        );
        assert!(!intersected.is_empty());
        assert!(intersected.iter().all(|seed| {
            seed.card.feature.id == feature
                && seed
                    .card
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository)
        }));

        let mut by_status = seeds();
        apply_board_filters(
            &mut by_status,
            &query(
                vec![feature],
                Vec::new(),
                vec![protocol::WorkItemStatus::Cancelled],
            ),
        );
        assert!(!by_status.is_empty());
        assert!(by_status.iter().all(|seed| seed.card.status
            == protocol::WorkItemStatus::Cancelled
            && seed.card.feature.id == feature));

        let mut unscoped = seeds();
        apply_board_filters(&mut unscoped, &query(Vec::new(), Vec::new(), Vec::new()));
        assert_eq!(unscoped.len(), 100);

        let mut absent = seeds();
        apply_board_filters(
            &mut absent,
            &query(
                vec![wire_id(5, 999, protocol::FeatureId::from_uuid)],
                Vec::new(),
                Vec::new(),
            ),
        );
        assert!(absent.is_empty());
    }

    #[test]
    fn large_projection_fixture_is_deterministic_filtered_paged_and_revisioned() {
        let statuses = [
            protocol::WorkItemStatus::Backlog,
            protocol::WorkItemStatus::Ready,
            protocol::WorkItemStatus::InProgress,
            protocol::WorkItemStatus::Blocked,
            protocol::WorkItemStatus::Review,
            protocol::WorkItemStatus::Done,
            protocol::WorkItemStatus::Cancelled,
        ];
        let mut cards = (0..10_000)
            .map(|index| {
                seed(
                    index,
                    statuses[index % statuses.len()],
                    &format!("feature-{}-parallel", index / 10),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 10_000);
        assert_eq!(
            cards
                .iter()
                .map(|seed| seed.card.feature.id)
                .collect::<HashSet<_>>()
                .len(),
            1_000
        );
        assert_eq!(
            cards
                .iter()
                .flat_map(|seed| seed
                    .card
                    .repositories
                    .iter()
                    .map(|repository| repository.id))
                .collect::<HashSet<_>>()
                .len(),
            100
        );
        assert!(cards.iter().all(|seed| seed.card.repositories.len() == 2));
        let stable_identity = cards[0].card.work_item.id;
        assert_eq!(
            cards
                .iter()
                .filter(|seed| seed.card.work_item.id == stable_identity)
                .count(),
            1
        );
        let repository_id = cards[42].card.repositories[0].id;
        let query = protocol::BoardQuery {
            cursor: None,
            limit: 200,
            query: Some("Work item 42".to_owned()),
            repository_ids: vec![repository_id],
            feature_ids: Vec::new(),
            statuses: vec![statuses[42 % statuses.len()]],
            lane_keys: Vec::new(),
            sort: protocol::BoardViewSort {
                field: protocol::BoardViewSortField::Key,
                direction: protocol::BoardViewSortDirection::Ascending,
            },
        };
        apply_board_filters(&mut cards, &query);
        sort_cards(&mut cards, query.sort);
        assign_lane_positions(&mut cards);
        assert!(!cards.is_empty());
        assert!(cards.iter().all(|seed| {
            seed.card
                .repositories
                .iter()
                .any(|repository| repository.id == repository_id)
        }));
        assert!(cards.iter().all(|seed| seed.card.revision > 0));
        assert_eq!(
            parse_cursor(Some("board:41:200"), "board", 41).expect("valid cursor"),
            200
        );
        assert!(parse_cursor(Some("board:40:200"), "board", 41).is_err());
        assert_eq!(page_limit(500).expect("maximum page"), 500);
        assert!(page_limit(501).is_err());
    }
}
