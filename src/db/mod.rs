//! Database layer with SQLite

pub mod repository;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Index `i` takes the schema from version `i` to `i + 1` (`PRAGMA user_version`).
/// Once a release has shipped, only append: a stamped database must match its version.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/001_initial.sql"),
    include_str!("../../migrations/002_link_dir.sql"),
];

/// Database wrapper with connection pooling
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open the database at `path`, creating it and any missing directories readable
    /// only by the owner: credentials are stored in it in plain text.
    pub fn new(path: &Path) -> Result<Self> {
        create_private(path).with_context(|| format!("Failed to create database at {:?}", path))?;
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

/// New directories get 0700 and the database and its WAL files 0600; directories that
/// already exist are left alone, since the path may point into one the user owns.
#[cfg(unix)]
fn create_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        if Path::new(&file).exists() {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> std::io::Result<()> {
    match path.parent() {
        Some(dir) => std::fs::create_dir_all(dir),
        None => Ok(()),
    }
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

    #[cfg(unix)]
    #[test]
    fn the_database_and_the_directories_created_for_it_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let root = std::env::temp_dir().join(format!("graft-db-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = root.join("data").join("graft.db");

        let db = Database::new(&path).unwrap();
        db.migrate().unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&root.join("data/graft.db-wal")), 0o600);
        assert_eq!(mode(&root.join("data")), 0o700);
        assert_eq!(mode(&root), 0o755, "a directory that already existed keeps its mode");
        drop(db);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        Database::new(&path).unwrap();
        assert_eq!(mode(&path), 0o600, "an older, readable database is tightened");
        std::fs::remove_dir_all(&root).unwrap();
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
    fn a_database_from_an_older_build_is_upgraded_keeping_its_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO clients (id, name, client_type, host, port, password)
             VALUES ('c', 'qb', 'qbittorrent', 'localhost', 8080, 'pw')",
            [],
        )
        .unwrap();
        run_migrations(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        let kept: (String, Option<String>) =
            conn.query_row("SELECT password, link_dir FROM clients WHERE id = 'c'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(kept, ("pw".to_string(), None));
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
                "INSERT INTO sites (id, name, base_url, template_type, download_pattern)
                 VALUES (?1, 's', 'https://s', ?1, '/d?id={id}')",
                [template],
            )
        };
        assert!(insert("nexusphp").is_ok());
        assert!(insert("bogus").is_err());
    }
}
