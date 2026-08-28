use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{
    Connection, ErrorCode, MAIN_DB, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use workboard_core::{ConversationId, LaunchLeaseId};

use crate::AppError;

const CURRENT_SCHEMA_VERSION: i64 = 24;
const FOUNDATION_SCHEMA_CHECKSUM: &str = "agent-workboard-foundation-v1";
const LAUNCH_LEASE_SCHEMA_CHECKSUM: &str = "agent-workboard-launch-leases-v1";
const WORKBOARD_DOMAIN_SCHEMA_CHECKSUM: &str = "agent-workboard-domain-v1";
const MANAGED_BINDING_SCHEMA_CHECKSUM: &str = "agent-workboard-managed-binding-v1";
const NATIVE_SOURCE_SCHEMA_CHECKSUM: &str = "agent-workboard-native-source-v1";
const INTEGRATION_STATE_SCHEMA_CHECKSUM: &str = "agent-workboard-integration-state-v1";
const FEATURE_PLANNING_SCHEMA_CHECKSUM: &str = "agent-workboard-feature-planning-v1";
const WORKFLOW_CREDENTIAL_SCHEMA_CHECKSUM: &str = "agent-workboard-workflow-credential-v1";
const SESSION_REQUEST_SCHEMA_CHECKSUM: &str = "agent-workboard-session-request-v1";
const MANAGED_RECOVERY_SCHEMA_CHECKSUM: &str = "agent-workboard-managed-recovery-v1";
const LEGACY_IMPORT_SCHEMA_CHECKSUM: &str = "agent-workboard-legacy-import-v1";
const LEGACY_IMPORT_RECORD_SCHEMA_CHECKSUM: &str = "agent-workboard-legacy-import-record-v1";
const LEGACY_CANDIDATE_METADATA_SCHEMA_CHECKSUM: &str =
    "agent-workboard-legacy-candidate-metadata-v1";
const IMPORT_BATCH_REPOSITORY_SCHEMA_CHECKSUM: &str = "agent-workboard-import-batch-repository-v1";
const IMPORT_BATCH_REPOSITORY_REPAIR_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-repository-repair-v1";
const IMPORT_BATCH_REPOSITORY_PARENT_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-repository-parent-v1";
const IMPORT_BATCH_ATTESTATION_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-attestation-v1";
const IMPORT_BATCH_AUDIT_SCHEMA_CHECKSUM: &str = "agent-workboard-import-batch-audit-v1";
const IMPORT_BATCH_ATTESTATION_VALIDATION_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-attestation-validation-v1";
const IMPORT_BATCH_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-audit-checkpoint-v1";
const IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-pre-audit-attestation-repair-v1";
const IMPORT_BATCH_FINAL_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-batch-final-audit-checkpoint-v1";
const IMPORT_DOCUMENT_MEMBERSHIP_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-document-membership-v1";
const IMPORT_DOCUMENT_MEMBERSHIP_SQL: &str = "CREATE TABLE import_document_memberships (
     import_id TEXT NOT NULL REFERENCES import_batches(id) ON DELETE RESTRICT,
     document_id TEXT NOT NULL UNIQUE REFERENCES documents(id) ON DELETE RESTRICT,
     destination_kind TEXT NOT NULL CHECK (
         destination_kind IN ('epic', 'feature', 'work_item')
     ),
     PRIMARY KEY (import_id, document_id)
 );
 CREATE TRIGGER import_document_memberships_valid
 BEFORE INSERT ON import_document_memberships
 WHEN NOT EXISTS (
     SELECT 1
       FROM import_batches batch
       JOIN workspaces workspace ON workspace.id = batch.workspace_id
       JOIN documents document
         ON document.id = NEW.document_id
        AND document.repository_id = workspace.planning_store_repository_id
        AND document.kind = NEW.destination_kind
      WHERE batch.id = NEW.import_id
        AND batch.kind = 'concertable_plans'
 )
 BEGIN
     SELECT RAISE(ABORT, 'import document membership is invalid');
 END;
 INSERT INTO import_document_memberships (import_id, document_id, destination_kind)
 SELECT DISTINCT batch.id, document.id, document.kind
   FROM import_batches batch
   JOIN workspaces workspace ON workspace.id = batch.workspace_id
   JOIN documents document
     ON document.repository_id = workspace.planning_store_repository_id
   JOIN document_revisions revision
     ON revision.document_id = document.id
    AND revision.observed_commit = batch.planning_commit
    AND revision.observed_at = batch.imported_at
  WHERE batch.kind = 'concertable_plans';
 CREATE TRIGGER import_document_memberships_no_update
 BEFORE UPDATE ON import_document_memberships
 BEGIN
     SELECT RAISE(ABORT, 'import document membership is immutable');
 END;
 CREATE TRIGGER import_document_memberships_no_delete
 BEFORE DELETE ON import_document_memberships
 BEGIN
     SELECT RAISE(ABORT, 'import document membership cannot be deleted');
 END;";
const IMPORT_DOCUMENT_MEMBERSHIP_FINALIZATION_SCHEMA_CHECKSUM: &str =
    "agent-workboard-import-document-membership-finalization-v1";
const IMPORT_DOCUMENT_MEMBERSHIP_FINALIZATION_SQL: &str =
    "DROP TRIGGER import_document_memberships_no_update;
 DROP TRIGGER import_document_memberships_no_delete;
 DELETE FROM import_document_memberships;
 INSERT INTO import_document_memberships (import_id, document_id, destination_kind)
 SELECT batch.id, document.id, 'epic'
   FROM import_batches batch
   JOIN workspaces workspace ON workspace.id = batch.workspace_id
   JOIN epics epic
     ON epic.workspace_id = batch.workspace_id
    AND epic.created_at = batch.imported_at
   JOIN documents document
     ON document.epic_id = epic.id
    AND document.repository_id = workspace.planning_store_repository_id
    AND document.kind = 'epic'
   JOIN document_revisions revision
     ON revision.document_id = document.id
    AND revision.observed_commit = batch.planning_commit
    AND revision.observed_at = batch.imported_at
  WHERE batch.kind = 'concertable_plans'
 UNION ALL
 SELECT batch.id, document.id, 'feature'
   FROM import_batches batch
   JOIN workspaces workspace ON workspace.id = batch.workspace_id
   JOIN epics epic
     ON epic.workspace_id = batch.workspace_id
    AND epic.created_at = batch.imported_at
   JOIN features feature
     ON feature.epic_id = epic.id
    AND feature.created_at = batch.imported_at
   JOIN documents document
     ON document.feature_id = feature.id
    AND document.repository_id = workspace.planning_store_repository_id
    AND document.kind = 'feature'
   JOIN document_revisions revision
     ON revision.document_id = document.id
    AND revision.observed_commit = batch.planning_commit
    AND revision.observed_at = batch.imported_at
  WHERE batch.kind = 'concertable_plans'
 UNION ALL
 SELECT batch.id, document.id, 'work_item'
   FROM import_batches batch
   JOIN workspaces workspace ON workspace.id = batch.workspace_id
   JOIN epics epic
     ON epic.workspace_id = batch.workspace_id
    AND epic.created_at = batch.imported_at
   JOIN features feature
     ON feature.epic_id = epic.id
    AND feature.created_at = batch.imported_at
   JOIN work_items work_item
     ON work_item.feature_id = feature.id
    AND work_item.created_at = batch.imported_at
   JOIN documents document
     ON document.work_item_id = work_item.id
    AND document.repository_id = workspace.planning_store_repository_id
    AND document.kind = 'work_item'
   JOIN document_revisions revision
     ON revision.document_id = document.id
    AND revision.observed_commit = batch.planning_commit
    AND revision.observed_at = batch.imported_at
  WHERE batch.kind = 'concertable_plans';
 CREATE TRIGGER import_document_memberships_no_update
 BEFORE UPDATE ON import_document_memberships
 BEGIN
     SELECT RAISE(ABORT, 'import document membership is immutable');
 END;
 CREATE TRIGGER import_document_memberships_no_delete
 BEFORE DELETE ON import_document_memberships
 BEGIN
     SELECT RAISE(ABORT, 'import document membership cannot be deleted');
 END;
 CREATE TABLE import_document_membership_finalizations (
     import_id TEXT PRIMARY KEY REFERENCES import_batches(id) ON DELETE RESTRICT,
     finalized_at TEXT NOT NULL
 );
 INSERT INTO import_document_membership_finalizations (import_id, finalized_at)
 SELECT batch.id, batch.imported_at
   FROM import_batches batch
  WHERE batch.kind = 'concertable_plans'
    AND EXISTS (
        SELECT 1 FROM import_document_memberships membership
         WHERE membership.import_id = batch.id
    );
 CREATE TRIGGER import_document_membership_finalizations_valid
 BEFORE INSERT ON import_document_membership_finalizations
 WHEN NOT EXISTS (
     SELECT 1 FROM import_batches batch
      WHERE batch.id = NEW.import_id
        AND batch.kind = 'concertable_plans'
        AND EXISTS (
            SELECT 1 FROM import_document_memberships membership
             WHERE membership.import_id = batch.id
        )
 )
 BEGIN
     SELECT RAISE(ABORT, 'import document membership finalization is invalid');
 END;
 CREATE TRIGGER import_document_membership_finalizations_no_update
 BEFORE UPDATE ON import_document_membership_finalizations
 BEGIN
     SELECT RAISE(ABORT, 'import document membership finalization is immutable');
 END;
 CREATE TRIGGER import_document_membership_finalizations_no_delete
 BEFORE DELETE ON import_document_membership_finalizations
 BEGIN
     SELECT RAISE(ABORT, 'import document membership finalization cannot be deleted');
 END;
 CREATE TRIGGER import_document_memberships_finalized
 BEFORE INSERT ON import_document_memberships
 WHEN EXISTS (
     SELECT 1 FROM import_document_membership_finalizations finalization
      WHERE finalization.import_id = NEW.import_id
 )
 BEGIN
     SELECT RAISE(ABORT, 'import document membership is finalized');
 END;
 CREATE TRIGGER import_document_member_fields_immutable
 BEFORE UPDATE OF repository_id, kind ON documents
 WHEN (NEW.repository_id IS NOT OLD.repository_id OR NEW.kind IS NOT OLD.kind)
      AND EXISTS (
          SELECT 1 FROM import_document_memberships membership
           WHERE membership.document_id = OLD.id
      )
 BEGIN
     SELECT RAISE(ABORT, 'import document membership fields are immutable');
 END;
 CREATE TRIGGER import_document_batches_finalized
 BEFORE UPDATE ON import_batches
 WHEN EXISTS (
     SELECT 1 FROM import_document_membership_finalizations finalization
      WHERE finalization.import_id = OLD.id
 )
 BEGIN
     SELECT RAISE(ABORT, 'import document batch is finalized');
 END;
 CREATE TRIGGER import_source_destinations_finalized_insert
 BEFORE INSERT ON import_source_destinations
 WHEN EXISTS (
     SELECT 1 FROM import_document_membership_finalizations finalization
      WHERE finalization.import_id = NEW.import_id
 )
 BEGIN
     SELECT RAISE(ABORT, 'import source destinations are finalized');
 END;
 CREATE TRIGGER import_source_destinations_finalized_update
 BEFORE UPDATE ON import_source_destinations
 WHEN EXISTS (
     SELECT 1 FROM import_document_membership_finalizations finalization
      WHERE finalization.import_id IN (OLD.import_id, NEW.import_id)
 )
 BEGIN
     SELECT RAISE(ABORT, 'import source destinations are finalized');
 END;
 CREATE TRIGGER import_source_destinations_finalized_delete
 BEFORE DELETE ON import_source_destinations
 WHEN EXISTS (
     SELECT 1 FROM import_document_membership_finalizations finalization
      WHERE finalization.import_id = OLD.import_id
 )
 BEGIN
     SELECT RAISE(ABORT, 'import source destinations are finalized');
 END;";
const IMPORT_BATCH_REPOSITORY_REPAIR_SQL: &str = "UPDATE import_batches
    SET repository_id = (
        SELECT MIN(record.destination_id)
          FROM legacy_import_records record
          JOIN repositories repository ON repository.id = record.destination_id
         WHERE record.import_id = import_batches.id
           AND record.destination_kind = 'repository'
           AND repository.workspace_id = import_batches.workspace_id
           AND repository.is_planning_store = 0
        HAVING COUNT(DISTINCT record.destination_id) = 1
    )
  WHERE kind = 'context_catalogue';
 UPDATE import_batches
    SET repository_id = (
        SELECT MIN(path.repository_id)
          FROM repository_paths path
          JOIN repositories repository ON repository.id = path.repository_id
         WHERE path.path = import_batches.source_path
           AND repository.workspace_id = import_batches.workspace_id
           AND repository.is_planning_store = 0
        HAVING COUNT(DISTINCT path.repository_id) = 1
    )
  WHERE kind = 'concertable_plans';
 CREATE TRIGGER import_batches_repository_required
 BEFORE INSERT ON import_batches
 WHEN NEW.repository_id IS NULL OR NOT EXISTS (
     SELECT 1 FROM repositories repository
      WHERE repository.id = NEW.repository_id
        AND repository.workspace_id = NEW.workspace_id
        AND repository.is_planning_store = 0
 )
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository is invalid');
 END;
 CREATE TRIGGER import_batches_repository_immutable
 BEFORE UPDATE OF repository_id, workspace_id ON import_batches
 WHEN NEW.repository_id IS NOT OLD.repository_id
      OR NEW.workspace_id IS NOT OLD.workspace_id
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository is immutable');
 END;";
const IMPORT_BATCH_REPOSITORY_PARENT_SQL: &str =
    "CREATE TRIGGER import_batch_repository_parent_immutable
 BEFORE UPDATE OF workspace_id, is_planning_store ON repositories
 WHEN (NEW.workspace_id IS NOT OLD.workspace_id
       OR NEW.is_planning_store IS NOT OLD.is_planning_store)
      AND EXISTS (
          SELECT 1 FROM import_batches batch
           WHERE batch.repository_id = OLD.id
      )
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository parent is immutable');
 END;";
const IMPORT_BATCH_ATTESTATION_SQL: &str = "CREATE TABLE import_batch_repository_attestations (
     import_id TEXT PRIMARY KEY REFERENCES import_batches(id) ON DELETE RESTRICT,
     repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
     authority TEXT NOT NULL CHECK (
         authority IN ('captured_direct', 'immutable_evidence', 'explicit_repair')
     ),
     confirmed_at TEXT NOT NULL
 );
 CREATE TRIGGER import_batch_repository_attestations_no_update
 BEFORE UPDATE ON import_batch_repository_attestations
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository attestation is immutable');
 END;
 CREATE TRIGGER import_batch_repository_attestations_no_delete
 BEFORE DELETE ON import_batch_repository_attestations
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository attestation cannot be deleted');
 END;";
const IMPORT_BATCH_ATTESTATION_VALIDATION_SQL: &str =
    "DROP TRIGGER IF EXISTS import_batch_repository_attestations_valid;
 DROP TRIGGER import_batch_repository_attestations_no_update;
 DROP TRIGGER import_batch_repository_attestations_no_delete;
 DELETE FROM import_batch_repository_attestations
  WHERE NOT EXISTS (
      SELECT 1
        FROM import_batches batch
        JOIN repositories repository
          ON repository.id = import_batch_repository_attestations.repository_id
         AND repository.workspace_id = batch.workspace_id
         AND repository.is_planning_store = 0
       WHERE batch.id = import_batch_repository_attestations.import_id
  );
 CREATE TRIGGER import_batch_repository_attestations_valid
 BEFORE INSERT ON import_batch_repository_attestations
 WHEN NOT EXISTS (
     SELECT 1
       FROM import_batches batch
       JOIN repositories repository
         ON repository.id = NEW.repository_id
        AND repository.workspace_id = batch.workspace_id
        AND repository.is_planning_store = 0
      WHERE batch.id = NEW.import_id
 )
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository attestation is invalid');
 END;
 CREATE TRIGGER import_batch_repository_attestations_no_update
 BEFORE UPDATE ON import_batch_repository_attestations
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository attestation is immutable');
 END;
 CREATE TRIGGER import_batch_repository_attestations_no_delete
 BEFORE DELETE ON import_batch_repository_attestations
 BEGIN
     SELECT RAISE(ABORT, 'import batch repository attestation cannot be deleted');
 END;";
const IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SQL: &str =
    "DROP TRIGGER IF EXISTS import_batch_repository_attestations_valid;
 DROP TRIGGER import_batch_repository_attestations_no_update;
 DROP TRIGGER import_batch_repository_attestations_no_delete;";

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
    apply_migration(
        connection,
        10,
        MANAGED_RECOVERY_SCHEMA_CHECKSUM,
        "ALTER TABLE launch_intents ADD COLUMN terminal_window TEXT;
         CREATE TABLE restore_entries (
             session_id TEXT PRIMARY KEY REFERENCES native_sessions(id) ON DELETE RESTRICT,
             epic_id TEXT REFERENCES epics(id) ON DELETE RESTRICT,
             feature_id TEXT REFERENCES features(id) ON DELETE RESTRICT,
             work_item_id TEXT REFERENCES work_items(id) ON DELETE RESTRICT,
             added_at TEXT NOT NULL,
             removed_at TEXT,
             remove_reason TEXT,
             CHECK (
                 (epic_id IS NOT NULL) + (feature_id IS NOT NULL) + (work_item_id IS NOT NULL) = 1
             ),
             CHECK (removed_at IS NULL OR removed_at >= added_at)
         );
         INSERT INTO restore_entries (
             session_id, epic_id, feature_id, work_item_id, added_at
         )
         SELECT managed.session_id, association.epic_id, association.feature_id,
                association.work_item_id, MIN(managed.managed_from)
         FROM managed_sessions managed
         JOIN native_session_associations association
           ON association.session_id = managed.session_id
          AND association.associated_until IS NULL
         GROUP BY managed.session_id;
         CREATE TABLE recovery_attempts (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
             idempotency_key TEXT NOT NULL UNIQUE CHECK (idempotency_key <> ''),
             requested_at TEXT NOT NULL,
             plan_json TEXT NOT NULL CHECK (plan_json <> ''),
             status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'partial')),
             completed_at TEXT
         );
         CREATE TABLE recovery_entry_outcomes (
             attempt_id TEXT NOT NULL REFERENCES recovery_attempts(id) ON DELETE RESTRICT,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             status TEXT NOT NULL CHECK (status IN ('skipped', 'launched', 'bound', 'conflict', 'failed')),
             launch_intent_id TEXT REFERENCES launch_intents(id) ON DELETE RESTRICT,
             code TEXT,
             message TEXT,
             observed_at TEXT NOT NULL,
             PRIMARY KEY (attempt_id, session_id)
         );",
    )?;
    apply_migration(
        connection,
        11,
        LEGACY_IMPORT_SCHEMA_CHECKSUM,
        "CREATE TABLE import_batches (
             id TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
             kind TEXT NOT NULL CHECK (kind IN ('concertable_plans', 'context_catalogue')),
             source_path TEXT NOT NULL CHECK (source_path <> ''),
             source_head TEXT,
             preview_hash TEXT NOT NULL UNIQUE CHECK (length(preview_hash) = 64),
             planning_commit TEXT,
             imported_at TEXT NOT NULL
         );
         CREATE TABLE import_source_destinations (
             import_id TEXT NOT NULL REFERENCES import_batches(id) ON DELETE RESTRICT,
             source_path TEXT NOT NULL CHECK (source_path <> ''),
             source_hash TEXT NOT NULL CHECK (length(source_hash) = 64),
             destination_kind TEXT NOT NULL CHECK (
                 destination_kind IN ('epic', 'feature', 'work_item', 'session_candidate')
             ),
             destination_id TEXT NOT NULL CHECK (destination_id <> ''),
             document_id TEXT,
             PRIMARY KEY (import_id, source_path, destination_kind, destination_id)
         );
         CREATE TABLE imported_session_candidates (
             workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
             session_id TEXT NOT NULL REFERENCES native_sessions(id) ON DELETE RESTRICT,
             repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE RESTRICT,
             checkout_id TEXT REFERENCES checkouts(id) ON DELETE RESTRICT,
             legacy_workstream_id TEXT,
             legacy_workstream_title TEXT,
             authority TEXT,
             confidence TEXT,
             status TEXT NOT NULL CHECK (status IN ('unassigned', 'confirmed', 'ignored')),
             imported_at TEXT NOT NULL,
             PRIMARY KEY (workspace_id, session_id)
         );",
    )?;
    apply_migration(
        connection,
        12,
        LEGACY_IMPORT_RECORD_SCHEMA_CHECKSUM,
        "CREATE TABLE legacy_import_records (
             import_id TEXT NOT NULL REFERENCES import_batches(id) ON DELETE RESTRICT,
             source_table TEXT NOT NULL CHECK (source_table <> ''),
             source_key TEXT NOT NULL CHECK (source_key <> ''),
             destination_kind TEXT NOT NULL CHECK (destination_kind <> ''),
             destination_id TEXT NOT NULL CHECK (destination_id <> ''),
             payload_json TEXT NOT NULL CHECK (payload_json <> ''),
             PRIMARY KEY (import_id, source_table, source_key)
         );",
    )?;
    apply_migration(
        connection,
        13,
        LEGACY_CANDIDATE_METADATA_SCHEMA_CHECKSUM,
        "ALTER TABLE imported_session_candidates ADD COLUMN native_title TEXT;
         ALTER TABLE imported_session_candidates ADD COLUMN first_prompt_preview TEXT;
         ALTER TABLE imported_session_candidates ADD COLUMN last_prompt_preview TEXT;
         ALTER TABLE imported_session_candidates ADD COLUMN last_activity_at TEXT;
         ALTER TABLE imported_session_candidates ADD COLUMN observed_cwd TEXT;",
    )?;
    apply_migration(
        connection,
        14,
        IMPORT_BATCH_REPOSITORY_SCHEMA_CHECKSUM,
        "ALTER TABLE import_batches ADD COLUMN repository_id TEXT
             REFERENCES repositories(id) ON DELETE RESTRICT;
         UPDATE import_batches
            SET repository_id = (
                SELECT record.destination_id
                  FROM legacy_import_records record
                 WHERE record.import_id = import_batches.id
                   AND record.destination_kind = 'repository'
                 ORDER BY record.source_table, record.source_key
                 LIMIT 1
            )
          WHERE kind = 'context_catalogue';
         UPDATE import_batches
            SET repository_id = COALESCE(
                (
                    SELECT path.repository_id
                      FROM repository_paths path
                     WHERE path.path = import_batches.source_path
                     ORDER BY path.observed_from, path.id
                     LIMIT 1
                ),
                (
                    SELECT relation.repository_id
                      FROM import_source_destinations destination
                      JOIN work_item_repositories relation
                        ON destination.destination_kind = 'work_item'
                       AND destination.destination_id = relation.work_item_id
                     WHERE destination.import_id = import_batches.id
                     ORDER BY relation.repository_id
                     LIMIT 1
                )
            )
          WHERE kind = 'concertable_plans';
         CREATE INDEX import_batches_target
             ON import_batches (workspace_id, repository_id, kind, preview_hash);",
    )?;
    let direct_ownership = capture_schema_14_direct_ownership(connection)?;
    if migration_exists(connection, 15)? {
        apply_import_repository_migrations(connection, &direct_ownership)?;
    } else {
        apply_import_repository_migrations_atomically(connection, &direct_ownership)?;
    }
    apply_migration(
        connection,
        23,
        IMPORT_DOCUMENT_MEMBERSHIP_SCHEMA_CHECKSUM,
        IMPORT_DOCUMENT_MEMBERSHIP_SQL,
    )?;
    apply_validated_migration(
        connection,
        24,
        IMPORT_DOCUMENT_MEMBERSHIP_FINALIZATION_SCHEMA_CHECKSUM,
        IMPORT_DOCUMENT_MEMBERSHIP_FINALIZATION_SQL,
        validate_import_document_memberships,
    )?;
    Ok(())
}

fn apply_import_repository_migrations(
    connection: &Connection,
    direct_ownership: &HashMap<String, String>,
) -> Result<(), AppError> {
    apply_validated_migration(
        connection,
        15,
        IMPORT_BATCH_REPOSITORY_REPAIR_SCHEMA_CHECKSUM,
        IMPORT_BATCH_REPOSITORY_REPAIR_SQL,
        |transaction| {
            restore_schema_14_direct_ownership(transaction, direct_ownership)?;
            validate_import_batch_repository_ownership(transaction, 15)
        },
    )?;
    apply_migration(
        connection,
        16,
        IMPORT_BATCH_REPOSITORY_PARENT_SCHEMA_CHECKSUM,
        IMPORT_BATCH_REPOSITORY_PARENT_SQL,
    )?;
    apply_migration(
        connection,
        17,
        IMPORT_BATCH_ATTESTATION_SCHEMA_CHECKSUM,
        IMPORT_BATCH_ATTESTATION_SQL,
    )?;
    if !migration_exists(connection, 18)? {
        apply_migration(
            connection,
            19,
            IMPORT_BATCH_ATTESTATION_VALIDATION_SCHEMA_CHECKSUM,
            IMPORT_BATCH_ATTESTATION_VALIDATION_SQL,
        )?;
        apply_validated_migration(
            connection,
            21,
            IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SCHEMA_CHECKSUM,
            IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SQL,
            repair_pre_audit_attestations,
        )?;
        prepare_import_batch_repository_audit(connection)?;
    }
    apply_validated_migration(
        connection,
        18,
        IMPORT_BATCH_AUDIT_SCHEMA_CHECKSUM,
        "",
        |transaction| audit_import_batch_repository_ownership(transaction, direct_ownership),
    )?;
    apply_migration(
        connection,
        19,
        IMPORT_BATCH_ATTESTATION_VALIDATION_SCHEMA_CHECKSUM,
        IMPORT_BATCH_ATTESTATION_VALIDATION_SQL,
    )?;
    apply_migration(
        connection,
        20,
        IMPORT_BATCH_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM,
        "",
    )?;
    apply_validated_migration(
        connection,
        21,
        IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SCHEMA_CHECKSUM,
        IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SQL,
        repair_pre_audit_attestations,
    )?;
    apply_migration(
        connection,
        22,
        IMPORT_BATCH_FINAL_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM,
        "",
    )
}

fn apply_import_repository_migrations_atomically(
    connection: &Connection,
    direct_ownership: &HashMap<String, String>,
) -> Result<(), AppError> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    apply_migration_step(
        &transaction,
        15,
        IMPORT_BATCH_REPOSITORY_REPAIR_SCHEMA_CHECKSUM,
        IMPORT_BATCH_REPOSITORY_REPAIR_SQL,
        |transaction| {
            restore_schema_14_direct_ownership(transaction, direct_ownership)?;
            validate_import_batch_repository_ownership(transaction, 15)
        },
    )?;
    apply_migration_step(
        &transaction,
        16,
        IMPORT_BATCH_REPOSITORY_PARENT_SCHEMA_CHECKSUM,
        IMPORT_BATCH_REPOSITORY_PARENT_SQL,
        |_| Ok(()),
    )?;
    apply_migration_step(
        &transaction,
        17,
        IMPORT_BATCH_ATTESTATION_SCHEMA_CHECKSUM,
        IMPORT_BATCH_ATTESTATION_SQL,
        |_| Ok(()),
    )?;
    apply_migration_step(
        &transaction,
        18,
        IMPORT_BATCH_AUDIT_SCHEMA_CHECKSUM,
        "",
        |transaction| audit_import_batch_repository_ownership(transaction, direct_ownership),
    )?;
    apply_migration_step(
        &transaction,
        19,
        IMPORT_BATCH_ATTESTATION_VALIDATION_SCHEMA_CHECKSUM,
        IMPORT_BATCH_ATTESTATION_VALIDATION_SQL,
        |_| Ok(()),
    )?;
    apply_migration_step(
        &transaction,
        20,
        IMPORT_BATCH_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM,
        "",
        |_| Ok(()),
    )?;
    apply_migration_step(
        &transaction,
        21,
        IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SCHEMA_CHECKSUM,
        IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SQL,
        repair_pre_audit_attestations,
    )?;
    apply_migration_step(
        &transaction,
        22,
        IMPORT_BATCH_FINAL_AUDIT_CHECKPOINT_SCHEMA_CHECKSUM,
        "",
        |_| Ok(()),
    )?;
    transaction.commit()?;
    Ok(())
}

fn apply_migration_step(
    transaction: &Transaction<'_>,
    version: i64,
    checksum: &str,
    sql: &str,
    validate: impl FnOnce(&Transaction<'_>) -> Result<(), AppError>,
) -> Result<(), AppError> {
    transaction.execute_batch(sql)?;
    validate(transaction)?;
    transaction.execute(
        "INSERT INTO schema_migrations (version, checksum, applied_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        (version, checksum),
    )?;
    transaction.pragma_update(None, "user_version", version)?;
    Ok(())
}

fn migration_exists(connection: &Connection, version: i64) -> Result<bool, AppError> {
    let exists: i64 = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM schema_migrations WHERE version = ?1)",
        [version],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

fn apply_migration(
    connection: &Connection,
    version: i64,
    checksum: &str,
    sql: &str,
) -> Result<(), AppError> {
    apply_validated_migration(connection, version, checksum, sql, |_| Ok(()))
}

fn apply_validated_migration(
    connection: &Connection,
    version: i64,
    checksum: &str,
    sql: &str,
    validate: impl FnOnce(&Transaction<'_>) -> Result<(), AppError>,
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
            validate(&transaction)?;
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

fn repair_pre_audit_attestations(transaction: &Transaction<'_>) -> Result<(), AppError> {
    if !migration_exists(transaction, 18)? {
        let migration_timestamp = schema_migration_timestamp(transaction, 14)?;
        let attestations = transaction
            .prepare(
                "SELECT attestation.import_id, attestation.authority, batch.imported_at,
                        EXISTS (
                            SELECT 1 FROM repositories repository
                             WHERE repository.id = attestation.repository_id
                               AND repository.workspace_id = batch.workspace_id
                               AND repository.is_planning_store = 0
                        )
                   FROM import_batch_repository_attestations attestation
                   JOIN import_batches batch ON batch.id = attestation.import_id
                  ORDER BY attestation.import_id",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (import_id, authority, imported_at, repository_is_valid) in attestations {
            let is_direct = import_timestamp(&import_id, &imported_at)? >= migration_timestamp;
            if !is_direct || authority != "explicit_repair" || !repository_is_valid {
                transaction.execute(
                    "DELETE FROM import_batch_repository_attestations WHERE import_id = ?1",
                    [import_id],
                )?;
            }
        }
    }
    transaction.execute_batch(
        "CREATE TRIGGER import_batch_repository_attestations_valid
         BEFORE INSERT ON import_batch_repository_attestations
         WHEN NOT EXISTS (
             SELECT 1
               FROM import_batches batch
               JOIN repositories repository
                 ON repository.id = NEW.repository_id
                AND repository.workspace_id = batch.workspace_id
                AND repository.is_planning_store = 0
              WHERE batch.id = NEW.import_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'import batch repository attestation is invalid');
         END;
         CREATE TRIGGER import_batch_repository_attestations_no_update
         BEFORE UPDATE ON import_batch_repository_attestations
         BEGIN
             SELECT RAISE(ABORT, 'import batch repository attestation is immutable');
         END;
         CREATE TRIGGER import_batch_repository_attestations_no_delete
         BEFORE DELETE ON import_batch_repository_attestations
         BEGIN
             SELECT RAISE(ABORT, 'import batch repository attestation cannot be deleted');
         END;",
    )?;
    Ok(())
}

fn prepare_import_batch_repository_audit(connection: &Connection) -> Result<(), AppError> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    transaction.execute_batch(IMPORT_BATCH_PRE_AUDIT_ATTESTATION_REPAIR_SQL)?;
    repair_pre_audit_attestations(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn capture_schema_14_direct_ownership(
    connection: &Connection,
) -> Result<HashMap<String, String>, AppError> {
    let migration_15_exists: i64 = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 15)",
        [],
        |row| row.get(0),
    )?;
    if migration_15_exists != 0 {
        return Ok(HashMap::new());
    }
    let migration_timestamp = schema_migration_timestamp(connection, 14)?;
    let batches = connection
        .prepare(
            "SELECT batch.id, batch.imported_at, batch.repository_id
               FROM import_batches batch
               JOIN repositories repository
                 ON repository.id = batch.repository_id
                AND repository.workspace_id = batch.workspace_id
                AND repository.is_planning_store = 0
              ORDER BY batch.id",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut ownership = HashMap::new();
    for (id, imported_at, repository_id) in batches {
        if import_timestamp(&id, &imported_at)? >= migration_timestamp {
            ownership.insert(id, repository_id);
        }
    }
    Ok(ownership)
}

fn restore_schema_14_direct_ownership(
    transaction: &Transaction<'_>,
    direct_ownership: &HashMap<String, String>,
) -> Result<(), AppError> {
    if direct_ownership.is_empty() {
        return Ok(());
    }
    transaction.execute_batch("DROP TRIGGER import_batches_repository_immutable;")?;
    for (import_id, repository_id) in direct_ownership {
        let updated = transaction.execute(
            "UPDATE import_batches SET repository_id = ?2
             WHERE id = ?1 AND EXISTS (
                 SELECT 1 FROM repositories repository
                  WHERE repository.id = ?2
                    AND repository.workspace_id = import_batches.workspace_id
                    AND repository.is_planning_store = 0
             )",
            params![import_id, repository_id],
        )?;
        if updated != 1 {
            return Err(AppError::Domain(format!(
                "captured direct repository ownership is invalid for import batch {import_id}"
            )));
        }
    }
    create_import_batch_repository_immutable_trigger(transaction)?;
    Ok(())
}

fn audit_import_batch_repository_ownership(
    transaction: &Transaction<'_>,
    direct_ownership: &HashMap<String, String>,
) -> Result<(), AppError> {
    let migration_timestamp = schema_migration_timestamp(transaction, 14)?;
    let batches = transaction
        .prepare(
            "SELECT id, workspace_id, kind, source_path, imported_at
               FROM import_batches ORDER BY id",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for (id, workspace_id, kind, source_path, imported_at) in batches {
        let direct = import_timestamp(&id, &imported_at)? >= migration_timestamp;
        let candidate = if direct {
            match direct_ownership.get(&id) {
                Some(repository_id) => Some((repository_id.clone(), Some("captured_direct"))),
                None => transaction
                    .query_row(
                        "SELECT attestation.repository_id
                           FROM import_batch_repository_attestations attestation
                           JOIN repositories repository
                             ON repository.id = attestation.repository_id
                            AND repository.workspace_id = ?2
                            AND repository.is_planning_store = 0
                          WHERE attestation.import_id = ?1
                            AND attestation.authority = 'explicit_repair'",
                        params![id, workspace_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .map(|repository_id| (repository_id, None)),
            }
        } else {
            immutable_import_repository(transaction, &id, &workspace_id, &kind, &source_path)?
                .map(|repository_id| (repository_id, Some("immutable_evidence")))
        };
        match candidate {
            Some((repository_id, authority)) => resolved.push((id, repository_id, authority)),
            None => unresolved.push(format!("{id} [{kind}: {source_path}]")),
        }
    }
    if !unresolved.is_empty() {
        return Err(AppError::Domain(format!(
            "schema migration 18 cannot attest immutable import repository ownership for {}; insert an explicit_repair row in import_batch_repository_attestations for each direct batch or repair the unique immutable evidence, then retry",
            unresolved.join(", ")
        )));
    }
    transaction.execute_batch("DROP TRIGGER import_batches_repository_immutable;")?;
    for (import_id, repository_id, authority) in resolved {
        transaction.execute(
            "UPDATE import_batches SET repository_id = ?2 WHERE id = ?1",
            params![import_id, repository_id],
        )?;
        if let Some(authority) = authority {
            transaction.execute(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![import_id, repository_id, authority],
            )?;
        }
    }
    create_import_batch_repository_immutable_trigger(transaction)?;
    validate_import_batch_repository_ownership(transaction, 18)
}

fn immutable_import_repository(
    transaction: &Transaction<'_>,
    import_id: &str,
    workspace_id: &str,
    kind: &str,
    source_path: &str,
) -> Result<Option<String>, AppError> {
    match kind {
        "context_catalogue" => transaction
            .query_row(
                "SELECT MIN(record.destination_id)
                   FROM legacy_import_records record
                   JOIN repositories repository ON repository.id = record.destination_id
                  WHERE record.import_id = ?1
                    AND record.destination_kind = 'repository'
                    AND repository.workspace_id = ?2
                    AND repository.is_planning_store = 0
                 HAVING COUNT(DISTINCT record.destination_id) = 1",
                params![import_id, workspace_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into),
        "concertable_plans" => transaction
            .query_row(
                "SELECT MIN(path.repository_id)
                   FROM repository_paths path
                   JOIN repositories repository ON repository.id = path.repository_id
                  WHERE path.path = ?1
                    AND repository.workspace_id = ?2
                    AND repository.is_planning_store = 0
                 HAVING COUNT(DISTINCT path.repository_id) = 1",
                params![source_path, workspace_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into),
        _ => Ok(None),
    }
}

fn schema_migration_timestamp(connection: &Connection, version: i64) -> Result<i128, AppError> {
    let migration_applied_at = connection.query_row(
        "SELECT applied_at FROM schema_migrations WHERE version = ?1",
        [version],
        |row| row.get::<_, String>(0),
    )?;
    Ok(OffsetDateTime::parse(&migration_applied_at, &Rfc3339)
        .map_err(|error| AppError::Domain(format!("invalid schema migration timestamp: {error}")))?
        .unix_timestamp_nanos())
}

fn import_timestamp(import_id: &str, value: &str) -> Result<i128, AppError> {
    value.parse::<i128>().map_err(|error| {
        AppError::Domain(format!(
            "invalid import timestamp for batch {import_id}: {error}"
        ))
    })
}

fn validate_import_batch_repository_ownership(
    transaction: &Transaction<'_>,
    migration_version: i64,
) -> Result<(), AppError> {
    let mut statement = transaction.prepare(
        "SELECT batch.id, batch.kind, batch.source_path
           FROM import_batches batch
           LEFT JOIN repositories repository
             ON repository.id = batch.repository_id
            AND repository.workspace_id = batch.workspace_id
            AND repository.is_planning_store = 0
          WHERE repository.id IS NULL
          ORDER BY batch.id",
    )?;
    let unresolved = statement
        .query_map([], |row| {
            Ok(format!(
                "{} [{}: {}]",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(AppError::Domain(format!(
        "schema migration {migration_version} cannot resolve immutable import repository ownership for {}; record exactly one matching non-planning repository path or legacy repository destination in the batch workspace, then retry",
        unresolved.join(", ")
    )))
}

fn validate_import_document_memberships(transaction: &Transaction<'_>) -> Result<(), AppError> {
    let mut statement = transaction.prepare(
        "SELECT batch.id
           FROM import_batches batch
           JOIN workspaces workspace ON workspace.id = batch.workspace_id
          WHERE batch.kind = 'concertable_plans'
            AND (
                EXISTS (
                    SELECT 1 FROM import_document_memberships membership
                     WHERE membership.import_id = batch.id
                )
                OR EXISTS (
                    SELECT 1 FROM import_source_destinations source
                     WHERE source.import_id = batch.id
                       AND source.document_id IS NOT NULL
                )
                OR EXISTS (
                    SELECT 1 FROM import_document_membership_finalizations finalization
                     WHERE finalization.import_id = batch.id
                )
            )
            AND (
                NOT EXISTS (
                    SELECT 1 FROM import_document_memberships membership
                     WHERE membership.import_id = batch.id
                )
                OR NOT EXISTS (
                    SELECT 1 FROM import_document_membership_finalizations finalization
                     WHERE finalization.import_id = batch.id
                )
                OR EXISTS (
                    SELECT 1
                      FROM import_source_destinations source
                      LEFT JOIN import_document_memberships membership
                        ON membership.import_id = source.import_id
                       AND membership.document_id = source.document_id
                       AND membership.destination_kind = source.destination_kind
                     WHERE source.import_id = batch.id
                       AND membership.document_id IS NULL
                )
                OR EXISTS (
                    SELECT 1
                      FROM import_document_memberships membership
                      JOIN documents document ON document.id = membership.document_id
                     WHERE membership.import_id = batch.id
                       AND NOT EXISTS (
                           SELECT 1 FROM import_source_destinations source
                            WHERE source.import_id = batch.id
                              AND source.document_id = membership.document_id
                              AND source.destination_kind = membership.destination_kind
                       )
                       AND NOT (
                           membership.destination_kind = 'epic'
                           AND EXISTS (
                               SELECT 1
                                 FROM features feature
                                 JOIN documents feature_document
                                   ON feature_document.feature_id = feature.id
                                 JOIN import_document_memberships feature_membership
                                   ON feature_membership.import_id = batch.id
                                  AND feature_membership.document_id = feature_document.id
                                  AND feature_membership.destination_kind = 'feature'
                                 JOIN import_source_destinations source
                                   ON source.import_id = batch.id
                                  AND source.document_id = feature_document.id
                                  AND source.destination_kind = 'feature'
                                WHERE feature.epic_id = document.epic_id
                           )
                       )
                )
                OR EXISTS (
                    SELECT 1
                      FROM epics epic
                     WHERE epic.workspace_id = batch.workspace_id
                       AND epic.created_at = batch.imported_at
                       AND NOT EXISTS (
                           SELECT 1
                             FROM documents document
                             JOIN import_document_memberships membership
                               ON membership.import_id = batch.id
                              AND membership.document_id = document.id
                              AND membership.destination_kind = 'epic'
                            WHERE document.epic_id = epic.id
                              AND document.repository_id = workspace.planning_store_repository_id
                              AND document.kind = 'epic'
                       )
                )
                OR EXISTS (
                    SELECT 1
                      FROM features feature
                      JOIN epics epic ON epic.id = feature.epic_id
                     WHERE epic.workspace_id = batch.workspace_id
                       AND epic.created_at = batch.imported_at
                       AND feature.created_at = batch.imported_at
                       AND NOT EXISTS (
                           SELECT 1
                             FROM documents document
                             JOIN import_document_memberships membership
                               ON membership.import_id = batch.id
                              AND membership.document_id = document.id
                              AND membership.destination_kind = 'feature'
                            WHERE document.feature_id = feature.id
                              AND document.repository_id = workspace.planning_store_repository_id
                              AND document.kind = 'feature'
                       )
                )
                OR EXISTS (
                    SELECT 1
                      FROM work_items work_item
                      JOIN features feature ON feature.id = work_item.feature_id
                      JOIN epics epic ON epic.id = feature.epic_id
                     WHERE epic.workspace_id = batch.workspace_id
                       AND epic.created_at = batch.imported_at
                       AND feature.created_at = batch.imported_at
                       AND work_item.created_at = batch.imported_at
                       AND NOT EXISTS (
                           SELECT 1
                             FROM documents document
                             JOIN import_document_memberships membership
                               ON membership.import_id = batch.id
                              AND membership.document_id = document.id
                              AND membership.destination_kind = 'work_item'
                            WHERE document.work_item_id = work_item.id
                              AND document.repository_id = workspace.planning_store_repository_id
                              AND document.kind = 'work_item'
                       )
                )
            )
          ORDER BY batch.id",
    )?;
    let unresolved = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(AppError::Domain(format!(
        "schema migration 24 cannot prove immutable Concertable import document membership for {}; restore the exact hierarchy, document revisions, and source mappings from a verified backup, then retry",
        unresolved.join(", ")
    )))
}

fn create_import_batch_repository_immutable_trigger(
    transaction: &Transaction<'_>,
) -> Result<(), AppError> {
    transaction.execute_batch(
        "CREATE TRIGGER import_batches_repository_immutable
         BEFORE UPDATE OF repository_id, workspace_id ON import_batches
         WHEN NEW.repository_id IS NOT OLD.repository_id
              OR NEW.workspace_id IS NOT OLD.workspace_id
         BEGIN
             SELECT RAISE(ABORT, 'import batch repository is immutable');
         END;",
    )?;
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
    use rusqlite::{Connection, Transaction, params};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::ConversationId;

    use super::SqliteStore;
    use crate::AppError;

    fn drop_import_document_membership_schema(connection: &Connection) {
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS import_source_destinations_finalized_insert;
                 DROP TRIGGER IF EXISTS import_source_destinations_finalized_update;
                 DROP TRIGGER IF EXISTS import_source_destinations_finalized_delete;
                 DROP TRIGGER IF EXISTS import_document_batches_finalized;
                 DROP TRIGGER IF EXISTS import_document_member_fields_immutable;
                 DROP TRIGGER IF EXISTS import_document_memberships_finalized;
                 DROP TRIGGER IF EXISTS import_document_membership_finalizations_no_update;
                 DROP TRIGGER IF EXISTS import_document_membership_finalizations_no_delete;
                 DROP TRIGGER IF EXISTS import_document_membership_finalizations_valid;
                 DROP TABLE IF EXISTS import_document_membership_finalizations;
                 DROP TRIGGER IF EXISTS import_document_memberships_no_update;
                 DROP TRIGGER IF EXISTS import_document_memberships_no_delete;
                 DROP TRIGGER IF EXISTS import_document_memberships_valid;
                 DROP TABLE IF EXISTS import_document_memberships;",
            )
            .expect("remove import document membership schema");
    }

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
    fn import_repository_migration_fails_closed_and_repairs_immutable_ownership() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        drop(store);

        let connection = Connection::open(&path).expect("open raw database");
        drop_import_document_membership_schema(&connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_batches_repository_required;
                 DROP TRIGGER import_batches_repository_immutable;
                 DROP TRIGGER import_batch_repository_parent_immutable;
                 DROP TRIGGER import_batch_repository_attestations_no_update;
                 DROP TRIGGER import_batch_repository_attestations_no_delete;
                 DROP TABLE import_batch_repository_attestations;
                 DROP INDEX import_batches_target;
                 ALTER TABLE import_batches DROP COLUMN repository_id;
                 DELETE FROM schema_migrations WHERE version >= 14;
                 PRAGMA user_version = 13;
                 INSERT INTO repositories (
                     id, workspace_id, slug, title, git_common_directory, default_branch,
                     is_planning_store, created_at
                 ) VALUES (
                     'aaa-repository', 'workspace', 'unrelated', 'Unrelated',
                     'C:/unrelated/.git', 'main', 0, '2026-08-27T08:00:00Z'
                 );
                 INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (
                     'unrelated-path', 'aaa-repository', 'C:/unrelated',
                     '2026-08-27T08:00:00Z', NULL
                 );
                 INSERT INTO work_item_repositories (work_item_id, repository_id)
                 VALUES ('work-item', 'aaa-repository');
                 INSERT INTO import_batches (
                     id, workspace_id, kind, source_path, source_head, preview_hash,
                     planning_commit, imported_at
                 ) VALUES (
                     'legacy-batch', 'workspace', 'concertable_plans', 'C:/code-linked',
                     'source-head',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'planning-commit', '1'
                 );
                 INSERT INTO import_source_destinations (
                     import_id, source_path, source_hash, destination_kind,
                     destination_id, document_id
                 ) VALUES (
                     'legacy-batch', 'plans/feature.md',
                     'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                     'work_item', 'work-item', NULL
                 );",
            )
            .expect("seed schema 13 import");
        drop(connection);

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("unresolved migration must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("legacy-batch"));
        assert!(error.to_string().contains("C:/code-linked"));

        let connection = Connection::open(&path).expect("inspect failed migration");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        let unsafe_repository: String = connection
            .query_row(
                "SELECT repository_id FROM import_batches WHERE id = 'legacy-batch'",
                [],
                |row| row.get(0),
            )
            .expect("schema 14 provisional repository");
        assert_eq!(version, 14);
        assert_eq!(unsafe_repository, "aaa-repository");
        drop(connection);

        let ambiguous_path = directory.path().join("ambiguous.sqlite");
        std::fs::copy(&path, &ambiguous_path).expect("copy unresolved database");
        let ambiguous = Connection::open(&ambiguous_path).expect("open ambiguous database");
        ambiguous
            .execute_batch(
                "INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES
                     (
                         'ambiguous-code-path', 'code-repository', 'C:/code-linked',
                         '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                     ),
                     (
                         'ambiguous-unrelated-path', 'aaa-repository', 'C:/code-linked',
                         '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                     );",
            )
            .expect("seed ambiguous immutable evidence");
        drop(ambiguous);
        let ambiguous_error = match SqliteStore::open(&ambiguous_path) {
            Ok(_) => panic!("ambiguous migration must fail"),
            Err(error) => error,
        };
        assert!(ambiguous_error.to_string().contains("legacy-batch"));

        let connection = Connection::open(&path).expect("open repairable database");
        connection
            .execute(
                "INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (
                     'linked-code-path', 'code-repository', 'C:/code-linked',
                     '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                 )",
                [],
            )
            .expect("record immutable source path identity");
        drop(connection);

        let mut store = SqliteStore::open(&path).expect("retry repaired migration");
        let repository: String = store
            .read(|connection| {
                Ok(connection.query_row(
                    "SELECT repository_id FROM import_batches WHERE id = 'legacy-batch'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("read repaired repository");
        assert_eq!(repository, "code-repository");
        assert!(store.health().expect("storage health").is_healthy());

        let missing_repository = store.write::<()>(|transaction| {
            transaction.execute(
                "INSERT INTO import_batches (
                     id, workspace_id, kind, source_path, source_head, preview_hash,
                     planning_commit, imported_at
                 ) VALUES (
                     'missing-repository', 'workspace', 'concertable_plans', 'C:/code',
                     'source-head',
                     'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                     'planning-commit', '2026-08-27T08:00:00Z'
                 )",
                [],
            )?;
            Ok(())
        });
        assert!(missing_repository.is_err());
        let reassignment = store.write::<()>(|transaction| {
            transaction.execute(
                "UPDATE import_batches SET repository_id = 'aaa-repository'
                 WHERE id = 'legacy-batch'",
                [],
            )?;
            Ok(())
        });
        assert!(reassignment.is_err());

        store
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO repositories (
                         id, workspace_id, slug, title, git_common_directory, default_branch,
                         is_planning_store, created_at
                     ) VALUES (
                         'other-store', 'other-workspace', 'planning', 'Other planning store',
                         'C:/other-planning/.git', 'main', 1, '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO workspaces (
                         id, slug, title, planning_store_repository_id, created_at
                     ) VALUES (
                         'other-workspace', 'other', 'Other', 'other-store',
                         '2026-08-27T08:00:00Z'
                     )",
                    [],
                )?;
                Ok(())
            })
            .expect("seed other workspace");
        let moved_parent = store.write::<()>(|transaction| {
            transaction.execute(
                "UPDATE repositories SET workspace_id = 'other-workspace'
                 WHERE id = 'code-repository'",
                [],
            )?;
            Ok(())
        });
        assert!(moved_parent.is_err());
        let planning_parent = store.write::<()>(|transaction| {
            transaction.execute(
                "UPDATE repositories SET is_planning_store = 1
                 WHERE id = 'code-repository'",
                [],
            )?;
            Ok(())
        });
        assert!(planning_parent.is_err());
    }

    #[test]
    fn schema_14_direct_import_repository_survives_upgrade() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        drop(store);

        let connection = Connection::open(&path).expect("open raw database");
        drop_import_document_membership_schema(&connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_batches_repository_required;
                 DROP TRIGGER import_batches_repository_immutable;
                 DROP TRIGGER import_batch_repository_parent_immutable;
                 DROP TRIGGER import_batch_repository_attestations_no_update;
                 DROP TRIGGER import_batch_repository_attestations_no_delete;
                 DROP TABLE import_batch_repository_attestations;
                 DELETE FROM schema_migrations WHERE version >= 15;
                 PRAGMA user_version = 14;
                 INSERT INTO repositories (
                     id, workspace_id, slug, title, git_common_directory, default_branch,
                     is_planning_store, created_at
                 ) VALUES (
                     'aaa-repository', 'workspace', 'unrelated', 'Unrelated',
                     'C:/unrelated/.git', 'main', 0, '2026-08-27T08:00:00Z'
                 );
                 INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (
                     'conflicting-history', 'aaa-repository', 'C:/code-linked',
                     '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                 );",
            )
            .expect("prepare schema 14");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'direct-batch', 'workspace', 'code-repository', 'concertable_plans',
                     'C:/code-linked', 'source-head',
                     'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
                     'planning-commit', ?1
                 )",
                [OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()],
            )
            .expect("seed direct schema 14 import");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'provisional-batch', 'workspace', 'aaa-repository',
                     'concertable_plans', 'C:/needs-repair', 'source-head',
                     'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff',
                     'planning-commit', '1'
                 )",
                [],
            )
            .expect("seed provisional schema 14 import");
        drop(connection);

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("atomic migration must roll back"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("provisional-batch"));

        let connection = Connection::open(&path).expect("inspect rolled-back upgrade");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        let direct_repository: String = connection
            .query_row(
                "SELECT repository_id FROM import_batches WHERE id = 'direct-batch'",
                [],
                |row| row.get(0),
            )
            .expect("read rolled-back direct repository");
        assert_eq!(version, 14);
        assert_eq!(direct_repository, "code-repository");
        connection
            .execute(
                "INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (
                     'provisional-repair-path', 'code-repository', 'C:/needs-repair',
                     '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                 )",
                [],
            )
            .expect("repair provisional ownership evidence");
        drop(connection);

        let store = SqliteStore::open(&path).expect("upgrade direct schema 14 import");
        let (repository, authority): (String, String) = store
            .read(|connection| {
                Ok(connection.query_row(
                    "SELECT batch.repository_id, attestation.authority
                       FROM import_batches batch
                       JOIN import_batch_repository_attestations attestation
                         ON attestation.import_id = batch.id
                      WHERE batch.id = 'direct-batch'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .expect("read direct repository");

        assert_eq!(repository, "code-repository");
        assert_eq!(authority, "captured_direct");
        assert!(store.health().expect("storage health").is_healthy());
    }

    #[test]
    fn stamped_schema_15_requires_explicit_direct_ownership_repair() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        drop(store);

        let connection = Connection::open(&path).expect("open raw database");
        drop_import_document_membership_schema(&connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_batch_repository_parent_immutable;
                 DROP TRIGGER import_batch_repository_attestations_no_update;
                 DROP TRIGGER import_batch_repository_attestations_no_delete;
                 DROP TABLE import_batch_repository_attestations;
                 DELETE FROM schema_migrations WHERE version >= 16;
                 PRAGMA user_version = 15;
                 INSERT INTO repositories (
                     id, workspace_id, slug, title, git_common_directory, default_branch,
                     is_planning_store, created_at
                 ) VALUES (
                     'aaa-repository', 'workspace', 'unrelated', 'Unrelated',
                     'C:/unrelated/.git', 'main', 0, '2026-08-27T08:00:00Z'
                 );
                 INSERT INTO repository_paths (
                     id, repository_id, path, observed_from, observed_until
                 ) VALUES (
                     'conflicting-history', 'aaa-repository', 'C:/code-linked',
                     '2026-08-27T08:00:00Z', '2026-08-27T09:00:00Z'
                 );",
            )
            .expect("prepare stamped schema 15");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'lost-direct-batch', 'workspace', 'aaa-repository',
                     'concertable_plans', 'C:/code-linked', 'source-head',
                     'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
                     'planning-commit', ?1
                 )",
                [OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()],
            )
            .expect("seed overwritten direct ownership");
        drop(connection);

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("lost direct provenance must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("lost-direct-batch"));
        assert!(error.to_string().contains("explicit_repair"));

        let connection = Connection::open(&path).expect("open repair database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 21);
        let invalid_attestation = connection.execute(
            "INSERT INTO import_batch_repository_attestations (
                 import_id, repository_id, authority, confirmed_at
             ) VALUES (
                 'lost-direct-batch', 'store-repository', 'explicit_repair',
                 '2026-08-28T11:00:00Z'
             )",
            [],
        );
        assert!(invalid_attestation.is_err());
        connection
            .execute(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'lost-direct-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T12:00:00Z'
                 )",
                [],
            )
            .expect("attest repaired ownership");
        drop(connection);

        let store = SqliteStore::open(&path).expect("retry attested migration");
        let repository: String = store
            .read(|connection| {
                Ok(connection.query_row(
                    "SELECT repository_id FROM import_batches WHERE id = 'lost-direct-batch'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("read attested repository");

        assert_eq!(repository, "code-repository");
        assert!(store.health().expect("storage health").is_healthy());
    }

    #[test]
    fn schema_17_unusable_attestations_can_be_repaired() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("workboard.sqlite");
        let mut store = SqliteStore::open(&path).expect("open store");
        store.write(seed_hierarchy).expect("seed hierarchy");
        drop(store);

        let connection = Connection::open(&path).expect("open raw database");
        drop_import_document_membership_schema(&connection);
        connection
            .execute_batch(
                "DROP TRIGGER import_batch_repository_attestations_valid;
                 DELETE FROM schema_migrations WHERE version >= 18;
                 PRAGMA user_version = 17;",
            )
            .expect("prepare prior schema 17");
        let imported_at = OffsetDateTime::now_utc().unix_timestamp_nanos().to_string();
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'invalid-attestation-batch', 'workspace', 'code-repository',
                     'concertable_plans', 'C:/invalid-attestation', 'source-head',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'planning-commit', ?1
                 )",
                [&imported_at],
            )
            .expect("seed direct batch with invalid attestation");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'valid-attestation-batch', 'workspace', 'code-repository',
                     'concertable_plans', 'C:/valid-attestation', 'source-head',
                     'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                     'planning-commit', ?1
                 )",
                [&imported_at],
            )
            .expect("seed direct batch with valid attestation");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'unusable-authority-batch', 'workspace', 'code-repository',
                     'concertable_plans', 'C:/unusable-authority', 'source-head',
                     'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                     'planning-commit', ?1
                 )",
                [&imported_at],
            )
            .expect("seed direct batch with unusable attestation authority");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'legacy-attestation-batch', 'workspace', 'code-repository',
                     'concertable_plans', 'C:/code', 'source-head',
                     'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
                     'planning-commit', '0'
                 )",
                [],
            )
            .expect("seed legacy batch with pre-audit attestation");
        connection
            .execute_batch(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'invalid-attestation-batch', 'store-repository', 'explicit_repair',
                     '2026-08-28T11:00:00Z'
                 );
                 INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'valid-attestation-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T11:00:00Z'
                 );
                 INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'unusable-authority-batch', 'code-repository', 'captured_direct',
                     '2026-08-28T11:00:00Z'
                 );
                 INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'legacy-attestation-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T11:00:00Z'
                 );",
            )
            .expect("seed prior schema 17 attestations");
        drop(connection);

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("invalid prior attestation must require repair"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("invalid-attestation-batch"));
        assert!(error.to_string().contains("unusable-authority-batch"));
        assert!(error.to_string().contains("explicit_repair"));

        let connection = Connection::open(&path).expect("open repair database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        let discarded_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM import_batch_repository_attestations
                  WHERE import_id IN (
                      'invalid-attestation-batch',
                      'unusable-authority-batch',
                      'legacy-attestation-batch'
                  )",
                [],
                |row| row.get(0),
            )
            .expect("count discarded unusable attestations");
        let valid_attestation: (String, String, String) = connection
            .query_row(
                "SELECT repository_id, authority, confirmed_at
                   FROM import_batch_repository_attestations
                  WHERE import_id = 'valid-attestation-batch'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read preserved valid attestation");
        assert_eq!(version, 21);
        assert_eq!(discarded_count, 0);
        assert_eq!(
            valid_attestation,
            (
                "code-repository".to_owned(),
                "explicit_repair".to_owned(),
                "2026-08-28T11:00:00Z".to_owned()
            )
        );
        assert!(
            connection
                .execute(
                    "UPDATE import_batch_repository_attestations
                        SET confirmed_at = '2026-08-28T12:00:00Z'
                      WHERE import_id = 'valid-attestation-batch'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM import_batch_repository_attestations
                      WHERE import_id = 'valid-attestation-batch'",
                    [],
                )
                .is_err()
        );
        let invalid_attestation = connection.execute(
            "INSERT INTO import_batch_repository_attestations (
                 import_id, repository_id, authority, confirmed_at
             ) VALUES (
                 'invalid-attestation-batch', 'store-repository', 'explicit_repair',
                 '2026-08-28T12:00:00Z'
             )",
            [],
        );
        assert!(invalid_attestation.is_err());
        connection
            .execute_batch(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'unusable-authority-batch', 'code-repository', 'captured_direct',
                     '2026-08-28T12:00:00Z'
                 );
                 INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'legacy-attestation-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T12:00:00Z'
                 );",
            )
            .expect("seed unusable attestations after repair checkpoint");
        drop(connection);

        let error = match SqliteStore::open(&path) {
            Ok(_) => panic!("post-checkpoint unusable attestations must be cleared"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("invalid-attestation-batch"));
        assert!(error.to_string().contains("unusable-authority-batch"));

        let connection = Connection::open(&path).expect("reopen repair database");
        let discarded_retry_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM import_batch_repository_attestations
                  WHERE import_id IN (
                      'unusable-authority-batch',
                      'legacy-attestation-batch'
                  )",
                [],
                |row| row.get(0),
            )
            .expect("count retry-discarded attestations");
        assert_eq!(discarded_retry_count, 0);
        connection
            .execute(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'invalid-attestation-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T13:00:00Z'
                 )",
                [],
            )
            .expect("replace discarded attestation with valid repair");
        connection
            .execute(
                "INSERT INTO import_batch_repository_attestations (
                     import_id, repository_id, authority, confirmed_at
                 ) VALUES (
                     'unusable-authority-batch', 'code-repository', 'explicit_repair',
                     '2026-08-28T13:00:00Z'
                 )",
                [],
            )
            .expect("replace unusable attestation authority with valid repair");
        drop(connection);

        let store = SqliteStore::open(&path).expect("retry schema 17 upgrade");
        let (repaired_batches, preserved_attestation, legacy_authority): (
            i64,
            (String, String, String),
            String,
        ) = store
            .read(|connection| {
                let repaired_batches = connection.query_row(
                    "SELECT COUNT(*) FROM import_batches
                      WHERE id IN (
                          'invalid-attestation-batch',
                          'valid-attestation-batch',
                          'unusable-authority-batch',
                          'legacy-attestation-batch'
                      ) AND repository_id = 'code-repository'",
                    [],
                    |row| row.get(0),
                )?;
                let preserved_attestation = connection.query_row(
                    "SELECT repository_id, authority, confirmed_at
                       FROM import_batch_repository_attestations
                      WHERE import_id = 'valid-attestation-batch'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                let legacy_authority = connection.query_row(
                    "SELECT authority FROM import_batch_repository_attestations
                      WHERE import_id = 'legacy-attestation-batch'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((repaired_batches, preserved_attestation, legacy_authority))
            })
            .expect("read repaired batches");

        assert_eq!(repaired_batches, 4);
        assert_eq!(preserved_attestation, valid_attestation);
        assert_eq!(legacy_authority, "immutable_evidence");
        let health = store.health().expect("storage health");
        assert_eq!(health.schema_version, 24);
        assert!(health.is_healthy());
        let audited_attestations: Vec<(String, String, String, String)> = store
            .read(|connection| {
                Ok(connection
                    .prepare(
                        "SELECT import_id, repository_id, authority, confirmed_at
                           FROM import_batch_repository_attestations
                          ORDER BY import_id",
                    )?
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .expect("snapshot audited attestations");
        drop(store);

        let connection = Connection::open(&path).expect("open schema 20 database");
        drop_import_document_membership_schema(&connection);
        connection
            .execute_batch(
                "DELETE FROM schema_migrations WHERE version >= 21;
                 PRAGMA user_version = 20;",
            )
            .expect("prepare completed schema 20 audit");
        drop(connection);

        let store = SqliteStore::open(&path).expect("upgrade completed schema 20 audit");
        let upgraded_attestations: Vec<(String, String, String, String)> = store
            .read(|connection| {
                Ok(connection
                    .prepare(
                        "SELECT import_id, repository_id, authority, confirmed_at
                           FROM import_batch_repository_attestations
                          ORDER BY import_id",
                    )?
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .expect("read upgraded schema 20 attestations");
        assert_eq!(upgraded_attestations, audited_attestations);
        let health = store.health().expect("upgraded storage health");
        assert_eq!(health.schema_version, 24);
        assert!(health.is_healthy());
        drop(store);

        let connection = Connection::open(&path).expect("inspect upgraded guards");
        connection
            .execute(
                "INSERT INTO import_batches (
                     id, workspace_id, repository_id, kind, source_path, source_head,
                     preview_hash, planning_commit, imported_at
                 ) VALUES (
                     'post-audit-validation-batch', 'workspace', 'code-repository',
                     'concertable_plans', 'C:/post-audit-validation', 'source-head',
                     'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff',
                     'planning-commit', ?1
                 )",
                [OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()],
            )
            .expect("seed post-audit validation batch");
        assert!(
            connection
                .execute(
                    "INSERT INTO import_batch_repository_attestations (
                         import_id, repository_id, authority, confirmed_at
                     ) VALUES (
                         'post-audit-validation-batch', 'store-repository',
                         'explicit_repair', '2026-08-28T14:00:00Z'
                     )",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "UPDATE import_batch_repository_attestations
                        SET confirmed_at = '2026-08-28T14:00:00Z'
                      WHERE import_id = 'valid-attestation-batch'",
                    [],
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    "DELETE FROM import_batch_repository_attestations
                      WHERE import_id = 'valid-attestation-batch'",
                    [],
                )
                .is_err()
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
