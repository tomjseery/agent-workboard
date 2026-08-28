use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, MAIN_DB, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use workboard_core::{
    AssociationIntervalId, CheckoutId, CheckoutPathId, ConversationId, ImportBatchId, RepositoryId,
    RepositoryPathId, Tool, WorkItemId, WorkspaceId,
};

use crate::AppError;
use crate::workspace::WorkboardApplication;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportPreview {
    pub source: PathBuf,
    pub source_hash: String,
    pub tables: Vec<String>,
    pub repositories: u64,
    pub native_sessions: u64,
    pub association_events: u64,
    pub checkouts: u64,
    pub repository_inventory: Vec<LegacyRepositoryPreview>,
    pub session_candidates: Vec<LegacySessionCandidatePreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyRepositoryPreview {
    pub source_id: String,
    pub common_directory: PathBuf,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacySessionCandidatePreview {
    pub selected: bool,
    pub source_conversation_id: String,
    pub destination_session_id: ConversationId,
    pub tool: Tool,
    pub native_id: String,
    pub discovered_at: String,
    pub source_repository_id: Option<String>,
    pub source_worktree_id: Option<String>,
    pub legacy_workstream_id: Option<String>,
    pub legacy_workstream_title: Option<String>,
    pub authority: Option<String>,
    pub confidence: Option<String>,
    pub confirmed_legacy_association: bool,
    pub adopt_work_item_id: Option<WorkItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_prompt_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_prompt_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportOutcome {
    pub import_id: ImportBatchId,
    pub preview_hash: String,
    pub repositories: usize,
    pub checkouts: usize,
    pub native_sessions: usize,
    pub session_sources: usize,
    pub live_observations: usize,
    pub adopted_sessions: usize,
    pub already_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSessionCandidate {
    pub workspace_id: WorkspaceId,
    pub session_id: ConversationId,
    pub repository_id: RepositoryId,
    pub checkout_id: Option<CheckoutId>,
    pub tool: Tool,
    pub native_id: String,
    pub legacy_workstream_id: Option<String>,
    pub legacy_workstream_title: Option<String>,
    pub authority: Option<String>,
    pub confidence: Option<String>,
    pub status: String,
    pub native_title: Option<String>,
    pub first_prompt_preview: Option<String>,
    pub last_prompt_preview: Option<String>,
    pub last_activity_at: Option<String>,
    pub observed_cwd: Option<PathBuf>,
}

pub fn snapshot_context_catalogue(
    source: &Path,
    destination: &Path,
) -> Result<LegacyImportPreview, AppError> {
    validate_source_database(source)?;
    if !destination.is_absolute() || destination.exists() {
        return Err(AppError::Domain(format!(
            "legacy backup destination must be an unused absolute path: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| AppError::StorageIo {
            operation: "creating the legacy backup directory",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let connection = open_read_only(source)?;
    connection.backup(MAIN_DB, destination, None)?;
    let backup = open_read_only(destination)?;
    let integrity: String = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(AppError::Domain(format!(
            "legacy backup integrity check failed: {integrity}"
        )));
    }
    drop(backup);
    preview_context_catalogue(destination)
}

pub fn preview_context_catalogue(path: &Path) -> Result<LegacyImportPreview, AppError> {
    validate_source_database(path)?;
    let connection = open_read_only(path)?;
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let repositories = count_if_present(&connection, &tables, "repositories")?;
    let native_sessions =
        count_first_present(&connection, &tables, &["conversations", "native_sessions"])?;
    let association_events = count_first_present(
        &connection,
        &tables,
        &["association_events", "conversation_context_intervals"],
    )?;
    let checkouts = count_first_present(&connection, &tables, &["worktrees", "checkouts"])?;
    let mut warnings = Vec::new();
    if !tables.iter().any(|table| table == "repositories") {
        warnings.push("repository inventory is unavailable".to_owned());
    }
    if native_sessions == 0 {
        warnings.push("no native sessions were found".to_owned());
    }
    let repository_inventory = legacy_repositories(&connection, &tables)?;
    let session_candidates = legacy_session_candidates(&connection, &tables)?;
    Ok(LegacyImportPreview {
        source: path.to_path_buf(),
        source_hash: hash_file(path)?,
        tables,
        repositories,
        native_sessions,
        association_events,
        checkouts,
        repository_inventory,
        session_candidates,
        warnings,
    })
}

fn validate_source_database(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::Domain(format!(
            "legacy database is unavailable: {}",
            path.display()
        )));
    }
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection, AppError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(Into::into)
}

fn legacy_repositories(
    connection: &Connection,
    tables: &[String],
) -> Result<Vec<LegacyRepositoryPreview>, AppError> {
    if !tables.iter().any(|table| table == "repositories") {
        return Ok(Vec::new());
    }
    let has_display_name = table_has_column(connection, "repositories", "display_name")?;
    let display_name = if has_display_name {
        "display_name"
    } else {
        "NULL"
    };
    let mut statement = connection.prepare(&format!(
        "SELECT id, common_dir, {display_name} FROM repositories ORDER BY common_dir"
    ))?;
    statement
        .query_map([], |row| {
            Ok(LegacyRepositoryPreview {
                source_id: row.get(0)?,
                common_directory: PathBuf::from(row.get::<_, String>(1)?),
                display_name: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn legacy_session_candidates(
    connection: &Connection,
    tables: &[String],
) -> Result<Vec<LegacySessionCandidatePreview>, AppError> {
    if !tables.iter().any(|table| table == "conversations") {
        return Ok(Vec::new());
    }
    let has_context = tables
        .iter()
        .any(|table| table == "conversation_context_intervals");
    let has_workstreams = tables.iter().any(|table| table == "workstreams");
    let has_association_events = tables.iter().any(|table| table == "association_events");
    let has_summaries = tables.iter().any(|table| table == "conversation_summaries");
    let has_cwd_observations = tables
        .iter()
        .any(|table| table == "conversation_cwd_observations");
    let has_last_activity = table_has_column(connection, "conversations", "last_activity_at")?;
    let context_columns = if has_context {
        format!(
            "context.repository_id, context.worktree_id, context.workstream_id, {},
             context.authority, context.confidence, {}",
            if has_workstreams {
                "workstream.title"
            } else {
                "NULL"
            },
            if has_association_events {
                "COALESCE(event.confirmed, 0)"
            } else {
                "0"
            }
        )
    } else {
        "NULL, NULL, NULL, NULL, NULL, NULL, 0".to_owned()
    };
    let mut context_joins = String::new();
    if has_context {
        context_joins.push_str(
            "LEFT JOIN conversation_context_intervals context ON context.id = (
                 SELECT candidate.id FROM conversation_context_intervals candidate
                 WHERE candidate.conversation_id = conversation.id
                 ORDER BY CAST(candidate.started_at AS INTEGER) DESC, candidate.id DESC LIMIT 1
             ) ",
        );
        if has_workstreams {
            context_joins.push_str(
                "LEFT JOIN workstreams workstream ON workstream.id = context.workstream_id ",
            );
        }
        if has_association_events {
            context_joins.push_str(
                "LEFT JOIN association_events event ON event.id = context.source_event_id ",
            );
        }
    }
    let summary_columns = if has_summaries {
        "summary.native_title, summary.first_prompt_preview, summary.last_prompt_preview"
    } else {
        "NULL, NULL, NULL"
    };
    let summary_join = if has_summaries {
        "LEFT JOIN conversation_summaries summary ON summary.conversation_id = conversation.id"
    } else {
        ""
    };
    let last_activity = if has_last_activity {
        "conversation.last_activity_at"
    } else {
        "NULL"
    };
    let observed_cwd = if has_cwd_observations {
        "(SELECT cwd.path FROM conversation_cwd_observations cwd
          WHERE cwd.conversation_id = conversation.id
          ORDER BY cwd.observed_at IS NULL, cwd.observed_at DESC, cwd.path LIMIT 1)"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT conversation.id, conversation.tool, conversation.native_id,
                conversation.created_at, {context_columns}, {summary_columns},
                {last_activity}, {observed_cwd}
         FROM conversations conversation
         {context_joins}
         {summary_join}
         ORDER BY CAST(conversation.created_at AS INTEGER), conversation.id"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map([], |row| {
            let tool = parse_tool(&row.get::<_, String>(1)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let source_conversation_id = row.get::<_, String>(0)?;
            let destination_session_id = source_conversation_id
                .parse()
                .unwrap_or_else(|_| ConversationId::generate());
            Ok(LegacySessionCandidatePreview {
                selected: true,
                source_conversation_id,
                destination_session_id,
                tool,
                native_id: row.get(2)?,
                discovered_at: row.get(3)?,
                source_repository_id: row.get(4)?,
                source_worktree_id: row.get(5)?,
                legacy_workstream_id: row.get(6)?,
                legacy_workstream_title: row.get(7)?,
                authority: row.get(8)?,
                confidence: row.get(9)?,
                confirmed_legacy_association: row.get::<_, i64>(10)? != 0,
                adopt_work_item_id: None,
                native_title: row.get(11)?,
                first_prompt_preview: row.get(12)?,
                last_prompt_preview: row.get(13)?,
                last_activity_at: row.get(14)?,
                observed_cwd: row.get::<_, Option<String>>(15)?.map(PathBuf::from),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}

impl WorkboardApplication {
    pub fn apply_context_catalogue_import(
        &mut self,
        workspace_id: WorkspaceId,
        repository_id: RepositoryId,
        preview: &LegacyImportPreview,
    ) -> Result<LegacyImportOutcome, AppError> {
        validate_legacy_preview(preview)?;
        if hash_file(&preview.source)? != preview.source_hash {
            return Err(AppError::WorkflowDocumentChanged);
        }
        let preview_hash = hash_bytes(&serde_json::to_vec(preview)?);
        if let Some(outcome) = self.existing_legacy_import(&preview_hash)? {
            self.enrich_legacy_candidates(workspace_id, repository_id, outcome.import_id, preview)?;
            return Ok(outcome);
        }
        let target_common_directory = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT git_common_directory FROM repositories
                     WHERE id = ?1 AND workspace_id = ?2 AND is_planning_store = 0",
                    params![repository_id.to_string(), workspace_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    AppError::Domain(
                        "legacy import repository is not registered in the workspace".to_owned(),
                    )
                })
        })?;
        let legacy_repository = preview
            .repository_inventory
            .iter()
            .find(|repository| {
                paths_equal(
                    &repository.common_directory,
                    Path::new(&target_common_directory),
                )
            })
            .ok_or_else(|| {
                AppError::Domain(
                    "no legacy repository has the registered Git common directory".to_owned(),
                )
            })?;
        let selected = preview
            .session_candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .map(|candidate| (candidate.source_conversation_id.as_str(), candidate))
            .collect::<HashMap<_, _>>();
        if selected.is_empty() {
            return Err(AppError::Domain(
                "legacy import selects no native sessions".to_owned(),
            ));
        }
        let mut destination_sessions = HashMap::new();
        let mut destination_session_ids = HashSet::new();
        for candidate in selected.values() {
            if !destination_session_ids.insert(candidate.destination_session_id) {
                return Err(AppError::Domain(
                    "legacy preview contains a duplicate destination session ID".to_owned(),
                ));
            }
            let existing = self.store.read(|connection| {
                connection
                    .query_row(
                        "SELECT id FROM native_sessions WHERE provider = ?1 AND native_id = ?2",
                        params![tool_name(candidate.tool), candidate.native_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(Into::into)
            })?;
            let destination = existing
                .as_deref()
                .map(parse_id)
                .transpose()?
                .unwrap_or(candidate.destination_session_id);
            destination_sessions.insert(candidate.source_conversation_id.clone(), destination);
        }
        self.validate_candidate_adoptions(repository_id, selected.values().copied())?;
        let source = open_read_only(&preview.source)?;
        source.execute_batch("BEGIN")?;
        let import_id = ImportBatchId::generate();
        let imported_at = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        let mut checkout_map = HashMap::new();
        let mut imported_checkouts = 0_usize;
        let mut imported_sessions = 0_usize;
        let mut imported_sources = 0_usize;
        let mut imported_live = 0_usize;
        let mut adopted_sessions = 0_usize;
        let legacy_repository_id = legacy_repository.source_id.clone();
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO import_batches (
                     id, workspace_id, kind, source_path, source_head, preview_hash,
                     planning_commit, imported_at
                 ) VALUES (?1, ?2, 'context_catalogue', ?3, NULL, ?4, NULL, ?5)",
                params![
                    import_id.to_string(),
                    workspace_id.to_string(),
                    path_text(&preview.source)?,
                    preview_hash,
                    imported_at,
                ],
            )?;
            import_repository_history(
                &source,
                transaction,
                import_id,
                &legacy_repository_id,
                repository_id,
                &imported_at,
            )?;
            import_repository_remotes(
                &source,
                transaction,
                import_id,
                &legacy_repository_id,
                repository_id,
            )?;
            let mut worktrees = source.prepare(
                "SELECT id, git_dir, path, branch, head_oid, last_seen_at, present
                 FROM worktrees WHERE repository_id = ?1 ORDER BY last_seen_at, id",
            )?;
            let worktree_rows = worktrees.query_map([legacy_repository_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?;
            for row in worktree_rows {
                let (source_id, git_dir, path, branch, head, last_seen_at, present) = row?;
                let existing = transaction
                    .query_row(
                        "SELECT id FROM checkouts
                         WHERE repository_id = ?1 AND git_worktree_identity = ?2",
                        params![repository_id.to_string(), git_dir],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let checkout_id = existing
                    .as_deref()
                    .map(parse_id)
                    .transpose()?
                    .unwrap_or_else(|| {
                        source_id.parse().unwrap_or_else(|_| CheckoutId::generate())
                    });
                if existing.is_none() {
                    transaction.execute(
                        "INSERT INTO checkouts (
                             id, repository_id, git_worktree_identity, branch, head,
                             availability, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            checkout_id.to_string(),
                            repository_id.to_string(),
                            git_dir,
                            branch.as_deref().map(short_branch),
                            head,
                            if present == 0 { "missing" } else { "available" },
                            last_seen_at,
                        ],
                    )?;
                    imported_checkouts += 1;
                }
                checkout_map.insert(source_id.clone(), checkout_id);
                record_legacy_row(
                    transaction,
                    import_id,
                    "worktrees",
                    &source_id,
                    "checkout",
                    &checkout_id.to_string(),
                    &serde_json::json!({
                        "id": source_id,
                        "repositoryId": legacy_repository_id,
                        "gitDir": git_dir,
                        "path": path,
                        "branch": branch,
                        "headOid": head,
                        "lastSeenAt": last_seen_at,
                        "present": present,
                    }),
                )?;
            }
            import_checkout_paths(&source, transaction, import_id, &checkout_map)?;
            for candidate in selected.values() {
                let session_id = destination_sessions[&candidate.source_conversation_id];
                let inserted = transaction.execute(
                    "INSERT OR IGNORE INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        session_id.to_string(),
                        tool_name(candidate.tool),
                        candidate.native_id,
                        candidate.discovered_at,
                    ],
                )?;
                imported_sessions += inserted;
                let checkout_id = candidate
                    .source_worktree_id
                    .as_ref()
                    .and_then(|source_id| checkout_map.get(source_id))
                    .copied();
                let status = if candidate.adopt_work_item_id.is_some() {
                    "confirmed"
                } else {
                    "unassigned"
                };
                let observed_cwd = candidate
                    .observed_cwd
                    .as_deref()
                    .map(path_text)
                    .transpose()?;
                transaction.execute(
                    "INSERT INTO imported_session_candidates (
                         workspace_id, session_id, repository_id, checkout_id,
                         legacy_workstream_id, legacy_workstream_title, authority,
                         confidence, status, imported_at, native_title,
                         first_prompt_preview, last_prompt_preview, last_activity_at,
                         observed_cwd
                     ) VALUES (
                         ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                         ?11, ?12, ?13, ?14, ?15
                     )
                     ON CONFLICT(workspace_id, session_id) DO NOTHING",
                    params![
                        workspace_id.to_string(),
                        session_id.to_string(),
                        repository_id.to_string(),
                        checkout_id.map(|id| id.to_string()),
                        candidate.legacy_workstream_id,
                        candidate.legacy_workstream_title,
                        candidate.authority,
                        candidate.confidence,
                        status,
                        imported_at,
                        candidate.native_title,
                        candidate.first_prompt_preview,
                        candidate.last_prompt_preview,
                        candidate.last_activity_at,
                        observed_cwd,
                    ],
                )?;
                if let Some(work_item_id) = candidate.adopt_work_item_id {
                    transaction.execute(
                        "INSERT INTO native_session_associations (
                             id, session_id, work_item_id, role, associated_from
                         ) VALUES (?1, ?2, ?3, 'work_item_execution', ?4)",
                        params![
                            AssociationIntervalId::generate().to_string(),
                            session_id.to_string(),
                            work_item_id.to_string(),
                            imported_at,
                        ],
                    )?;
                    adopted_sessions += 1;
                }
                transaction.execute(
                    "INSERT INTO import_source_destinations (
                         import_id, source_path, source_hash, destination_kind,
                         destination_id, document_id
                     ) VALUES (?1, ?2, ?3, 'session_candidate', ?4, NULL)",
                    params![
                        import_id.to_string(),
                        candidate.source_conversation_id,
                        preview.source_hash,
                        session_id.to_string(),
                    ],
                )?;
                record_legacy_row(
                    transaction,
                    import_id,
                    "conversations",
                    &candidate.source_conversation_id,
                    "native_session",
                    &session_id.to_string(),
                    &serde_json::json!({
                        "id": candidate.source_conversation_id,
                        "tool": tool_name(candidate.tool),
                        "nativeId": candidate.native_id,
                        "createdAt": candidate.discovered_at,
                    }),
                )?;
            }
            imported_sources =
                import_session_sources(&source, transaction, import_id, &destination_sessions)?;
            imported_live =
                import_live_observations(&source, transaction, import_id, &destination_sessions)?;
            import_context_evidence(
                &source,
                transaction,
                import_id,
                &destination_sessions,
                &checkout_map,
            )?;
            Ok(())
        })?;
        source.execute_batch("COMMIT")?;
        self.enrich_legacy_candidates(workspace_id, repository_id, import_id, preview)?;
        Ok(LegacyImportOutcome {
            import_id,
            preview_hash,
            repositories: 1,
            checkouts: imported_checkouts,
            native_sessions: imported_sessions,
            session_sources: imported_sources,
            live_observations: imported_live,
            adopted_sessions,
            already_applied: false,
        })
    }

    pub fn imported_session_candidates(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ImportedSessionCandidate>, AppError> {
        self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT candidate.session_id, candidate.repository_id, candidate.checkout_id,
                        session.provider, session.native_id, candidate.legacy_workstream_id,
                        candidate.legacy_workstream_title, candidate.authority,
                        candidate.confidence, candidate.status, candidate.native_title,
                        candidate.first_prompt_preview, candidate.last_prompt_preview,
                        candidate.last_activity_at, candidate.observed_cwd
                 FROM imported_session_candidates candidate
                 JOIN native_sessions session ON session.id = candidate.session_id
                 WHERE candidate.workspace_id = ?1
                 ORDER BY candidate.status, candidate.last_activity_at IS NULL,
                          candidate.last_activity_at DESC, session.provider, session.native_id",
            )?;
            let rows = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, Option<String>>(13)?,
                        row.get::<_, Option<String>>(14)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(
                    |(
                        session_id,
                        repository_id,
                        checkout_id,
                        provider,
                        native_id,
                        workstream_id,
                        workstream_title,
                        authority,
                        confidence,
                        status,
                        native_title,
                        first_prompt_preview,
                        last_prompt_preview,
                        last_activity_at,
                        observed_cwd,
                    )| {
                        Ok(ImportedSessionCandidate {
                            workspace_id,
                            session_id: parse_id(&session_id)?,
                            repository_id: parse_id(&repository_id)?,
                            checkout_id: checkout_id.as_deref().map(parse_id).transpose()?,
                            tool: parse_tool(&provider)?,
                            native_id,
                            legacy_workstream_id: workstream_id,
                            legacy_workstream_title: workstream_title,
                            authority,
                            confidence,
                            status,
                            native_title,
                            first_prompt_preview,
                            last_prompt_preview,
                            last_activity_at,
                            observed_cwd: observed_cwd.map(PathBuf::from),
                        })
                    },
                )
                .collect()
        })
    }

    pub fn adopt_imported_session_candidate(
        &mut self,
        workspace_id: WorkspaceId,
        session_id: ConversationId,
        work_item_id: WorkItemId,
        observed_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            let valid: i64 = transaction.query_row(
                "SELECT EXISTS (
                     SELECT 1 FROM imported_session_candidates candidate
                     JOIN work_item_repositories target
                       ON target.repository_id = candidate.repository_id
                      AND target.work_item_id = ?3
                     WHERE candidate.workspace_id = ?1 AND candidate.session_id = ?2
                       AND candidate.status = 'unassigned'
                 )",
                params![
                    workspace_id.to_string(),
                    session_id.to_string(),
                    work_item_id.to_string(),
                ],
                |row| row.get(0),
            )?;
            if valid == 0 {
                return Err(AppError::WorkItemRepositoryMismatch);
            }
            transaction.execute(
                "INSERT INTO native_session_associations (
                     id, session_id, work_item_id, role, associated_from
                 ) VALUES (?1, ?2, ?3, 'work_item_execution', ?4)",
                params![
                    AssociationIntervalId::generate().to_string(),
                    session_id.to_string(),
                    work_item_id.to_string(),
                    observed_at.unix_timestamp_nanos().to_string(),
                ],
            )?;
            transaction.execute(
                "UPDATE imported_session_candidates SET status = 'confirmed'
                 WHERE workspace_id = ?1 AND session_id = ?2 AND status = 'unassigned'",
                params![workspace_id.to_string(), session_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn ignore_imported_session_candidate(
        &mut self,
        workspace_id: WorkspaceId,
        session_id: ConversationId,
    ) -> Result<(), AppError> {
        let changed = self.store.write(|transaction| {
            transaction
                .execute(
                    "UPDATE imported_session_candidates SET status = 'ignored'
                     WHERE workspace_id = ?1 AND session_id = ?2 AND status = 'unassigned'",
                    params![workspace_id.to_string(), session_id.to_string()],
                )
                .map_err(Into::into)
        })?;
        if changed == 0 {
            return Err(AppError::ConversationNotFound);
        }
        Ok(())
    }

    fn enrich_legacy_candidates(
        &mut self,
        workspace_id: WorkspaceId,
        repository_id: RepositoryId,
        import_id: ImportBatchId,
        preview: &LegacyImportPreview,
    ) -> Result<(), AppError> {
        let selected = preview
            .session_candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .map(|candidate| candidate.source_conversation_id.as_str())
            .collect::<HashSet<_>>();
        let source = open_read_only(&preview.source)?;
        let candidates = legacy_session_candidates(&source, &preview.tables)?
            .into_iter()
            .filter(|candidate| selected.contains(candidate.source_conversation_id.as_str()))
            .collect::<Vec<_>>();
        let destinations = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT source_path, destination_id FROM import_source_destinations
                 WHERE import_id = ?1 AND destination_kind = 'session_candidate'",
            )?;
            statement
                .query_map([import_id.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(Into::into)
        })?;
        let checkout_paths = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT checkout.id, path.path FROM checkouts checkout
                 JOIN checkout_paths path ON path.checkout_id = checkout.id
                 WHERE checkout.repository_id = ?1",
            )?;
            let rows = statement
                .query_map([repository_id.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(checkout_id, path)| Ok((parse_id(&checkout_id)?, PathBuf::from(path))))
                .collect::<Result<Vec<_>, AppError>>()
        })?;
        let summaries = legacy_summary_records(&source, &preview.tables, &selected)?;
        let cwd_observations = legacy_cwd_records(&source, &preview.tables, &selected)?;
        self.store.write(|transaction| {
            for candidate in &candidates {
                let Some(destination) = destinations.get(&candidate.source_conversation_id) else {
                    continue;
                };
                let session_id = parse_id::<ConversationId>(destination)?;
                let checkout_id = candidate
                    .observed_cwd
                    .as_deref()
                    .and_then(|cwd| checkout_for_path(cwd, &checkout_paths));
                let observed_cwd = candidate
                    .observed_cwd
                    .as_deref()
                    .map(path_text)
                    .transpose()?;
                transaction.execute(
                    "UPDATE imported_session_candidates SET
                         checkout_id = COALESCE(checkout_id, ?3), native_title = ?4,
                         first_prompt_preview = ?5, last_prompt_preview = ?6,
                         last_activity_at = ?7, observed_cwd = ?8
                     WHERE workspace_id = ?1 AND session_id = ?2",
                    params![
                        workspace_id.to_string(),
                        session_id.to_string(),
                        checkout_id.map(|id| id.to_string()),
                        candidate.native_title,
                        candidate.first_prompt_preview,
                        candidate.last_prompt_preview,
                        candidate.last_activity_at,
                        observed_cwd,
                    ],
                )?;
                transaction.execute(
                    "UPDATE legacy_import_records SET payload_json = ?4
                     WHERE import_id = ?1 AND source_table = 'conversations' AND source_key = ?2
                       AND destination_id = ?3",
                    params![
                        import_id.to_string(),
                        candidate.source_conversation_id,
                        session_id.to_string(),
                        serde_json::json!({
                            "id": candidate.source_conversation_id,
                            "tool": tool_name(candidate.tool),
                            "nativeId": candidate.native_id,
                            "createdAt": candidate.discovered_at,
                            "lastActivityAt": candidate.last_activity_at,
                        })
                        .to_string(),
                    ],
                )?;
            }
            for (source_id, payload) in &summaries {
                let Some(destination) = destinations.get(source_id) else {
                    continue;
                };
                record_legacy_row_once(
                    transaction,
                    import_id,
                    "conversation_summaries",
                    source_id,
                    "session_summary",
                    destination,
                    payload,
                )?;
            }
            for (source_id, conversation_id, payload) in &cwd_observations {
                let Some(destination) = destinations.get(conversation_id) else {
                    continue;
                };
                record_legacy_row_once(
                    transaction,
                    import_id,
                    "conversation_cwd_observations",
                    source_id,
                    "session_cwd_observation",
                    destination,
                    payload,
                )?;
            }
            Ok(())
        })
    }

    fn validate_candidate_adoptions<'a>(
        &self,
        repository_id: RepositoryId,
        candidates: impl Iterator<Item = &'a LegacySessionCandidatePreview>,
    ) -> Result<(), AppError> {
        self.store.read(|connection| {
            for candidate in candidates {
                if let Some(work_item_id) = candidate.adopt_work_item_id {
                    let valid: i64 = connection.query_row(
                        "SELECT EXISTS (SELECT 1 FROM work_item_repositories
                         WHERE work_item_id = ?1 AND repository_id = ?2)",
                        params![work_item_id.to_string(), repository_id.to_string()],
                        |row| row.get(0),
                    )?;
                    if valid == 0 {
                        return Err(AppError::WorkItemRepositoryMismatch);
                    }
                }
            }
            Ok(())
        })
    }

    fn existing_legacy_import(
        &self,
        preview_hash: &str,
    ) -> Result<Option<LegacyImportOutcome>, AppError> {
        self.store.read(|connection| {
            let id = connection
                .query_row(
                    "SELECT id FROM import_batches
                     WHERE preview_hash = ?1 AND kind = 'context_catalogue'",
                    [preview_hash],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            id.map(|id| {
                let count = |kind: &str| -> Result<usize, AppError> {
                    let count: i64 = connection.query_row(
                        "SELECT COUNT(*) FROM legacy_import_records
                         WHERE import_id = ?1 AND destination_kind = ?2",
                        params![id, kind],
                        |row| row.get(0),
                    )?;
                    usize::try_from(count)
                        .map_err(|_| AppError::Domain("invalid legacy import count".to_owned()))
                };
                let adopted: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM imported_session_candidates candidate
                     JOIN import_batches batch ON batch.workspace_id = candidate.workspace_id
                     WHERE batch.id = ?1 AND candidate.status = 'confirmed'",
                    [id.as_str()],
                    |row| row.get(0),
                )?;
                Ok(LegacyImportOutcome {
                    import_id: parse_id(&id)?,
                    preview_hash: preview_hash.to_owned(),
                    repositories: count("repository")?,
                    checkouts: count("checkout")?,
                    native_sessions: count("native_session")?,
                    session_sources: count("session_source")?,
                    live_observations: count("live_observation")?,
                    adopted_sessions: usize::try_from(adopted).map_err(|_| {
                        AppError::Domain("invalid adopted session count".to_owned())
                    })?,
                    already_applied: true,
                })
            })
            .transpose()
        })
    }
}

fn import_repository_history(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    source_repository_id: &str,
    repository_id: RepositoryId,
    imported_at: &str,
) -> Result<(), AppError> {
    let repository = source.query_row(
        "SELECT id, common_dir, created_at, display_name, last_seen_at, present
         FROM repositories WHERE id = ?1",
        [source_repository_id],
        |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "commonDir": row.get::<_, String>(1)?,
                "createdAt": row.get::<_, String>(2)?,
                "displayName": row.get::<_, Option<String>>(3)?,
                "lastSeenAt": row.get::<_, Option<String>>(4)?,
                "present": row.get::<_, i64>(5)?,
            }))
        },
    )?;
    record_legacy_row(
        transaction,
        import_id,
        "repositories",
        source_repository_id,
        "repository",
        &repository_id.to_string(),
        &repository,
    )?;
    let current = transaction
        .query_row(
            "SELECT id, path FROM repository_paths
             WHERE repository_id = ?1 AND observed_until IS NULL",
            [repository_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let mut statement = source.prepare(
        "SELECT id, path, started_at, ended_at, last_verified_at
         FROM repository_path_intervals WHERE repository_id = ?1 ORDER BY started_at, id",
    )?;
    let rows = statement.query_map([source_repository_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (source_id, path, started_at, ended_at, last_verified_at) = row?;
        let destination_id = if ended_at.is_none()
            && current.as_ref().is_some_and(|(_, current_path)| {
                paths_equal(Path::new(current_path), Path::new(&path))
            }) {
            current
                .as_ref()
                .map(|(id, _)| id.clone())
                .unwrap_or_default()
        } else {
            let path_id = RepositoryPathId::generate();
            transaction.execute(
                "INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    path_id.to_string(),
                    repository_id.to_string(),
                    path,
                    started_at,
                    ended_at.as_deref().or(Some(imported_at)),
                ],
            )?;
            path_id.to_string()
        };
        record_legacy_row(
            transaction,
            import_id,
            "repository_path_intervals",
            &source_id.to_string(),
            "repository_path",
            &destination_id,
            &serde_json::json!({
                "id": source_id,
                "repositoryId": source_repository_id,
                "path": path,
                "startedAt": started_at,
                "endedAt": ended_at,
                "lastVerifiedAt": last_verified_at,
            }),
        )?;
    }
    Ok(())
}

fn import_repository_remotes(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    source_repository_id: &str,
    repository_id: RepositoryId,
) -> Result<(), AppError> {
    let mut statement = source.prepare(
        "SELECT name, url, first_seen_at, last_seen_at, present
         FROM repository_remotes WHERE repository_id = ?1 ORDER BY name, url",
    )?;
    let rows = statement.query_map([source_repository_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    for row in rows {
        let (name, url, first_seen_at, last_seen_at, present) = row?;
        transaction.execute(
            "INSERT OR IGNORE INTO repository_remotes (
                 repository_id, name, url, observed_at
             ) VALUES (?1, ?2, ?3, ?4)",
            params![repository_id.to_string(), name, url, first_seen_at],
        )?;
        let key = format!("{name}\n{url}");
        record_legacy_row(
            transaction,
            import_id,
            "repository_remotes",
            &key,
            "repository_remote",
            &repository_id.to_string(),
            &serde_json::json!({
                "repositoryId": source_repository_id,
                "name": name,
                "url": url,
                "firstSeenAt": first_seen_at,
                "lastSeenAt": last_seen_at,
                "present": present,
            }),
        )?;
    }
    Ok(())
}

fn import_checkout_paths(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    checkout_map: &HashMap<String, CheckoutId>,
) -> Result<(), AppError> {
    let mut statement = source.prepare(
        "SELECT id, worktree_id, path, started_at, ended_at, last_verified_at
         FROM worktree_path_intervals ORDER BY started_at, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (source_id, source_worktree_id, path, started_at, ended_at, last_verified_at) = row?;
        let Some(checkout_id) = checkout_map.get(&source_worktree_id).copied() else {
            continue;
        };
        let current = transaction
            .query_row(
                "SELECT id, path FROM checkout_paths
                 WHERE checkout_id = ?1 AND observed_until IS NULL",
                [checkout_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let destination_id = if ended_at.is_none()
            && current.as_ref().is_some_and(|(_, current_path)| {
                paths_equal(Path::new(current_path), Path::new(&path))
            }) {
            current
                .as_ref()
                .map(|(id, _)| id.clone())
                .unwrap_or_default()
        } else if ended_at.is_none() && current.is_some() {
            checkout_id.to_string()
        } else {
            let path_id = CheckoutPathId::generate();
            transaction.execute(
                "INSERT INTO checkout_paths (
                     id, checkout_id, path, observed_from, observed_until
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    path_id.to_string(),
                    checkout_id.to_string(),
                    path,
                    started_at,
                    ended_at,
                ],
            )?;
            path_id.to_string()
        };
        record_legacy_row(
            transaction,
            import_id,
            "worktree_path_intervals",
            &source_id.to_string(),
            "checkout_path",
            &destination_id,
            &serde_json::json!({
                "id": source_id,
                "worktreeId": source_worktree_id,
                "path": path,
                "startedAt": started_at,
                "endedAt": ended_at,
                "lastVerifiedAt": last_verified_at,
            }),
        )?;
    }
    Ok(())
}

fn import_session_sources(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    destination_sessions: &HashMap<String, ConversationId>,
) -> Result<usize, AppError> {
    let mut statement = source.prepare(
        "SELECT id, conversation_id, tool, adapter_version, path, source_size,
                modified_at_ns, byte_offset, cursor_json, snapshot_json, incomplete_tail,
                first_seen_at, last_seen_at, missing
         FROM transcript_sources ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, i64>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, i64>(13)?,
        ))
    })?;
    let mut imported = 0;
    for row in rows {
        let (
            source_id,
            source_conversation_id,
            tool,
            adapter_version,
            path,
            source_size,
            modified_at_ns,
            byte_offset,
            cursor_json,
            snapshot_json,
            incomplete_tail,
            first_seen_at,
            last_seen_at,
            missing,
        ) = row?;
        let Some(session_id) = destination_sessions.get(&source_conversation_id).copied() else {
            continue;
        };
        let existing = transaction
            .query_row(
                "SELECT session_id FROM native_session_sources WHERE path = ?1",
                [path.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if existing
            .as_deref()
            .is_some_and(|id| id != session_id.to_string())
        {
            return Err(AppError::IdempotencyConflict);
        }
        imported += transaction.execute(
            "INSERT OR IGNORE INTO native_session_sources (
                 session_id, path, adapter_version, snapshot_json, missing, observed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id.to_string(),
                path,
                adapter_version,
                snapshot_json,
                missing,
                last_seen_at,
            ],
        )?;
        record_legacy_row(
            transaction,
            import_id,
            "transcript_sources",
            &source_id.to_string(),
            "session_source",
            &session_id.to_string(),
            &serde_json::json!({
                "id": source_id,
                "conversationId": source_conversation_id,
                "tool": tool,
                "adapterVersion": adapter_version,
                "path": path,
                "sourceSize": source_size,
                "modifiedAtNs": modified_at_ns,
                "byteOffset": byte_offset,
                "cursorJson": cursor_json,
                "snapshotHash": hash_bytes(snapshot_json.as_bytes()),
                "incompleteTail": incomplete_tail,
                "firstSeenAt": first_seen_at,
                "lastSeenAt": last_seen_at,
                "missing": missing,
            }),
        )?;
    }
    Ok(imported)
}

fn import_live_observations(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    destination_sessions: &HashMap<String, ConversationId>,
) -> Result<usize, AppError> {
    let mut statement = source.prepare(
        "SELECT id, conversation_id, source, status, observed_at, expires_at, pid,
                process_created_at, executable_path, parent_pid, details_json
         FROM live_observations ORDER BY observed_at, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<i64>>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    let mut imported = 0;
    for row in rows {
        let (
            source_id,
            source_conversation_id,
            observation_source,
            status,
            observed_at,
            expires_at,
            pid,
            process_created_at,
            executable,
            parent_pid,
            details_json,
        ) = row?;
        let Some(session_id) = destination_sessions.get(&source_conversation_id).copied() else {
            continue;
        };
        let observation_id = source_id
            .parse::<workboard_core::LiveObservationId>()
            .unwrap_or_else(|_| workboard_core::LiveObservationId::generate());
        imported += transaction.execute(
            "INSERT OR IGNORE INTO live_observations (
                 id, session_id, source, status, observed_at, expires_at,
                 pid, process_created_at, executable, parent_pid
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                observation_id.to_string(),
                session_id.to_string(),
                observation_source,
                status,
                observed_at,
                expires_at,
                pid,
                process_created_at,
                executable,
                parent_pid,
            ],
        )?;
        record_legacy_row(
            transaction,
            import_id,
            "live_observations",
            &source_id,
            "live_observation",
            &observation_id.to_string(),
            &serde_json::json!({
                "id": source_id,
                "conversationId": source_conversation_id,
                "source": observation_source,
                "status": status,
                "observedAt": observed_at,
                "expiresAt": expires_at,
                "pid": pid,
                "processCreatedAt": process_created_at,
                "executablePath": executable,
                "parentPid": parent_pid,
                "detailsJson": details_json,
            }),
        )?;
    }
    Ok(imported)
}

fn import_context_evidence(
    source: &Connection,
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    destination_sessions: &HashMap<String, ConversationId>,
    checkout_map: &HashMap<String, CheckoutId>,
) -> Result<(), AppError> {
    let mut contexts = source.prepare(
        "SELECT id, conversation_id, repository_id, workstream_id, worktree_id,
                started_at, ended_at, authority, confidence, source_event_id
         FROM conversation_context_intervals ORDER BY id",
    )?;
    let rows = contexts.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            conversation_id,
            repository_id,
            workstream_id,
            worktree_id,
            started_at,
            ended_at,
            authority,
            confidence,
            source_event_id,
        ) = row?;
        let Some(session_id) = destination_sessions.get(&conversation_id) else {
            continue;
        };
        record_legacy_row(
            transaction,
            import_id,
            "conversation_context_intervals",
            &id.to_string(),
            "session_context",
            &session_id.to_string(),
            &serde_json::json!({
                "id": id,
                "conversationId": conversation_id,
                "repositoryId": repository_id,
                "workstreamId": workstream_id,
                "worktreeId": worktree_id,
                "destinationCheckoutId": checkout_map.get(&worktree_id).map(ToString::to_string),
                "startedAt": started_at,
                "endedAt": ended_at,
                "authority": authority,
                "confidence": confidence,
                "sourceEventId": source_event_id,
            }),
        )?;
    }
    let mut access = source.prepare(
        "SELECT id, conversation_id, repository_id, worktree_id, observed_at,
                recorded_at, reason, idempotency_key, evidence_json
         FROM conversation_worktree_access ORDER BY id",
    )?;
    let rows = access.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            conversation_id,
            repository_id,
            worktree_id,
            observed_at,
            recorded_at,
            reason,
            idempotency_key,
            evidence_json,
        ) = row?;
        let Some(session_id) = destination_sessions.get(&conversation_id) else {
            continue;
        };
        record_legacy_row(
            transaction,
            import_id,
            "conversation_worktree_access",
            &id.to_string(),
            "session_access",
            &session_id.to_string(),
            &serde_json::json!({
                "id": id,
                "conversationId": conversation_id,
                "repositoryId": repository_id,
                "worktreeId": worktree_id,
                "destinationCheckoutId": checkout_map.get(&worktree_id).map(ToString::to_string),
                "observedAt": observed_at,
                "recordedAt": recorded_at,
                "reason": reason,
                "idempotencyKey": idempotency_key,
                "evidenceJson": evidence_json,
            }),
        )?;
    }
    Ok(())
}

fn legacy_summary_records(
    source: &Connection,
    tables: &[String],
    selected: &HashSet<&str>,
) -> Result<Vec<(String, serde_json::Value)>, AppError> {
    if !tables.iter().any(|table| table == "conversation_summaries") {
        return Ok(Vec::new());
    }
    let mut statement = source.prepare(
        "SELECT conversation_id, native_title, first_prompt_preview, last_prompt_preview,
                tool_version, compacted, source_version, updated_at
         FROM conversation_summaries ORDER BY conversation_id",
    )?;
    let rows = statement.query_map([], |row| {
        let conversation_id = row.get::<_, String>(0)?;
        Ok((
            conversation_id.clone(),
            serde_json::json!({
                "conversationId": conversation_id,
                "nativeTitle": row.get::<_, Option<String>>(1)?,
                "firstPromptPreview": row.get::<_, Option<String>>(2)?,
                "lastPromptPreview": row.get::<_, Option<String>>(3)?,
                "toolVersion": row.get::<_, Option<String>>(4)?,
                "compacted": row.get::<_, i64>(5)?,
                "sourceVersion": row.get::<_, String>(6)?,
                "updatedAt": row.get::<_, String>(7)?,
            }),
        ))
    })?;
    rows.filter_map(|row| match row {
        Ok((source_id, payload)) if selected.contains(source_id.as_str()) => {
            Some(Ok((source_id, payload)))
        }
        Ok(_) => None,
        Err(error) => Some(Err(error.into())),
    })
    .collect()
}

fn legacy_cwd_records(
    source: &Connection,
    tables: &[String],
    selected: &HashSet<&str>,
) -> Result<Vec<(String, String, serde_json::Value)>, AppError> {
    if !tables
        .iter()
        .any(|table| table == "conversation_cwd_observations")
    {
        return Ok(Vec::new());
    }
    let mut statement = source.prepare(
        "SELECT conversation_id, path, observed_at, source_path
         FROM conversation_cwd_observations
         ORDER BY conversation_id, path, observed_at, source_path",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.filter_map(|row| match row {
        Ok((conversation_id, path, observed_at, source_path))
            if selected.contains(conversation_id.as_str()) =>
        {
            let payload = serde_json::json!({
                "conversationId": conversation_id,
                "path": path,
                "observedAt": observed_at,
                "sourcePath": source_path,
            });
            let source_id = format!(
                "{}:{}",
                conversation_id,
                hash_bytes(payload.to_string().as_bytes())
            );
            Some(Ok((source_id, conversation_id, payload)))
        }
        Ok(_) => None,
        Err(error) => Some(Err(error.into())),
    })
    .collect()
}

fn checkout_for_path(cwd: &Path, checkout_paths: &[(CheckoutId, PathBuf)]) -> Option<CheckoutId> {
    checkout_paths
        .iter()
        .filter(|(_, root)| paths_equal(cwd, root) || path_is_within(cwd, root))
        .max_by_key(|(_, root)| root.as_os_str().len())
        .map(|(checkout_id, _)| *checkout_id)
}

fn record_legacy_row_once(
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    source_table: &str,
    source_key: &str,
    destination_kind: &str,
    destination_id: &str,
    payload: &serde_json::Value,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT OR IGNORE INTO legacy_import_records (
             import_id, source_table, source_key, destination_kind,
             destination_id, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            import_id.to_string(),
            source_table,
            source_key,
            destination_kind,
            destination_id,
            payload.to_string(),
        ],
    )?;
    Ok(())
}

fn record_legacy_row(
    transaction: &Transaction<'_>,
    import_id: ImportBatchId,
    source_table: &str,
    source_key: &str,
    destination_kind: &str,
    destination_id: &str,
    payload: &serde_json::Value,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO legacy_import_records (
             import_id, source_table, source_key, destination_kind,
             destination_id, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            import_id.to_string(),
            source_table,
            source_key,
            destination_kind,
            destination_id,
            payload.to_string(),
        ],
    )?;
    Ok(())
}

fn validate_legacy_preview(preview: &LegacyImportPreview) -> Result<(), AppError> {
    validate_source_database(&preview.source)?;
    if preview.source_hash.len() != 64 || preview.repository_inventory.is_empty() {
        return Err(AppError::Domain(
            "legacy import preview is invalid".to_owned(),
        ));
    }
    let mut source_ids = HashSet::new();
    let mut native_ids = HashSet::new();
    for candidate in preview
        .session_candidates
        .iter()
        .filter(|candidate| candidate.selected)
    {
        if candidate.native_id.trim().is_empty()
            || candidate.discovered_at.trim().is_empty()
            || !source_ids.insert(candidate.source_conversation_id.clone())
            || !native_ids.insert((tool_name(candidate.tool), candidate.native_id.clone()))
        {
            return Err(AppError::Domain(
                "legacy import preview contains invalid or duplicate sessions".to_owned(),
            ));
        }
    }
    Ok(())
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
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(AppError::Domain(format!(
            "unsupported legacy native provider: {value}"
        ))),
    }
}

const fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn short_branch(branch: &str) -> &str {
    branch.strip_prefix("refs/heads/").unwrap_or(branch)
}

fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path).map_err(|source| AppError::StorageIo {
        operation: "reading the legacy database",
        path: path.to_path_buf(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| AppError::StorageIo {
                operation: "hashing the legacy database",
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
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
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(windows)]
fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = windows_path_key(path);
    let root = windows_path_key(root);
    path.strip_prefix(&root)
        .is_some_and(|remainder| remainder.starts_with('\\'))
}

#[cfg(windows)]
fn windows_path_key(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut text = canonical
        .as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase();
    if let Some(remainder) = text.strip_prefix(r"\\?\unc\") {
        text = format!(r"\\{remainder}");
    } else if let Some(remainder) = text.strip_prefix(r"\\?\") {
        text = remainder.to_owned();
    }
    text.trim_end_matches('\\').to_owned()
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(not(windows))]
fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    path.starts_with(root) && path != root
}

fn count_first_present(
    connection: &Connection,
    tables: &[String],
    candidates: &[&str],
) -> Result<u64, AppError> {
    let table = candidates
        .iter()
        .find(|candidate| tables.iter().any(|table| table == **candidate));
    match table {
        Some(table) => count_table(connection, table),
        None => Ok(0),
    }
}

fn count_if_present(
    connection: &Connection,
    tables: &[String],
    table: &str,
) -> Result<u64, AppError> {
    if tables.iter().any(|candidate| candidate == table) {
        count_table(connection, table)
    } else {
        Ok(0)
    }
}

fn count_table(connection: &Connection, table: &str) -> Result<u64, AppError> {
    let permitted = [
        "repositories",
        "conversations",
        "native_sessions",
        "association_events",
        "conversation_context_intervals",
        "worktrees",
        "checkouts",
    ];
    if !permitted.contains(&table) {
        return Err(AppError::Domain("legacy table is unsupported".to_owned()));
    }
    let count = connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .unwrap_or(0);
    u64::try_from(count).map_err(|_| AppError::Domain("legacy row count is invalid".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use rusqlite::{Connection, params};
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{FeatureId, Slug, WorkItemId};

    use super::{preview_context_catalogue, snapshot_context_catalogue};
    use crate::workspace::{
        CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
    };

    #[test]
    fn preview_reads_known_inventory_without_mutating_the_source_database() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("legacy.sqlite");
        let connection = Connection::open(&path).expect("create legacy database");
        connection
            .execute_batch(
                "CREATE TABLE repositories (
                     id TEXT PRIMARY KEY, common_dir TEXT NOT NULL, display_name TEXT
                 );
                 CREATE TABLE conversations (
                     id TEXT PRIMARY KEY, tool TEXT NOT NULL, native_id TEXT NOT NULL,
                     created_at TEXT NOT NULL
                 );
                 CREATE TABLE association_events (id TEXT PRIMARY KEY);
                 CREATE TABLE worktrees (id TEXT PRIMARY KEY);
                 INSERT INTO repositories VALUES ('repository', 'C:/repo/.git', 'Repo');
                 INSERT INTO conversations VALUES
                     ('one', 'codex', 'native-one', '1'),
                     ('two', 'claude', 'native-two', '2');
                 INSERT INTO association_events VALUES ('association');
                 INSERT INTO worktrees VALUES ('checkout');",
            )
            .expect("seed legacy database");
        drop(connection);
        let before = Sha256::digest(fs::read(&path).expect("read source before preview"));
        let preview = preview_context_catalogue(&path).expect("preview import");
        let after = Sha256::digest(fs::read(&path).expect("read source after preview"));

        assert_eq!(preview.repositories, 1);
        assert_eq!(preview.native_sessions, 2);
        assert_eq!(preview.association_events, 1);
        assert_eq!(preview.checkouts, 1);
        assert_eq!(preview.session_candidates.len(), 2);
        assert_eq!(before, after);
        assert!(!path.with_extension("sqlite-wal").exists());
        assert!(!path.with_extension("sqlite-shm").exists());
    }

    #[test]
    fn verified_backup_imports_evidence_and_supports_explicit_adoption() {
        let directory = TempDir::new().expect("temporary directory");
        let source_repository = directory.path().join("source");
        fs::create_dir_all(&source_repository).expect("create source repository");
        successful(
            Command::new("git")
                .arg("init")
                .args(["-b", "main"])
                .arg(&source_repository),
        );
        fs::write(source_repository.join("README.md"), "# Source\n").expect("write source file");
        successful(
            Command::new("git")
                .arg("-C")
                .arg(&source_repository)
                .args(["add", "."]),
        );
        successful(
            Command::new("git")
                .arg("-C")
                .arg(&source_repository)
                .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
                .args(["commit", "-m", "Seed"]),
        );
        let common_directory = source_repository
            .join(".git")
            .canonicalize()
            .expect("canonical Git directory");
        let legacy = directory.path().join("legacy.sqlite");
        create_legacy_database(&legacy, &source_repository, &common_directory);
        let backup = directory.path().join("backup/catalogue.sqlite");
        let preview = snapshot_context_catalogue(&legacy, &backup).expect("snapshot catalogue");
        let database = directory.path().join("workboard.sqlite");
        let planning_store = directory.path().join("planning-store");
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
                slug: Slug::new("source").expect("repository slug"),
                title: "Source".to_owned(),
                path: source_repository.clone(),
            })
            .expect("register repository");
        let first = application
            .apply_context_catalogue_import(workspace.workspace.id, repository.id, &preview)
            .expect("apply legacy import");
        let second = application
            .apply_context_catalogue_import(workspace.workspace.id, repository.id, &preview)
            .expect("repeat legacy import");
        let candidates = application
            .imported_session_candidates(workspace.workspace.id)
            .expect("list candidates");

        assert_eq!(first.checkouts, 1);
        assert_eq!(first.native_sessions, 1);
        assert_eq!(first.session_sources, 1);
        assert_eq!(first.live_observations, 1);
        assert!(second.already_applied);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].status, "unassigned");
        assert_eq!(candidates[0].native_title.as_deref(), Some("Native Launch"));
        assert_eq!(
            candidates[0].first_prompt_preview.as_deref(),
            Some("Implement the launch flow")
        );
        assert_eq!(
            candidates[0].last_prompt_preview.as_deref(),
            Some("Verify the launch flow")
        );
        assert_eq!(candidates[0].last_activity_at.as_deref(), Some("250"));
        assert_eq!(
            candidates[0].observed_cwd.as_deref(),
            Some(source_repository.as_path())
        );
        assert!(candidates[0].checkout_id.is_some());
        assert_eq!(
            candidates[0].legacy_workstream_title.as_deref(),
            Some("Launch")
        );
        let reconstruction_rows = application
            .store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM legacy_import_records
                         WHERE import_id = ?1 AND source_table IN (
                             'conversation_summaries', 'conversation_cwd_observations'
                         )",
                        [first.import_id.to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(Into::into)
            })
            .expect("count reconstruction rows");
        assert_eq!(reconstruction_rows, 2);

        let epic = application
            .create_epic(CreateEpic {
                workspace_id: workspace.workspace.id,
                slug: Slug::new("legacy").expect("Epic slug"),
                title: "Legacy".to_owned(),
                body: "# Legacy\n\nImported work.\n".to_owned(),
            })
            .expect("create Epic");
        let feature_id = FeatureId::generate();
        let work_item_id = WorkItemId::generate();
        application
            .store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
                     VALUES (?1, ?2, 'launch', 'Launch', 'planned', '1')",
                    params![feature_id.to_string(), epic.id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_items (id, feature_id, key, slug, title, status, created_at)
                     VALUES (?1, ?2, 'legacy/launch/adopted', 'adopted', 'Adopted', 'ready', '1')",
                    params![work_item_id.to_string(), feature_id.to_string()],
                )?;
                transaction.execute(
                    "INSERT INTO work_item_repositories (work_item_id, repository_id)
                     VALUES (?1, ?2)",
                    params![work_item_id.to_string(), repository.id.to_string()],
                )?;
                Ok(())
            })
            .expect("seed adoption target");
        application
            .adopt_imported_session_candidate(
                workspace.workspace.id,
                candidates[0].session_id,
                work_item_id,
                OffsetDateTime::now_utc(),
            )
            .expect("adopt imported session");
        let adopted = application
            .imported_session_candidates(workspace.workspace.id)
            .expect("list adopted candidate");
        assert_eq!(adopted[0].status, "confirmed");
    }

    fn create_legacy_database(path: &Path, repository: &Path, common_directory: &Path) {
        let connection = Connection::open(path).expect("create legacy database");
        connection
            .execute_batch(
                "CREATE TABLE repositories (
                     id TEXT PRIMARY KEY, common_dir TEXT NOT NULL, created_at TEXT NOT NULL,
                     display_name TEXT, last_seen_at TEXT, present INTEGER NOT NULL
                 );
                 CREATE TABLE repository_path_intervals (
                     id INTEGER PRIMARY KEY, repository_id TEXT NOT NULL, path TEXT NOT NULL,
                     started_at TEXT NOT NULL, ended_at TEXT, last_verified_at TEXT NOT NULL
                 );
                 CREATE TABLE repository_remotes (
                     repository_id TEXT NOT NULL, name TEXT NOT NULL, url TEXT NOT NULL,
                     first_seen_at TEXT NOT NULL, last_seen_at TEXT NOT NULL, present INTEGER NOT NULL
                 );
                 CREATE TABLE worktrees (
                     id TEXT PRIMARY KEY, repository_id TEXT NOT NULL, git_dir TEXT NOT NULL,
                     path TEXT NOT NULL, branch TEXT, head_oid TEXT NOT NULL,
                     last_seen_at TEXT NOT NULL, present INTEGER NOT NULL
                 );
                 CREATE TABLE worktree_path_intervals (
                     id INTEGER PRIMARY KEY, worktree_id TEXT NOT NULL, path TEXT NOT NULL,
                     started_at TEXT NOT NULL, ended_at TEXT, last_verified_at TEXT NOT NULL
                 );
                 CREATE TABLE conversations (
                     id TEXT PRIMARY KEY, tool TEXT NOT NULL, native_id TEXT NOT NULL,
                     created_at TEXT NOT NULL, last_activity_at TEXT
                 );
                 CREATE TABLE conversation_summaries (
                     conversation_id TEXT PRIMARY KEY, native_title TEXT,
                     first_prompt_preview TEXT, last_prompt_preview TEXT, tool_version TEXT,
                     compacted INTEGER NOT NULL, source_version TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );
                 CREATE TABLE conversation_cwd_observations (
                     conversation_id TEXT NOT NULL, path TEXT NOT NULL, observed_at TEXT,
                     source_path TEXT NOT NULL,
                     PRIMARY KEY(conversation_id, path, observed_at, source_path)
                 );
                 CREATE TABLE transcript_sources (
                     id INTEGER PRIMARY KEY, conversation_id TEXT NOT NULL, tool TEXT NOT NULL,
                     adapter_version TEXT NOT NULL, path TEXT NOT NULL, source_size INTEGER NOT NULL,
                     modified_at_ns TEXT, byte_offset INTEGER NOT NULL, cursor_json TEXT NOT NULL,
                     snapshot_json TEXT NOT NULL, incomplete_tail INTEGER NOT NULL,
                     first_seen_at TEXT NOT NULL, last_seen_at TEXT NOT NULL, missing INTEGER NOT NULL
                 );
                 CREATE TABLE live_observations (
                     id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, source TEXT NOT NULL,
                     status TEXT NOT NULL, observed_at TEXT NOT NULL, expires_at TEXT NOT NULL,
                     pid INTEGER, process_created_at TEXT, executable_path TEXT, parent_pid INTEGER,
                     details_json TEXT NOT NULL
                 );
                 CREATE TABLE workstreams (
                     id TEXT PRIMARY KEY, repository_id TEXT NOT NULL, title TEXT
                 );
                 CREATE TABLE association_events (id TEXT PRIMARY KEY, confirmed INTEGER NOT NULL);
                 CREATE TABLE conversation_context_intervals (
                     id INTEGER PRIMARY KEY, conversation_id TEXT NOT NULL,
                     repository_id TEXT NOT NULL, workstream_id TEXT NOT NULL,
                     worktree_id TEXT NOT NULL, started_at TEXT NOT NULL, ended_at TEXT,
                     authority TEXT NOT NULL, confidence TEXT NOT NULL, source_event_id TEXT NOT NULL
                 );
                 CREATE TABLE conversation_worktree_access (
                     id INTEGER PRIMARY KEY, conversation_id TEXT NOT NULL,
                     repository_id TEXT NOT NULL, worktree_id TEXT NOT NULL,
                     observed_at TEXT NOT NULL, recorded_at TEXT NOT NULL,
                     reason TEXT NOT NULL, idempotency_key TEXT NOT NULL, evidence_json TEXT NOT NULL
                 );",
            )
            .expect("create legacy schema");
        let repository_text = text_path(repository);
        let common_text = text_path(common_directory);
        connection
            .execute(
                "INSERT INTO repositories VALUES ('repo', ?1, '100', 'Source', '200', 1)",
                [common_text.as_str()],
            )
            .expect("insert repository");
        connection
            .execute(
                "INSERT INTO repository_path_intervals
                 VALUES (1, 'repo', ?1, '100', NULL, '200')",
                [repository_text.as_str()],
            )
            .expect("insert repository path");
        connection
            .execute(
                "INSERT INTO repository_remotes
                 VALUES ('repo', 'origin', 'https://example.test/source.git', '100', '200', 1)",
                [],
            )
            .expect("insert remote");
        connection
            .execute(
                "INSERT INTO worktrees VALUES (
                     '11111111-1111-4111-8111-111111111111', 'repo', ?1, ?2,
                     'refs/heads/main', 'abc123', '200', 1
                 )",
                params![common_text, repository_text],
            )
            .expect("insert worktree");
        connection
            .execute(
                "INSERT INTO worktree_path_intervals VALUES (
                     1, '11111111-1111-4111-8111-111111111111', ?1, '100', NULL, '200'
                 )",
                [repository_text.as_str()],
            )
            .expect("insert worktree path");
        connection
            .execute_batch(
                "INSERT INTO conversations VALUES (
                     '22222222-2222-4222-8222-222222222222', 'codex', 'native-session', '100', '250'
                 );
                 INSERT INTO conversation_summaries VALUES (
                     '22222222-2222-4222-8222-222222222222', 'Native Launch',
                     'Implement the launch flow', 'Verify the launch flow', '1.0', 0, '1', '250'
                 );
                 INSERT INTO transcript_sources VALUES (
                     1, '22222222-2222-4222-8222-222222222222', 'codex', '1',
                     'C:/transcripts/session.jsonl', 20, '100', 20, '{}', '{\"cwd\":\"C:/repo\"}',
                     0, '100', '200', 0
                 );
                 INSERT INTO live_observations VALUES (
                     '33333333-3333-4333-8333-333333333333',
                     '22222222-2222-4222-8222-222222222222', 'hook', 'confirmed',
                     '100', '200', 10, '100', 'codex.exe', 1, '{}'
                 );
                 INSERT INTO workstreams VALUES ('workstream', 'repo', 'Launch');
                 INSERT INTO association_events VALUES ('event', 1);
                 INSERT INTO conversation_context_intervals VALUES (
                     1, '22222222-2222-4222-8222-222222222222', 'repo', 'workstream',
                     '11111111-1111-4111-8111-111111111111', '100', NULL,
                     'explicit', 'confirmed', 'event'
                 );
                 INSERT INTO conversation_worktree_access VALUES (
                     1, '22222222-2222-4222-8222-222222222222', 'repo',
                     '11111111-1111-4111-8111-111111111111', '100', '100',
                     'cwd', 'access-one', '{}'
                 );",
            )
            .expect("insert legacy evidence");
        connection
            .execute(
                "INSERT INTO conversation_cwd_observations VALUES (
                     '22222222-2222-4222-8222-222222222222', ?1, '240',
                     'C:/transcripts/session.jsonl'
                 )",
                [repository_text.as_str()],
            )
            .expect("insert legacy CWD observation");
    }

    fn successful(command: &mut Command) {
        let output = command.output().expect("run Git");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn text_path(path: &Path) -> String {
        path.to_str().expect("UTF-8 path").to_owned()
    }
}
