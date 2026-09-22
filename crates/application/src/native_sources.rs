use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_adapter_claude::ClaudeAdapterV1;
use workboard_adapter_codex::CodexAdapterV1;
use workboard_core::{ConversationId, Tool};
use workboard_native::{ConversationKind, NativeAdapter};

use crate::AppError;
use crate::native_launch::{ResumeContext, ResumeSource};
use crate::storage::SqliteStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshNativeSources {
    pub tool: Tool,
    pub root: PathBuf,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSourceFailure {
    pub path: PathBuf,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRefreshOutcome {
    pub tool: Tool,
    pub inventory_count: usize,
    pub source_count: usize,
    pub conversation_count: usize,
    pub failures: Vec<NativeSourceFailure>,
}

pub struct NativeSourceService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> NativeSourceService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn refresh(
        &mut self,
        request: RefreshNativeSources,
    ) -> Result<NativeRefreshOutcome, AppError> {
        if !request.root.is_absolute() || !request.root.is_dir() {
            return Err(AppError::IntegrationPathInvalid {
                label: "native source root",
                path: request.root,
            });
        }
        let scan = match request.tool {
            Tool::Claude => ClaudeAdapterV1::default().scan(&request.root, &HashMap::new()),
            Tool::Codex => CodexAdapterV1::default().scan(&request.root, &HashMap::new()),
        }
        .map_err(|failure| AppError::Adapter {
            tool: tool_title(request.tool),
            message: failure.message,
        })?;
        let observed_at = timestamp(request.observed_at);
        let inventory = scan
            .inventory
            .iter()
            .map(|path| path_text(path).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        let mut conversations = 0usize;
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE native_session_sources SET missing = 1, observed_at = ?2
                 WHERE session_id IN (
                     SELECT id FROM native_sessions WHERE provider = ?1
                 )",
                params![tool_name(request.tool), observed_at],
            )?;
            for source in &scan.sources {
                if source.conversation.kind != ConversationKind::TopLevel {
                    continue;
                }
                conversations += 1;
                let existing = transaction
                    .query_row(
                        "SELECT id FROM native_sessions WHERE provider = ?1 AND native_id = ?2",
                        params![tool_name(request.tool), source.conversation.native_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let session_id = existing.unwrap_or_else(|| ConversationId::generate().to_string());
                transaction.execute(
                    "INSERT OR IGNORE INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        session_id,
                        tool_name(request.tool),
                        source.conversation.native_id,
                        observed_at,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_sources (
                         session_id, path, adapter_version, snapshot_json, missing, observed_at
                     ) VALUES (?1, ?2, ?3, ?4, 0, ?5)
                     ON CONFLICT(path) DO UPDATE SET
                         session_id = excluded.session_id,
                         adapter_version = excluded.adapter_version,
                         snapshot_json = excluded.snapshot_json,
                         missing = 0,
                         observed_at = excluded.observed_at",
                    params![
                        session_id,
                        path_text(&source.path)?,
                        scan.adapter_version,
                        serde_json::to_string(&source.conversation)?,
                        observed_at,
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(NativeRefreshOutcome {
            tool: request.tool,
            inventory_count: inventory.len(),
            source_count: scan.sources.len(),
            conversation_count: conversations,
            failures: scan
                .failures
                .into_iter()
                .map(|failure| NativeSourceFailure {
                    path: failure.path,
                    code: format!("{:?}", failure.kind).to_ascii_lowercase(),
                    message: failure.message,
                })
                .collect(),
        })
    }

    pub fn resume_context(
        &self,
        session_id: ConversationId,
        working_directory: PathBuf,
        title: String,
    ) -> Result<ResumeContext, AppError> {
        let sources = self.store.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT path, missing, snapshot_json FROM native_session_sources
                 WHERE session_id = ?1 ORDER BY missing, observed_at DESC, path",
            )?;
            let rows = statement
                .query_map([session_id.to_string()], |row| {
                    Ok(ResumeSource {
                        path: PathBuf::from(row.get::<_, String>(0)?),
                        missing: row.get::<_, i64>(1)? != 0,
                        snapshot_json: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;
        if sources.is_empty() {
            return Err(AppError::ConversationNotResumable(
                "no recorded native source is available".to_owned(),
            ));
        }
        Ok(ResumeContext {
            working_directory,
            title,
            sources,
        })
    }
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

fn timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339 timestamps always format")
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn tool_title(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "Claude",
        Tool::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::{ConversationId, ConversationRef, Tool};

    use super::{NativeSourceService, RefreshNativeSources};
    use crate::native_launch::validate_native_source;
    use crate::storage::SqliteStore;

    #[test]
    fn refresh_persists_exact_resume_evidence_and_marks_missing_sources() {
        let directory = TempDir::new().expect("temporary directory");
        let native_root = directory.path().join("sessions");
        let checkout = directory.path().join("checkout");
        fs::create_dir_all(&native_root).expect("native root");
        fs::create_dir(&checkout).expect("checkout");
        let source = native_root.join("thread.jsonl");
        fs::write(
            &source,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-one\",\"cwd\":\"{}\"}}}}\n",
                checkout.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .expect("native source");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let observed_at = OffsetDateTime::parse(
            "2026-08-27T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("timestamp");
        let outcome = NativeSourceService::new(&mut store)
            .refresh(RefreshNativeSources {
                tool: Tool::Codex,
                root: native_root.clone(),
                observed_at,
            })
            .expect("refresh sources");
        assert_eq!(outcome.conversation_count, 1);
        assert!(outcome.failures.is_empty());
        let session_id = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT id FROM native_sessions WHERE native_id = 'thread-one'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(Into::into)
            })
            .and_then(|id| {
                id.parse::<ConversationId>()
                    .map_err(|error| crate::AppError::Domain(error.to_string()))
            })
            .expect("session ID");
        let context = NativeSourceService::new(&mut store)
            .resume_context(session_id, checkout, "Thread one".to_owned())
            .expect("resume context");
        validate_native_source(
            &ConversationRef::new(Tool::Codex, "thread-one").expect("conversation"),
            &context,
        )
        .expect("read-only preflight");

        fs::remove_file(source).expect("remove native source");
        NativeSourceService::new(&mut store)
            .refresh(RefreshNativeSources {
                tool: Tool::Codex,
                root: native_root,
                observed_at: observed_at + time::Duration::minutes(1),
            })
            .expect("refresh missing source");
        let context = NativeSourceService::new(&mut store)
            .resume_context(
                session_id,
                directory.path().to_path_buf(),
                "Thread one".to_owned(),
            )
            .expect("missing context remains visible");
        assert!(context.sources[0].missing);
    }
}
