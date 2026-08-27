use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportPreview {
    pub source: PathBuf,
    pub tables: Vec<String>,
    pub repositories: u64,
    pub native_sessions: u64,
    pub association_events: u64,
    pub checkouts: u64,
    pub warnings: Vec<String>,
}

pub fn preview_context_catalogue(path: &Path) -> Result<LegacyImportPreview, AppError> {
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::Domain(format!(
            "legacy database is unavailable: {}",
            path.display()
        )));
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
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
    Ok(LegacyImportPreview {
        source: path.to_path_buf(),
        tables,
        repositories,
        native_sessions,
        association_events,
        checkouts,
        warnings,
    })
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

    use rusqlite::Connection;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::preview_context_catalogue;

    #[test]
    fn preview_reads_known_inventory_without_mutating_the_source_database() {
        let directory = TempDir::new().expect("temporary directory");
        let path = directory.path().join("legacy.sqlite");
        let connection = Connection::open(&path).expect("create legacy database");
        connection
            .execute_batch(
                "CREATE TABLE repositories (id TEXT PRIMARY KEY);
                 CREATE TABLE conversations (id TEXT PRIMARY KEY);
                 CREATE TABLE association_events (id TEXT PRIMARY KEY);
                 CREATE TABLE worktrees (id TEXT PRIMARY KEY);
                 INSERT INTO repositories VALUES ('repository');
                 INSERT INTO conversations VALUES ('one'), ('two');
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
        assert_eq!(before, after);
        assert!(!path.with_extension("sqlite-wal").exists());
        assert!(!path.with_extension("sqlite-shm").exists());
    }
}
