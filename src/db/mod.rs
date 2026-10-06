//! Database layer with SQLite

pub mod repository;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Index `i` takes the schema from version `i` to `i + 1` (`PRAGMA user_version`).
/// Once a release has shipped, only append: a stamped database must match its version.
const MIGRATIONS: &[&str] = &[include_str!("../../migrations/001_initial.sql")];

/// Database wrapper with connection pooling
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Create a new database connection
    pub fn new(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create database directory")?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at {:?}", path))?;

        // Enable foreign keys and WAL mode for better performance
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Bring the schema up to the latest version.
    ///
    /// # Errors
    /// Refuses, without touching the schema, a database that has tables but no schema
    /// version (written before versioning existed) or one stamped with a version this
    /// build does not know. A failing migration is rolled back and also returns an error.
    pub fn migrate(&self) -> Result<()> {
        run_migrations(&mut self.conn())
    }

    /// Get a connection for executing queries
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

fn run_migrations(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let known = MIGRATIONS.len() as i64;
    if !(0..=known).contains(&version) {
        bail!(
            "the database is at schema version {version}, but this Graft only knows up to \
             {known}; it was probably written by a newer Graft"
        );
    }
    if version == 0 {
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if tables > 0 {
            bail!(
                "the database was created by a pre-release Graft without schema versioning \
                 and cannot be upgraded; delete it together with its -wal and -shm files, \
                 then add your clients and sites again"
            );
        }
    }

    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("schema migration {} failed", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap()
    }

    #[test]
    fn fresh_database_gets_the_latest_schema() {
        let conn = migrated();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        conn.execute(
            "INSERT INTO clients (id, name, client_type, host, port, password)
             VALUES ('c', 'qb', 'qbittorrent', 'localhost', 8080, 'pw')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn migrating_again_changes_nothing() {
        let mut conn = migrated();
        run_migrations(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn unversioned_database_with_tables_is_refused_untouched() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE clients (id TEXT PRIMARY KEY)").unwrap();
        let err = run_migrations(&mut conn).unwrap_err().to_string();
        assert!(err.contains("delete it"), "{err}");
        assert_eq!(version(&conn), 0);
        let sites: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'sites'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sites, 0);
    }

    #[test]
    fn database_from_a_newer_build_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(run_migrations(&mut conn).is_err());
        assert_eq!(version(&conn), 99);
    }

    #[test]
    fn unknown_template_type_cannot_be_stored() {
        let conn = migrated();
        let insert = |template: &str| {
            conn.execute(
                "INSERT INTO sites (id, name, base_url, template_type) VALUES (?1, 's', 'https://s', ?1)",
                [template],
            )
        };
        assert!(insert("nexusphp").is_ok());
        assert!(insert("bogus").is_err());
    }
}
