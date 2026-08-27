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

const CURRENT_SCHEMA_VERSION: i64 = 2;
const FOUNDATION_SCHEMA_CHECKSUM: &str = "agent-workboard-foundation-v1";
const LAUNCH_LEASE_SCHEMA_CHECKSUM: &str = "agent-workboard-launch-leases-v1";

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
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::ConversationId;

    use super::SqliteStore;
    use crate::AppError;

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
}
