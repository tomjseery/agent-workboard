use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{
    Connection, ErrorCode, MAIN_DB, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use workboard_core::{ConversationId, LaunchLeaseId};

use crate::AppError;

const CURRENT_SCHEMA_VERSION: i64 = 9;
const FOUNDATION_SCHEMA_CHECKSUM: &str = "agent-workboard-foundation-v1";
const LAUNCH_LEASE_SCHEMA_CHECKSUM: &str = "agent-workboard-launch-leases-v1";
const WORKBOARD_DOMAIN_SCHEMA_CHECKSUM: &str = "agent-workboard-domain-v1";
const MANAGED_BINDING_SCHEMA_CHECKSUM: &str = "agent-workboard-managed-binding-v1";
const NATIVE_SOURCE_SCHEMA_CHECKSUM: &str = "agent-workboard-native-source-v1";
const INTEGRATION_STATE_SCHEMA_CHECKSUM: &str = "agent-workboard-integration-state-v1";
const FEATURE_PLANNING_SCHEMA_CHECKSUM: &str = "agent-workboard-feature-planning-v1";
const WORKFLOW_CREDENTIAL_SCHEMA_CHECKSUM: &str = "agent-workboard-workflow-credential-v1";
const SESSION_REQUEST_SCHEMA_CHECKSUM: &str = "agent-workboard-session-request-v1";

pub struct SqliteStore {
    path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageHealth {
    pub integrity: String,
    pub foreign_key_violations: usize,
    pub schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquiredLaunchLease {
    pub id: LaunchLeaseId,
    pub conversation_id: ConversationId,
    #[serde(with = "time::serde::rfc3339")]
    pub acquired_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

impl StorageHealth {
    pub fn is_healthy(&self) -> bool {
        self.integrity == "ok"
            && self.foreign_key_violations == 0
            && self.schema_version == CURRENT_SCHEMA_VERSION
    }
}

impl SqliteStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AppError::StorageIo {
                operation: "creating the database directory",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&connection)?;
        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        operation(&self.connection)
    }

    pub fn write<T>(
        &mut self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = operation(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn health(&self) -> Result<StorageHealth, AppError> {
        health(&self.connection)
    }

    pub fn backup(&self, destination: &Path) -> Result<StorageHealth, AppError> {
        if destination.exists() {
            return Err(AppError::Domain(format!(
                "backup destination already exists: {}",
                destination.display()
            )));
        }
        let parent = destination.parent().ok_or_else(|| {
            AppError::Domain(format!(
                "backup destination has no parent: {}",
                destination.display()
            ))
        })?;
        fs::create_dir_all(parent).map_err(|source| AppError::StorageIo {
            operation: "creating the backup directory",
            path: parent.to_path_buf(),
            source,
        })?;
        let partial = destination.with_extension("partial");
        if partial.exists() {
            return Err(AppError::Domain(format!(
                "partial backup destination already exists: {}",
                partial.display()
            )));
        }
        let result = (|| {
            self.connection.backup(MAIN_DB, &partial, None)?;
            let verification = Connection::open(&partial)?;
            verification.pragma_update(None, "foreign_keys", "ON")?;
            let health = health(&verification)?;
            if !health.is_healthy() {
                return Err(AppError::Domain(format!(
                    "backup verification failed: {health:?}"
                )));
            }
            drop(verification);
            fs::rename(&partial, destination).map_err(|source| AppError::StorageIo {
                operation: "publishing the verified backup",
                path: destination.to_path_buf(),
                source,
            })?;
            Ok(health)
        })();
        if result.is_err() && partial.exists() {
            drop(fs::remove_file(&partial));
        }
        result
    }

    pub fn repair(&mut self) -> Result<StorageHealth, AppError> {
        self.connection.execute_batch("REINDEX; PRAGMA optimize;")?;
        let health = self.health()?;
        if !health.is_healthy() {
            return Err(AppError::Domain(format!(
                "storage repair did not produce a healthy database: {health:?}"
            )));
        }
        Ok(health)
    }

    pub fn acquire_launch_lease(
        &mut self,
        conversation_id: ConversationId,
        working_directory: &Path,
        launch_json: &str,
        acquired_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<AcquiredLaunchLease, AppError> {
        if !working_directory.is_absolute() {
            return Err(AppError::WorktreePathNotAbsolute(
                working_directory.to_path_buf(),
            ));
        }
        if launch_json.trim().is_empty() || expires_at <= acquired_at {
            return Err(AppError::Domain("launch lease input is invalid".to_owned()));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE launch_leases SET status = 'expired'
             WHERE status = 'pending' AND CAST(expires_at AS INTEGER) <= ?1",
            [timestamp(acquired_at)],
        )?;
        let id = LaunchLeaseId::generate();
        let inserted = transaction.execute(
            "INSERT INTO launch_leases (
                 id, conversation_id, acquired_at, expires_at, status,
                 working_directory, launch_json
             ) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
            params![
                id.to_string(),
                conversation_id.to_string(),
                timestamp(acquired_at),
                timestamp(expires_at),
                working_directory.to_string_lossy(),
                launch_json,
            ],
        );
        if matches!(
            inserted,
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::ConstraintViolation
        ) {
            return Err(AppError::DuplicateConfirmed);
        }
        inserted?;
        transaction.commit()?;
        Ok(AcquiredLaunchLease {
            id,
            conversation_id,
            acquired_at,
            expires_at,
        })
    }

    pub fn complete_launch_lease(
        &mut self,
        lease_id: LaunchLeaseId,
        terminal_pid: u32,
    ) -> Result<(), AppError> {
        if terminal_pid == 0 {
            return Err(AppError::Domain("terminal PID cannot be zero".to_owned()));
        }
        let updated = self.connection.execute(
            "UPDATE launch_leases SET status = 'completed', terminal_pid = ?2
             WHERE id = ?1 AND status = 'pending'",
            params![lease_id.to_string(), terminal_pid],
        )?;
        if updated != 1 {
            return Err(AppError::LaunchLeaseLost);
        }
        Ok(())
    }

    pub fn fail_launch_lease(
        &mut self,
        lease_id: LaunchLeaseId,
        failure: &str,
    ) -> Result<(), AppError> {
        if failure.trim().is_empty() {
            return Err(AppError::Domain(
                "launch failure cannot be blank".to_owned(),
            ));
        }
        let updated = self.connection.execute(
            "UPDATE launch_leases SET status = 'failed', failure = ?2
             WHERE id = ?1 AND status = 'pending'",
            params![lease_id.to_string(), failure],
        )?;
        if updated != 1 {
            return Err(AppError::LaunchLeaseLost);
        }
        Ok(())
    }
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn migrate(connection: &Connection) -> Result<(), AppError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             checksum TEXT NOT NULL,
             applied_at TEXT NOT NULL
         );",
    )?;
    apply_migration(connection, 1, FOUNDATION_SCHEMA_CHECKSUM, "")?;
    apply_migration(
        connection,
        2,
        LAUNCH_LEASE_SCHEMA_CHECKSUM,
        "CREATE TABLE launch_leases (
             id TEXT PRIMARY KEY,
             conversation_id TEXT NOT NULL,
             acquired_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             status TEXT NOT NULL CHECK (status IN ('pending', 'completed', 'failed', 'expired')),
             working_directory TEXT NOT NULL,
             launch_json TEXT NOT NULL,
             terminal_pid INTEGER,
             failure TEXT
         );
         CREATE UNIQUE INDEX launch_leases_one_pending_per_conversation
             ON launch_leases (conversation_id) WHERE status = 'pending';",
    )?;
    apply_migration(
        connection,
        3,
        WORKBOARD_DOMAIN_SCHEMA_CHECKSUM,
        r#"CREATE TABLE workspaces (
             id TEXT PRIMARY KEY,
             slug TEXT NOT NULL UNIQUE CHECK (slug <> ''),
             title TEXT NOT NULL CHECK (title <> ''),
             planning_store_repository_id TEXT NOT NULL UNIQUE,
             created_at TEXT NOT NULL,
             FOREIGN KEY (planning_store_repository_id) REFERENCES repositories(id)
                 DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TABLE repositories (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT
                 DEFERRABLE INITIALLY DEFERRED,
             slug TEXT NOT NULL CHECK (slug <> ''),
             title TEXT NOT NULL CHECK (title <> ''),
             git_common_directory TEXT NOT NULL CHECK (git_common_directory <> ''),
             default_branch TEXT,
             is_planning_store INTEGER NOT NULL CHECK (is_planning_store IN (0, 1)),
             created_at TEXT NOT NULL,
             UNIQUE (workspace_id, slug),
             UNIQUE (git_common_directory)
         );
         CREATE TABLE repository_paths (
             id TEXT PRIMARY KEY,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             path TEXT NOT NULL CHECK (path <> ''),
             observed_from TEXT NOT NULL,
             observed_until TEXT,
             CHECK (observed_until IS NULL OR observed_until > observed_from)
         );
         CREATE UNIQUE INDEX repository_paths_one_current
             ON repository_paths (repository_id) WHERE observed_until IS NULL;
         CREATE UNIQUE INDEX repository_paths_current_path
             ON repository_paths (path) WHERE observed_until IS NULL;
         CREATE TRIGGER repository_paths_no_delete
         BEFORE DELETE ON repository_paths
         BEGIN
             SELECT RAISE(ABORT, 'repository path history cannot be deleted');
         END;
         CREATE TRIGGER repository_paths_no_rewrite
         BEFORE UPDATE ON repository_paths
         WHEN OLD.observed_until IS NOT NULL OR
              NEW.id <> OLD.id OR
              NEW.repository_id <> OLD.repository_id OR
              NEW.path <> OLD.path OR
              NEW.observed_from <> OLD.observed_from OR
              NEW.observed_until IS NULL OR
              NEW.observed_until <= OLD.observed_from
         BEGIN
             SELECT RAISE(ABORT, 'repository path history cannot be rewritten');
         END;
         CREATE TABLE repository_remotes (
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
             name TEXT NOT NULL CHECK (name <> ''),
             url TEXT NOT NULL CHECK (url <> ''),
             observed_at TEXT NOT NULL,
             PRIMARY KEY (repository_id, name, url)
         );
         CREATE TABLE epics (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
             slug TEXT NOT NULL CHECK (slug <> ''),
             title TEXT NOT NULL CHECK (title <> ''),
             created_at TEXT NOT NULL,
             UNIQUE (workspace_id, slug)
         );
         CREATE TABLE features (
             id TEXT PRIMARY KEY,
             epic_id TEXT NOT NULL REFERENCES epics(id) ON DELETE RESTRICT,
             slug TEXT NOT NULL CHECK (slug <> ''),
             title TEXT NOT NULL CHECK (title <> ''),
             workflow_state TEXT NOT NULL,
             created_at TEXT NOT NULL,
             UNIQUE (epic_id, slug)
         );
         CREATE TABLE work_items (
             id TEXT PRIMARY KEY,
             feature_id TEXT NOT NULL REFERENCES features(id) ON DELETE RESTRICT,
             key TEXT NOT NULL UNIQUE CHECK (key <> ''),
             slug TEXT NOT NULL CHECK (slug <> ''),
             title TEXT NOT NULL CHECK (title <> ''),
             status TEXT NOT NULL CHECK (
                 status IN ('backlog', 'ready', 'in_progress', 'blocked', 'review', 'done', 'cancelled')
             ),
             created_at TEXT NOT NULL,
             UNIQUE (feature_id, slug)
         );
         CREATE TABLE work_item_repositories (
             work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             PRIMARY KEY (work_item_id, repository_id)
         );
         CREATE TABLE documents (
             id TEXT PRIMARY KEY,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             kind TEXT NOT NULL CHECK (kind IN ('epic', 'feature', 'work_item')),
             relative_path TEXT NOT NULL CHECK (
                 relative_path <> '' AND
                 relative_path NOT LIKE '/%' AND
                 relative_path NOT LIKE '\\%' AND
                 relative_path NOT LIKE '%/../%' AND
                 relative_path NOT LIKE '../%' AND
                 relative_path <> '..'
             ),
             content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
             observed_commit TEXT,
             observed_at TEXT NOT NULL,
             CHECK (
                 (kind = 'epic' AND epic_id IS NOT NULL AND feature_id IS NULL AND work_item_id IS NULL) OR
                 (kind = 'feature' AND epic_id IS NULL AND feature_id IS NOT NULL AND work_item_id IS NULL) OR
                 (kind = 'work_item' AND epic_id IS NULL AND feature_id IS NULL AND work_item_id IS NOT NULL)
             ),
             UNIQUE (repository_id, relative_path),
             UNIQUE (epic_id),
             UNIQUE (feature_id),
             UNIQUE (work_item_id)
         );
         CREATE TABLE document_revisions (
             document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE RESTRICT,
             revision INTEGER NOT NULL CHECK (revision > 0),
             content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
             observed_commit TEXT,
             observed_at TEXT NOT NULL,
             PRIMARY KEY (document_id, revision),
             UNIQUE (document_id, content_hash)
         );
         CREATE TABLE checkouts (
             id TEXT PRIMARY KEY,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             git_worktree_identity TEXT NOT NULL CHECK (git_worktree_identity <> ''),
             branch TEXT,
             head TEXT,
             availability TEXT NOT NULL CHECK (
                 availability IN ('available', 'missing', 'deleted', 'replaced')
             ),
             replaces_checkout_id TEXT REFERENCES checkouts(id) ON DELETE RESTRICT,
             created_intent_id TEXT,
             created_at TEXT NOT NULL,
             CHECK (replaces_checkout_id IS NULL OR replaces_checkout_id <> id),
             UNIQUE (repository_id, git_worktree_identity)
         );
         CREATE TRIGGER checkouts_validate_replacement
         BEFORE INSERT ON checkouts
         WHEN NEW.replaces_checkout_id IS NOT NULL
         BEGIN
             SELECT CASE WHEN NOT EXISTS (
                 SELECT 1 FROM checkouts replaced
                 WHERE replaced.id = NEW.replaces_checkout_id
                   AND replaced.repository_id = NEW.repository_id
             ) THEN RAISE(ABORT, 'replacement checkout must belong to the same repository') END;
         END;
         CREATE TABLE checkout_paths (
             id TEXT PRIMARY KEY,
             checkout_id TEXT NOT NULL REFERENCES checkouts(id) ON DELETE RESTRICT,
             path TEXT NOT NULL CHECK (path <> ''),
             observed_from TEXT NOT NULL,
             observed_until TEXT,
             CHECK (observed_until IS NULL OR observed_until > observed_from)
         );
         CREATE UNIQUE INDEX checkout_paths_one_current
             ON checkout_paths (checkout_id) WHERE observed_until IS NULL;
         CREATE UNIQUE INDEX checkout_paths_current_path
             ON checkout_paths (path) WHERE observed_until IS NULL;
         CREATE TRIGGER checkout_paths_no_delete
         BEFORE DELETE ON checkout_paths
         BEGIN
             SELECT RAISE(ABORT, 'checkout path history cannot be deleted');
         END;
         CREATE TRIGGER checkout_paths_no_rewrite
         BEFORE UPDATE ON checkout_paths
         WHEN OLD.observed_until IS NOT NULL OR
              NEW.id <> OLD.id OR
              NEW.checkout_id <> OLD.checkout_id OR
              NEW.path <> OLD.path OR
              NEW.observed_from <> OLD.observed_from OR
              NEW.observed_until IS NULL OR
              NEW.observed_until <= OLD.observed_from
         BEGIN
             SELECT RAISE(ABORT, 'checkout path history cannot be rewritten');
         END;
         CREATE TABLE feature_checkouts (
             feature_id TEXT NOT NULL REFERENCES features(id) ON DELETE RESTRICT,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             checkout_id TEXT NOT NULL REFERENCES checkouts(id) ON DELETE RESTRICT,
             assigned_at TEXT NOT NULL,
             PRIMARY KEY (feature_id, repository_id)
         );
         CREATE TABLE work_item_checkout_overrides (
             work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE RESTRICT,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             checkout_id TEXT NOT NULL REFERENCES checkouts(id) ON DELETE RESTRICT,
             assigned_at TEXT NOT NULL,
             PRIMARY KEY (work_item_id, repository_id)
         );
         CREATE TRIGGER feature_checkouts_validate_repository
         BEFORE INSERT ON feature_checkouts
         BEGIN
             SELECT CASE WHEN NOT EXISTS (
                 SELECT 1 FROM checkouts
                 WHERE id = NEW.checkout_id AND repository_id = NEW.repository_id
             ) THEN RAISE(ABORT, 'feature checkout repository mismatch') END;
         END;
         CREATE TRIGGER work_item_overrides_validate_repository
         BEFORE INSERT ON work_item_checkout_overrides
         BEGIN
             SELECT CASE WHEN NOT EXISTS (
                 SELECT 1 FROM checkouts
                 WHERE id = NEW.checkout_id AND repository_id = NEW.repository_id
             ) THEN RAISE(ABORT, 'Work item checkout repository mismatch') END;
         END;
         CREATE VIEW effective_work_item_checkouts AS
             SELECT override.work_item_id, override.repository_id, override.checkout_id, 0 AS inherited
             FROM work_item_checkout_overrides override
             UNION ALL
             SELECT item.id, feature.repository_id, feature.checkout_id, 1 AS inherited
             FROM work_items item
             JOIN feature_checkouts feature ON feature.feature_id = item.feature_id
             WHERE NOT EXISTS (
                 SELECT 1 FROM work_item_checkout_overrides override
                 WHERE override.work_item_id = item.id
                   AND override.repository_id = feature.repository_id
             );
         CREATE TABLE native_sessions (
             id TEXT PRIMARY KEY,
             provider TEXT NOT NULL CHECK (provider IN ('claude', 'codex')),
             native_id TEXT NOT NULL CHECK (native_id <> ''),
             discovered_at TEXT NOT NULL,
             UNIQUE (provider, native_id)
         );
         CREATE TABLE native_session_associations (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             role TEXT NOT NULL,
             associated_from TEXT NOT NULL,
             associated_until TEXT,
             CHECK (
                 (epic_id IS NOT NULL) + (feature_id IS NOT NULL) + (work_item_id IS NOT NULL) = 1
             ),
             CHECK (associated_until IS NULL OR associated_until > associated_from)
         );
         CREATE UNIQUE INDEX native_session_associations_one_current
             ON native_session_associations (session_id) WHERE associated_until IS NULL;
         CREATE TRIGGER native_session_associations_no_delete
         BEFORE DELETE ON native_session_associations
         BEGIN
             SELECT RAISE(ABORT, 'native session associations are append-only');
         END;
         CREATE TRIGGER native_session_associations_no_rewrite
         BEFORE UPDATE ON native_session_associations
         WHEN OLD.associated_until IS NOT NULL OR
              NEW.id <> OLD.id OR
              NEW.session_id <> OLD.session_id OR
              NEW.epic_id IS NOT OLD.epic_id OR
              NEW.feature_id IS NOT OLD.feature_id OR
              NEW.work_item_id IS NOT OLD.work_item_id OR
              NEW.role <> OLD.role OR
              NEW.associated_from <> OLD.associated_from OR
              NEW.associated_until IS NULL OR
              NEW.associated_until <= OLD.associated_from
         BEGIN
             SELECT RAISE(ABORT, 'native session associations are append-only');
         END;
         CREATE TABLE workflow_runs (
             id TEXT PRIMARY KEY,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             current_state TEXT NOT NULL,
             started_at TEXT NOT NULL,
             completed_at TEXT,
             CHECK (
                 (epic_id IS NOT NULL) + (feature_id IS NOT NULL) + (work_item_id IS NOT NULL) = 1
             )
         );
         CREATE TABLE workflow_events (
             id TEXT PRIMARY KEY,
             run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE RESTRICT,
             sequence INTEGER NOT NULL CHECK (sequence > 0),
             from_state TEXT NOT NULL,
             to_state TEXT NOT NULL,
             actor TEXT NOT NULL,
             occurred_at TEXT NOT NULL,
             payload_json TEXT NOT NULL,
             UNIQUE (run_id, sequence)
         );
         CREATE TABLE operation_intents (
             id TEXT PRIMARY KEY,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             kind TEXT NOT NULL,
             status TEXT NOT NULL,
             payload_json TEXT NOT NULL,
             created_at TEXT NOT NULL,
             completed_at TEXT,
             CHECK (
                 (epic_id IS NOT NULL) + (feature_id IS NOT NULL) + (work_item_id IS NOT NULL) = 1
             )
         );
         CREATE TABLE launch_intents (
             id TEXT PRIMARY KEY,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             checkout_id TEXT NOT NULL REFERENCES checkouts(id) ON DELETE RESTRICT,
             provider TEXT NOT NULL CHECK (provider IN ('claude', 'codex')),
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             token_hash TEXT NOT NULL UNIQUE CHECK (token_hash <> ''),
             status TEXT NOT NULL,
             created_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             CHECK (
                 (epic_id IS NOT NULL) + (feature_id IS NOT NULL) + (work_item_id IS NOT NULL) = 1
             ),
             CHECK (expires_at > created_at)
         );
         CREATE TABLE restore_memberships (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             feature_id TEXT NOT NULL REFERENCES features(id) ON DELETE RESTRICT,
             active_from TEXT NOT NULL,
             active_until TEXT,
             CHECK (active_until IS NULL OR active_until > active_from)
         );
         CREATE UNIQUE INDEX restore_memberships_one_current
             ON restore_memberships (session_id) WHERE active_until IS NULL;
         CREATE TABLE terminal_layouts (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
             captured_at TEXT NOT NULL
         );
         CREATE TABLE terminal_tabs (
             id TEXT PRIMARY KEY,
             layout_id TEXT NOT NULL REFERENCES terminal_layouts(id) ON DELETE CASCADE,
             feature_id TEXT NOT NULL REFERENCES features(id) ON DELETE RESTRICT,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             position INTEGER NOT NULL CHECK (position >= 0),
             UNIQUE (layout_id, position)
         );"#,
    )?;
    apply_migration(
        connection,
        4,
        MANAGED_BINDING_SCHEMA_CHECKSUM,
        "ALTER TABLE launch_intents
             ADD COLUMN role TEXT NOT NULL DEFAULT 'work_item_execution';
         ALTER TABLE launch_intents ADD COLUMN expected_native_id TEXT;
         ALTER TABLE launch_intents ADD COLUMN terminal_pid INTEGER;
         ALTER TABLE launch_intents ADD COLUMN failure TEXT;
         CREATE TABLE managed_sessions (
             id TEXT PRIMARY KEY,
             launch_intent_id TEXT UNIQUE REFERENCES launch_intents(id) ON DELETE RESTRICT,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             checkout_id TEXT NOT NULL REFERENCES checkouts(id) ON DELETE RESTRICT,
             role TEXT NOT NULL,
             status TEXT NOT NULL CHECK (status IN ('bound', 'adopted', 'stopped')),
             managed_from TEXT NOT NULL,
             managed_until TEXT,
             CHECK (managed_until IS NULL OR managed_until > managed_from)
         );
         CREATE UNIQUE INDEX managed_sessions_one_current
             ON managed_sessions (session_id) WHERE managed_until IS NULL;
         CREATE TABLE live_observations (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             source TEXT NOT NULL,
             status TEXT NOT NULL,
             observed_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             cwd TEXT,
             pid INTEGER,
             process_created_at TEXT,
             executable TEXT,
             parent_pid INTEGER,
             CHECK (expires_at > observed_at)
         );
         CREATE INDEX live_observations_session_time
             ON live_observations (session_id, observed_at DESC);",
    )?;
    apply_migration(
        connection,
        5,
        NATIVE_SOURCE_SCHEMA_CHECKSUM,
        "CREATE TABLE native_session_sources (
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             path TEXT NOT NULL UNIQUE CHECK (path <> ''),
             adapter_version TEXT NOT NULL CHECK (adapter_version <> ''),
             snapshot_json TEXT NOT NULL CHECK (snapshot_json <> ''),
             missing INTEGER NOT NULL CHECK (missing IN (0, 1)),
             observed_at TEXT NOT NULL,
             PRIMARY KEY (session_id, path)
         );
         CREATE INDEX native_session_sources_session
             ON native_session_sources (session_id, missing, observed_at DESC);",
    )?;
    apply_migration(
        connection,
        6,
        INTEGRATION_STATE_SCHEMA_CHECKSUM,
        "CREATE TABLE integration_registrations (
             provider TEXT PRIMARY KEY CHECK (provider IN ('claude', 'codex')),
             enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
             adapter_version TEXT NOT NULL CHECK (adapter_version <> ''),
             first_observed_at TEXT,
             last_observed_at TEXT
         );
         CREATE TABLE integration_observations (
             provider TEXT PRIMARY KEY CHECK (provider IN ('claude', 'codex')),
             first_observed_at TEXT NOT NULL,
             last_observed_at TEXT NOT NULL,
             last_hook_observed_at TEXT,
             last_app_server_observed_at TEXT
         );
         CREATE TABLE integration_confirmations (
             token_hash TEXT PRIMARY KEY CHECK (token_hash <> ''),
             provider TEXT NOT NULL CHECK (provider IN ('claude', 'codex')),
             operation TEXT NOT NULL,
             configuration_digest TEXT NOT NULL CHECK (configuration_digest <> ''),
             created_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             consumed_at TEXT,
             CHECK (expires_at > created_at)
         );",
    )?;
    apply_migration(
        connection,
        7,
        FEATURE_PLANNING_SCHEMA_CHECKSUM,
        "ALTER TABLE workflow_events ADD COLUMN idempotency_key TEXT;
         CREATE UNIQUE INDEX workflow_events_idempotency
             ON workflow_events (idempotency_key) WHERE idempotency_key IS NOT NULL;
         CREATE TABLE feature_planning_contexts (
             feature_id TEXT PRIMARY KEY REFERENCES features(id) ON DELETE RESTRICT,
             workflow_run_id TEXT NOT NULL UNIQUE REFERENCES workflow_runs(id) ON DELETE RESTRICT,
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             epic_content_hash TEXT NOT NULL CHECK (length(epic_content_hash) = 64),
             repository_head TEXT NOT NULL CHECK (repository_head <> ''),
             created_at TEXT NOT NULL
         );
         CREATE TABLE feature_planning_proposals (
             feature_id TEXT PRIMARY KEY REFERENCES features(id) ON DELETE RESTRICT,
             workflow_run_id TEXT NOT NULL UNIQUE REFERENCES workflow_runs(id) ON DELETE RESTRICT,
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             proposal_json TEXT NOT NULL CHECK (proposal_json <> ''),
             status TEXT NOT NULL CHECK (
                 status IN ('awaiting_approval', 'rejected', 'publishing', 'published')
             ),
             submitted_at TEXT NOT NULL,
             approved_at TEXT,
             published_commit TEXT
         );
         CREATE TABLE work_item_checkpoints (
             id TEXT PRIMARY KEY,
             work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE RESTRICT,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             next_action_kind TEXT NOT NULL,
             summary TEXT NOT NULL CHECK (summary <> ''),
             recorded_at TEXT NOT NULL
         );",
    )?;
    apply_migration(
        connection,
        8,
        WORKFLOW_CREDENTIAL_SCHEMA_CHECKSUM,
        "ALTER TABLE launch_intents ADD COLUMN workflow_token_hash TEXT;
         ALTER TABLE launch_intents ADD COLUMN workflow_token_expires_at TEXT;",
    )?;
    apply_migration(
        connection,
        9,
        SESSION_REQUEST_SCHEMA_CHECKSUM,
        "CREATE TABLE managed_session_requests (
             id TEXT PRIMARY KEY,
             requesting_session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE RESTRICT,
             provider TEXT NOT NULL CHECK (provider IN ('claude', 'codex')),
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             status TEXT NOT NULL CHECK (status IN ('pending', 'launched', 'bound', 'failed')),
             requested_at TEXT NOT NULL,
             launch_intent_id TEXT UNIQUE REFERENCES launch_intents(id) ON DELETE RESTRICT,
             failure TEXT
         );",
    )?;
    Ok(())
}

fn apply_migration(
    connection: &Connection,
    version: i64,
    checksum: &str,
    sql: &str,
) -> Result<(), AppError> {
    let existing = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            [version],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match existing {
        Some(existing_checksum) if existing_checksum != checksum => {
            return Err(AppError::Domain(format!(
                "schema migration {version} checksum mismatch"
            )));
        }
        Some(_) => {}
        None => {
            let transaction =
                Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
            transaction.execute_batch(sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, checksum, applied_at)
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                (version, checksum),
            )?;
            transaction.pragma_update(None, "user_version", version)?;
            transaction.commit()?;
        }
    }
    Ok(())
}

fn health(connection: &Connection) -> Result<StorageHealth, AppError> {
    let integrity = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let foreign_key_violations: i64 =
        connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    let schema_version = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(StorageHealth {
        integrity,
        foreign_key_violations: usize::try_from(foreign_key_violations)
            .map_err(|_| AppError::Domain("foreign-key violation count is invalid".to_owned()))?,
        schema_version,
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::{Transaction, params};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::ConversationId;

    use super::SqliteStore;
    use crate::AppError;

    fn seed_hierarchy(transaction: &Transaction<'_>) -> Result<(), AppError> {
        transaction.execute(
            "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
             VALUES ('workspace', 'concertable', 'Concertable', 'store-repository', '2026-08-27T08:00:00Z')",
            [],
        )?;
        transaction.execute(
            "INSERT INTO repositories (
                 id, workspace_id, slug, title, git_common_directory, default_branch,
                 is_planning_store, created_at
             ) VALUES (
                 'store-repository', 'workspace', 'planning', 'Planning store',
                 'C:/planning/.git', 'main', 1, '2026-08-27T08:00:00Z'
             )",
            [],
        )?;
        transaction.execute(
            "INSERT INTO repositories (
                 id, workspace_id, slug, title, git_common_directory, default_branch,
                 is_planning_store, created_at
             ) VALUES (
                 'code-repository', 'workspace', 'concertable-code', 'Concertable code',
                 'C:/code/.git', 'main', 0, '2026-08-27T08:00:00Z'
             )",
            [],
        )?;
        transaction.execute(
            "INSERT INTO repository_paths (
                 id, repository_id, path, observed_from, observed_until
             ) VALUES (
                 'repository-path', 'code-repository', 'C:/code',
                 '2026-08-27T08:00:00Z', NULL
             )",
            [],
        )?;
        transaction.execute(
            "INSERT INTO epics (id, workspace_id, slug, title, created_at)
             VALUES ('epic', 'workspace', 'launch', 'Launch', '2026-08-27T08:00:00Z')",
            [],
        )?;
        transaction.execute(
            "INSERT INTO features (id, epic_id, slug, title, workflow_state, created_at)
             VALUES (
                 'feature', 'epic', 'availability', 'Availability', 'draft',
                 '2026-08-27T08:00:00Z'
             )",
            [],
        )?;
        transaction.execute(
            "INSERT INTO work_items (id, feature_id, key, slug, title, status, created_at)
             VALUES (
                 'work-item', 'feature', 'launch/availability/api', 'api',
                 'Availability API', 'ready', '2026-08-27T08:00:00Z'
             )",
            [],
        )?;
        transaction.execute(
            "INSERT INTO work_item_repositories (work_item_id, repository_id)
             VALUES ('work-item', 'code-repository')",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn migration_is_idempotent_and_write_failures_roll_back() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        store
            .write(|transaction| {
                transaction.execute("CREATE TABLE proof (value TEXT NOT NULL)", [])?;
                transaction.execute("INSERT INTO proof VALUES ('kept')", [])?;
                Ok(())
            })
            .expect("commit proof");
        let failed = store.write::<()>(|transaction| {
            transaction.execute("INSERT INTO proof VALUES ('rolled-back')", [])?;
            Err(AppError::Domain("injected failure".to_owned()))
        });
        assert!(failed.is_err());
        let count: i64 = store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM proof", [], |row| row.get(0))
                    .map_err(Into::into)
            })
            .expect("proof count");
        assert_eq!(count, 1);
        drop(store);
        assert!(
            SqliteStore::open(path)
                .expect("reopen store")
                .health()
                .expect("health")
                .is_healthy()
        );
    }

    #[test]
    fn backup_is_verified_and_repair_preserves_health() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(path).expect("open store");
        let backup = directory.path().join("backups").join("workboard.sqlite");

        assert!(store.backup(&backup).expect("backup").is_healthy());
        assert!(backup.is_file());
        assert!(store.repair().expect("repair").is_healthy());
        assert!(store.backup(&backup).is_err());
    }

    #[test]
    fn launch_leases_are_durable_and_duplicate_protected() {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        let conversation_id = ConversationId::generate();
        let acquired_at =
            OffsetDateTime::from_unix_timestamp(1_777_000_000).expect("acquired timestamp");
        let expires_at = acquired_at + time::Duration::minutes(2);
        let launch_json = serde_json::json!({ "command": "resume" }).to_string();
        let first = store
            .acquire_launch_lease(
                conversation_id,
                directory.path(),
                &launch_json,
                acquired_at,
                expires_at,
            )
            .expect("first lease");

        assert!(matches!(
            store.acquire_launch_lease(
                conversation_id,
                directory.path(),
                &launch_json,
                acquired_at,
                expires_at,
            ),
            Err(AppError::DuplicateConfirmed)
        ));
        store
            .fail_launch_lease(first.id, "launcher unavailable")
            .expect("fail first lease");
        let second = store
            .acquire_launch_lease(
                conversation_id,
                directory.path(),
                &launch_json,
                acquired_at,
                expires_at,
            )
            .expect("second lease");
        store
            .complete_launch_lease(second.id, 42)
            .expect("complete second lease");
        assert!(matches!(
            store.complete_launch_lease(second.id, 42),
            Err(AppError::LaunchLeaseLost)
        ));
    }

    #[test]
    fn domain_schema_round_trips_hierarchy_and_rejects_invalid_parentage() {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        let hierarchy = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT workspaces.slug, epics.slug, features.slug, work_items.key
                         FROM work_items
                         JOIN features ON features.id = work_items.feature_id
                         JOIN epics ON epics.id = features.epic_id
                         JOIN workspaces ON workspaces.id = epics.workspace_id",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                            ))
                        },
                    )
                    .map_err(Into::into)
            })
            .expect("read hierarchy");
        assert_eq!(
            hierarchy,
            (
                "concertable".to_owned(),
                "launch".to_owned(),
                "availability".to_owned(),
                "launch/availability/api".to_owned(),
            )
        );

        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO features (
                             id, epic_id, slug, title, workflow_state, created_at
                         ) VALUES (
                             'orphan', 'work-item', 'orphan', 'Orphan', 'draft',
                             '2026-08-27T08:00:00Z'
                         )",
                        [],
                    )?;
                    Ok(())
                })
                .is_err()
        );
        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO work_items (
                             id, feature_id, key, slug, title, status, created_at
                         ) VALUES (
                             'duplicate', 'feature', 'launch/availability/api', 'duplicate',
                             'Duplicate', 'ready', '2026-08-27T08:00:00Z'
                         )",
                        [],
                    )?;
                    Ok(())
                })
                .is_err()
        );

        let hash = "a".repeat(64);
        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO documents (
                             id, repository_id, epic_id, kind, relative_path,
                             content_hash, observed_at
                         ) VALUES (
                             'escaped', 'store-repository', 'epic', 'epic', '../EPIC.md',
                             ?1, '2026-08-27T08:00:00Z'
                         )",
                        [&hash],
                    )?;
                    Ok(())
                })
                .is_err()
        );
    }

    #[test]
    fn checkout_and_association_history_survive_replacement_and_reassignment() {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE repository_paths SET observed_until = '2026-08-27T09:00:00Z'
                     WHERE id = 'repository-path'",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO repository_paths (
                         id, repository_id, path, observed_from, observed_until
                     ) VALUES (
                         'moved-path', 'code-repository', 'D:/code',
                         '2026-08-27T09:00:00Z', NULL
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, availability, created_at
                     ) VALUES (
                         'checkout-old', 'code-repository', 'old', 'feature/availability',
                         'deleted', '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (
                         'checkout-path-old', 'checkout-old', 'C:/worktrees/old',
                         '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO checkouts (
                         id, repository_id, git_worktree_identity, branch, availability,
                         replaces_checkout_id, created_at
                     ) VALUES (
                         'checkout-new', 'code-repository', 'new', 'feature/availability',
                         'available', 'checkout-old', '2026-08-27T09:00:00Z'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO checkout_paths (
                         id, checkout_id, path, observed_from, observed_until
                     ) VALUES (
                         'checkout-path-new', 'checkout-new', 'D:/worktrees/new',
                         '2026-08-27T09:00:00Z', NULL
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO feature_checkouts (
                         feature_id, repository_id, checkout_id, assigned_at
                     ) VALUES (
                         'feature', 'code-repository', 'checkout-old', '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                Ok(())
            })
            .expect("record checkout history");

        let inherited: (String, i64) = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT checkout_id, inherited FROM effective_work_item_checkouts
                         WHERE work_item_id = 'work-item' AND repository_id = 'code-repository'",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("inherited checkout");
        assert_eq!(inherited, ("checkout-old".to_owned(), 1));

        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO work_item_checkout_overrides (
                         work_item_id, repository_id, checkout_id, assigned_at
                     ) VALUES (
                         'work-item', 'code-repository', 'checkout-new', '2026-08-27T09:00:00Z'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO native_sessions (id, provider, native_id, discovered_at)
                     VALUES ('session', 'codex', 'thread-1', '2026-08-27T08:00:00Z')",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_associations (
                         id, session_id, feature_id, role, associated_from
                     ) VALUES (
                         'association-old', 'session', 'feature', 'feature_planning',
                         '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                Ok(())
            })
            .expect("record override and association");
        store
            .write(|transaction| {
                transaction.execute(
                    "UPDATE native_session_associations
                     SET associated_until = '2026-08-27T09:00:00Z'
                     WHERE id = 'association-old'",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO native_session_associations (
                         id, session_id, work_item_id, role, associated_from
                     ) VALUES (
                         'association-new', 'session', 'work-item', 'work_item_execution',
                         '2026-08-27T09:00:00Z'
                     )",
                    [],
                )?;
                Ok(())
            })
            .expect("reassign session");

        let current: (String, i64) = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT checkout_id, inherited FROM effective_work_item_checkouts
                         WHERE work_item_id = 'work-item' AND repository_id = 'code-repository'",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("overridden checkout");
        assert_eq!(current, ("checkout-new".to_owned(), 0));
        let counts: (i64, i64, i64) = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT
                             (SELECT COUNT(*) FROM repository_paths),
                             (SELECT COUNT(*) FROM checkout_paths),
                             (SELECT COUNT(*) FROM native_session_associations)",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(Into::into)
            })
            .expect("history counts");
        assert_eq!(counts, (2, 2, 2));
        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "DELETE FROM native_session_associations WHERE id = 'association-old'",
                        [],
                    )?;
                    Ok(())
                })
                .is_err()
        );
        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO checkouts (
                             id, repository_id, git_worktree_identity, availability,
                             replaces_checkout_id, created_at
                         ) VALUES (
                             'wrong-replacement', 'store-repository', 'wrong', 'available',
                             'checkout-old', '2026-08-27T10:00:00Z'
                         )",
                        [],
                    )?;
                    Ok(())
                })
                .is_err()
        );
    }

    #[test]
    fn operation_intents_are_idempotent() {
        let directory = TempDir::new().expect("temporary directory");
        let mut store =
            SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO operation_intents (
                         id, work_item_id, idempotency_key, kind, status, payload_json, created_at
                     ) VALUES (
                         'intent-one', 'work-item', 'create-checkout', 'create_worktree',
                         'pending', '{}', '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                Ok(())
            })
            .expect("first intent");
        assert!(
            store
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO operation_intents (
                             id, work_item_id, idempotency_key, kind, status, payload_json, created_at
                         ) VALUES (
                             'intent-two', 'work-item', 'create-checkout', 'create_worktree',
                             'pending', '{}', '2026-08-27T08:00:00Z'
                         )",
                        [],
                    )?;
                    Ok(())
                })
                .is_err()
        );

        let intent_id: String = store
            .read(|connection| {
                connection
                    .query_row(
                        "SELECT id FROM operation_intents WHERE idempotency_key = ?1",
                        params!["create-checkout"],
                        |row| row.get(0),
                    )
                    .map_err(Into::into)
            })
            .expect("stored intent");
        assert_eq!(intent_id, "intent-one");
    }
}
