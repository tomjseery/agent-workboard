use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{
    AVAILABLE_ACTIONS_SCHEMA_VERSION, AvailableAction, AvailableActionKind, AvailableActions,
    CheckoutAccessMode, CheckoutId, CheckoutPurpose, ConversationId, HierarchyOwner, LaunchProfile,
    LiveStatus, ManagedSessionRole, RepositoryId, Resumability, Tool, WorkItem, WorkItemId,
    WorkItemStatus, WorkspaceId,
};

use crate::AppError;
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemDependencyFact {
    pub work_item: WorkItem,
    pub satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemReadiness {
    pub ready: bool,
    pub layer: u32,
    pub dependencies: Vec<WorkItemDependencyFact>,
    pub dependants: Vec<WorkItemId>,
    pub blocked_by: Vec<WorkItemId>,
    pub active: bool,
    pub parallelizable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChoice {
    pub session_id: ConversationId,
    pub binding_generation: u32,
    pub provider: Tool,
    pub profile: LaunchProfile,
    pub role: ManagedSessionRole,
    pub primary_writer: bool,
    pub binding_status: String,
    pub live_status: Option<LiveStatus>,
    pub restore_active: bool,
    pub last_activity: Option<OffsetDateTime>,
    pub checkout_id: CheckoutId,
    pub repository_id: RepositoryId,
    pub checkout_purpose: Option<CheckoutPurpose>,
    pub checkout_path: PathBuf,
    pub branch: Option<String>,
    pub checkout_generation: Option<u64>,
    pub resumability: Resumability,
    pub actions: Vec<AvailableAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemProjection {
    pub schema_version: u32,
    pub workspace_id: WorkspaceId,
    pub work_item: WorkItem,
    pub readiness: WorkItemReadiness,
    pub sessions: Vec<SessionChoice>,
    pub available_actions: AvailableActions,
}

pub struct WorkProjectionService<'a> {
    store: &'a SqliteStore,
}

impl<'a> WorkProjectionService<'a> {
    pub fn new(store: &'a SqliteStore) -> Self {
        Self { store }
    }

    pub fn project(&self, work_item_id: WorkItemId) -> Result<WorkItemProjection, AppError> {
        let (workspace_id, feature_id) = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT epic.workspace_id, item.feature_id
                     FROM work_items item
                     JOIN features feature ON feature.id = item.feature_id
                     JOIN epics epic ON epic.id = feature.epic_id
                     WHERE item.id = ?1",
                    [work_item_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
                .ok_or(AppError::WorkItemNotFound)
        })?;
        let workspace_id = parse_id(&workspace_id)?;
        let feature_id = parse_id(&feature_id)?;
        let items = self.feature_items(feature_id)?;
        let work_item = items
            .iter()
            .find(|item| item.id == work_item_id)
            .cloned()
            .ok_or(AppError::WorkItemNotFound)?;
        let edges = self.feature_edges(feature_id)?;
        let readiness = readiness(&work_item, &items, &edges)?;
        let sessions = self.sessions(work_item_id)?;
        let available_actions = work_item_actions(&work_item, &readiness, &sessions);
        Ok(WorkItemProjection {
            schema_version: 1,
            workspace_id,
            work_item,
            readiness,
            sessions,
            available_actions,
        })
    }

    fn feature_items(
        &self,
        feature_id: workboard_core::FeatureId,
    ) -> Result<Vec<WorkItem>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT item.id, item.feature_id, item.key, item.slug, item.title,
                        item.status, document.id
                 FROM work_items item
                 JOIN documents document ON document.work_item_id = item.id
                 WHERE item.feature_id = ?1 ORDER BY item.proposal_order, item.id",
            )?;
            statement
                .query_map([feature_id.to_string()], |row| {
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
                .map(|row| {
                    let (id, feature_id, key, slug, title, status, document_id) = row?;
                    Ok(WorkItem {
                        id: parse_id(&id)?,
                        feature_id: parse_id(&feature_id)?,
                        key: workboard_core::WorkItemKey::new(key)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                        slug: workboard_core::Slug::new(slug)
                            .map_err(|error| AppError::Domain(error.to_string()))?,
                        title,
                        status: parse_wire(&status)?,
                        document_id: parse_id(&document_id)?,
                        repository_ids: repository_ids(connection, &id)?,
                    })
                })
                .collect()
        })
    }

    fn feature_edges(
        &self,
        feature_id: workboard_core::FeatureId,
    ) -> Result<Vec<(WorkItemId, WorkItemId)>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT edge.work_item_id, edge.dependency_work_item_id
                 FROM work_item_dependencies edge
                 JOIN work_items item ON item.id = edge.work_item_id
                 WHERE item.feature_id = ?1
                 ORDER BY item.proposal_order, edge.dependency_order",
            )?;
            statement
                .query_map([feature_id.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .map(|row| {
                    let (item, dependency) = row?;
                    Ok((parse_id(&item)?, parse_id(&dependency)?))
                })
                .collect()
        })
    }

    fn sessions(&self, work_item_id: WorkItemId) -> Result<Vec<SessionChoice>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT session.id, session.provider, association.role,
                        association.associated_until, managed.status, managed.managed_until,
                        live.status, live.observed_at, managed.checkout_id,
                        checkout.repository_id, path.path, checkout.branch,
                        readiness.purpose, readiness.reconciliation_generation,
                        EXISTS (
                            SELECT 1 FROM restore_entries restore
                            WHERE restore.session_id = session.id AND restore.removed_at IS NULL
                        ),
                        CASE
                          WHEN EXISTS (
                            SELECT 1 FROM native_session_sources source
                            WHERE source.session_id = session.id AND source.missing = 0
                          ) THEN 'validated'
                          WHEN EXISTS (
                            SELECT 1 FROM native_session_sources source
                            WHERE source.session_id = session.id
                          ) THEN 'missing'
                          ELSE 'unknown'
                        END,
                        profile.schema_version, profile.model, profile.effort, profile.source,
                        managed.binding_generation
                 FROM native_session_associations association
                 JOIN native_sessions session ON session.id = association.session_id
                 JOIN managed_sessions managed ON managed.id = (
                    SELECT candidate.id FROM managed_sessions candidate
                    WHERE candidate.session_id = session.id
                    ORDER BY candidate.managed_from DESC, candidate.id DESC LIMIT 1
                 )
                 JOIN checkouts checkout ON checkout.id = managed.checkout_id
                 JOIN checkout_paths path ON path.id = (
                    SELECT candidate.id FROM checkout_paths candidate
                    WHERE candidate.checkout_id = checkout.id
                    ORDER BY candidate.observed_from DESC, candidate.id DESC LIMIT 1
                 )
                 LEFT JOIN checkout_readiness readiness ON readiness.checkout_id = checkout.id
                 LEFT JOIN launch_profiles profile ON profile.id = managed.profile_id
                 LEFT JOIN live_observations live ON live.id = (
                    SELECT candidate.id FROM live_observations candidate
                    WHERE candidate.session_id = session.id
                    ORDER BY candidate.observed_at DESC, candidate.id DESC LIMIT 1
                 )
                 WHERE association.work_item_id = ?1
                 ORDER BY association.associated_until IS NULL DESC,
                          managed.managed_until IS NULL DESC,
                          live.observed_at DESC, session.id",
            )?;
            let rows = statement
                .query_map([work_item_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, bool>(14)?,
                        row.get::<_, String>(15)?,
                        row.get::<_, Option<u32>>(16)?,
                        row.get::<_, Option<String>>(17)?,
                        row.get::<_, Option<String>>(18)?,
                        row.get::<_, Option<String>>(19)?,
                        row.get::<_, u32>(20)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(
                        session_id,
                        provider,
                        role,
                        associated_until,
                        managed_status,
                        managed_until,
                        live_status,
                        last_activity,
                        checkout_id,
                        repository_id,
                        checkout_path,
                        branch,
                        purpose,
                        generation,
                        restore_active,
                        resumability,
                        profile_schema,
                        model,
                        effort,
                        source,
                        binding_generation,
                    )| {
                        let session_id = parse_id(&session_id)?;
                        let provider = parse_tool(&provider)?;
                        let role = parse_wire(&role)?;
                        let profile = match (profile_schema, model, effort, source) {
                            (Some(schema_version), Some(model), Some(effort), Some(source)) => {
                                LaunchProfile {
                                    schema_version,
                                    tool: provider,
                                    model: Some(model),
                                    effort: Some(parse_wire(&effort)?),
                                    role,
                                    source: parse_wire(&source)?,
                                }
                            }
                            _ => LaunchProfile::legacy_unknown(provider, role),
                        };
                        let live_status = live_status.as_deref().map(parse_wire).transpose()?;
                        let resumability = parse_wire(&resumability)?;
                        let primary_writer = role == ManagedSessionRole::WorkItemExecution
                            && associated_until.is_none()
                            && managed_until.is_none()
                            && purpose.as_deref() == Some("work_item_write");
                        let actions = session_actions(
                            work_item_id,
                            session_id,
                            live_status,
                            resumability,
                            associated_until.is_none() && managed_until.is_none(),
                        );
                        Ok(SessionChoice {
                            session_id,
                            binding_generation,
                            provider,
                            profile,
                            role,
                            primary_writer,
                            binding_status: if managed_until.is_none() {
                                managed_status
                            } else {
                                "stopped".to_owned()
                            },
                            live_status,
                            restore_active,
                            last_activity: last_activity.as_deref().map(parse_time).transpose()?,
                            checkout_id: parse_id(&checkout_id)?,
                            repository_id: parse_id(&repository_id)?,
                            checkout_purpose: purpose.as_deref().map(parse_wire).transpose()?,
                            checkout_path: PathBuf::from(checkout_path),
                            branch,
                            checkout_generation: generation
                                .map(u64::try_from)
                                .transpose()
                                .map_err(|error| AppError::Domain(error.to_string()))?,
                            resumability,
                            actions,
                        })
                    },
                )
                .collect()
        })
    }
}

fn readiness(
    work_item: &WorkItem,
    items: &[WorkItem],
    edges: &[(WorkItemId, WorkItemId)],
) -> Result<WorkItemReadiness, AppError> {
    let dependencies = edges
        .iter()
        .filter(|(item, _)| *item == work_item.id)
        .filter_map(|(_, dependency)| items.iter().find(|item| item.id == *dependency))
        .cloned()
        .map(|work_item| WorkItemDependencyFact {
            satisfied: dependency_satisfied(work_item.status),
            work_item,
        })
        .collect::<Vec<_>>();
    let blocked_by = dependencies
        .iter()
        .filter(|dependency| !dependency.satisfied)
        .map(|dependency| dependency.work_item.id)
        .collect::<Vec<_>>();
    let dependants = edges
        .iter()
        .filter(|(_, dependency)| *dependency == work_item.id)
        .map(|(item, _)| *item)
        .collect::<Vec<_>>();
    let mut memo = HashMap::new();
    let layer = dependency_layer(work_item.id, edges, &mut memo, &mut HashSet::new())?;
    let ready = blocked_by.is_empty()
        && matches!(
            work_item.status,
            WorkItemStatus::Ready | WorkItemStatus::InProgress | WorkItemStatus::Review
        );
    Ok(WorkItemReadiness {
        ready,
        layer,
        dependencies,
        dependants,
        blocked_by,
        active: work_item.status == WorkItemStatus::InProgress,
        parallelizable: ready,
    })
}

fn dependency_layer(
    work_item_id: WorkItemId,
    edges: &[(WorkItemId, WorkItemId)],
    memo: &mut HashMap<WorkItemId, u32>,
    visiting: &mut HashSet<WorkItemId>,
) -> Result<u32, AppError> {
    if let Some(layer) = memo.get(&work_item_id) {
        return Ok(*layer);
    }
    if !visiting.insert(work_item_id) {
        return Err(AppError::PlanningDocumentInvalid(
            "Work-item dependencies must be acyclic".to_owned(),
        ));
    }
    let mut layer = 0;
    for (_, dependency) in edges.iter().filter(|(item, _)| *item == work_item_id) {
        layer = layer.max(dependency_layer(*dependency, edges, memo, visiting)?.saturating_add(1));
    }
    visiting.remove(&work_item_id);
    memo.insert(work_item_id, layer);
    Ok(layer)
}

fn work_item_actions(
    work_item: &WorkItem,
    readiness: &WorkItemReadiness,
    sessions: &[SessionChoice],
) -> AvailableActions {
    let owner = HierarchyOwner::WorkItem(work_item.id);
    let current_primary = sessions.iter().any(|session| session.primary_writer);
    let kind = if sessions.is_empty() {
        AvailableActionKind::Start
    } else {
        AvailableActionKind::StartAnother
    };
    let label = if sessions.is_empty() {
        "Start"
    } else {
        "Start another"
    };
    let mut start = AvailableAction::enabled(kind, label, owner, "work.start");
    start.role = Some(ManagedSessionRole::WorkItemExecution);
    start.access_mode = Some(CheckoutAccessMode::WriteIsolated);
    if !readiness.ready {
        start = start.disabled(if readiness.blocked_by.is_empty() {
            "Work-item status is not launchable".to_owned()
        } else {
            format!(
                "blocked by {} incomplete dependencies",
                readiness.blocked_by.len()
            )
        });
    } else if current_primary {
        start.confirmation =
            Some("Start an additional writer in a distinct writer-session checkout".to_owned());
    }
    let mut actions = vec![start];
    if sessions.iter().any(|session| {
        session
            .actions
            .iter()
            .any(|action| action.kind == AvailableActionKind::Resume && action.enabled)
    }) {
        actions.push(AvailableAction::enabled(
            AvailableActionKind::Resume,
            "Resume selected session",
            owner,
            "work.continue",
        ));
    }
    AvailableActions {
        schema_version: AVAILABLE_ACTIONS_SCHEMA_VERSION,
        owner,
        workflow_state: None,
        revision: u64::from(readiness.layer),
        actions,
        diagnostics: Vec::new(),
    }
}

fn session_actions(
    work_item_id: WorkItemId,
    session_id: ConversationId,
    live_status: Option<LiveStatus>,
    resumability: Resumability,
    current: bool,
) -> Vec<AvailableAction> {
    let owner = HierarchyOwner::WorkItem(work_item_id);
    let mut resume = AvailableAction::enabled(
        AvailableActionKind::Resume,
        "Resume",
        owner,
        "work.continue",
    );
    resume.session_id = Some(session_id);
    if live_status.is_some_and(LiveStatus::indicates_live) && current {
        resume = resume.disabled("session is already live");
    } else if resumability != Resumability::Validated {
        resume = resume.disabled("session has no validated resume source");
    }
    let mut actions = vec![resume];
    if live_status.is_some_and(LiveStatus::indicates_live) && current {
        let mut follow_up = AvailableAction::enabled(
            AvailableActionKind::SendFollowUp,
            "Send follow-up",
            owner,
            "session.follow_up",
        );
        follow_up.session_id = Some(session_id);
        actions.push(follow_up);
    }
    actions
}

fn dependency_satisfied(status: WorkItemStatus) -> bool {
    matches!(status, WorkItemStatus::Review | WorkItemStatus::Done)
}

fn repository_ids(
    connection: &rusqlite::Connection,
    work_item_id: &str,
) -> Result<Vec<RepositoryId>, AppError> {
    let mut statement = connection.prepare(
        "SELECT repository_id FROM work_item_repositories
         WHERE work_item_id = ?1 ORDER BY repository_id",
    )?;
    statement
        .query_map([work_item_id], |row| row.get::<_, String>(0))?
        .map(|row| parse_id(&row?))
        .collect()
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

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    parse_wire(value)
}

fn parse_wire<T>(value: &str) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(Into::into)
}

fn parse_time(value: &str) -> Result<OffsetDateTime, AppError> {
    value
        .parse::<i128>()
        .ok()
        .and_then(|value| OffsetDateTime::from_unix_timestamp_nanos(value).ok())
        .or_else(|| {
            OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
        })
        .ok_or_else(|| AppError::Domain(format!("invalid session activity timestamp: {value}")))
}

#[cfg(test)]
mod tests {
    use workboard_core::{FeatureId, Slug, WorkItemKey};

    use super::{dependency_layer, readiness};

    fn item(status: workboard_core::WorkItemStatus, suffix: &str) -> workboard_core::WorkItem {
        workboard_core::WorkItem {
            id: workboard_core::WorkItemId::generate(),
            feature_id: FeatureId::generate(),
            key: WorkItemKey::new(format!("epic/feature/{suffix}")).expect("key"),
            slug: Slug::new(suffix).expect("slug"),
            title: suffix.to_owned(),
            status,
            document_id: workboard_core::DocumentId::generate(),
            repository_ids: Vec::new(),
        }
    }

    #[test]
    fn readiness_projects_layers_blockers_and_review_as_accepted() {
        let root = item(workboard_core::WorkItemStatus::Review, "root");
        let middle = item(workboard_core::WorkItemStatus::Ready, "middle");
        let leaf = item(workboard_core::WorkItemStatus::Ready, "leaf");
        let items = vec![root.clone(), middle.clone(), leaf.clone()];
        let edges = vec![(middle.id, root.id), (leaf.id, middle.id)];

        let middle_readiness = readiness(&middle, &items, &edges).expect("middle readiness");
        assert!(middle_readiness.ready);
        assert_eq!(middle_readiness.layer, 1);
        let leaf_readiness = readiness(&leaf, &items, &edges).expect("leaf readiness");
        assert!(!leaf_readiness.ready);
        assert_eq!(leaf_readiness.blocked_by, [middle.id]);
        assert_eq!(
            dependency_layer(
                leaf.id,
                &edges,
                &mut std::collections::HashMap::new(),
                &mut std::collections::HashSet::new(),
            )
            .expect("layer"),
            2
        );
    }
}
