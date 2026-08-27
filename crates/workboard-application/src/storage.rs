use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, MAIN_DB, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::AppError;

const FOUNDATION_SCHEMA_VERSION: i64 = 1;
const FOUNDATION_SCHEMA_CHECKSUM: &str = "agent-workboard-foundation-v1";

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

impl StorageHealth {
    pub fn is_healthy(&self) -> bool {
        self.integrity == "ok"
            && self.foreign_key_violations == 0
            && self.schema_version == FOUNDATION_SCHEMA_VERSION
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
}

fn migrate(connection: &Connection) -> Result<(), AppError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             checksum TEXT NOT NULL,
             applied_at TEXT NOT NULL
         );",
    )?;
    let existing = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            [FOUNDATION_SCHEMA_VERSION],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match existing {
        Some(checksum) if checksum != FOUNDATION_SCHEMA_CHECKSUM => {
            return Err(AppError::Domain(format!(
                "schema migration {FOUNDATION_SCHEMA_VERSION} checksum mismatch"
            )));
        }
        Some(_) => {}
        None => {
            connection.execute(
                "INSERT INTO schema_migrations (version, checksum, applied_at)
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                (FOUNDATION_SCHEMA_VERSION, FOUNDATION_SCHEMA_CHECKSUM),
            )?;
        }
    }
    connection.pragma_update(None, "user_version", FOUNDATION_SCHEMA_VERSION)?;
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
}
