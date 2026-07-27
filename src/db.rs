use rusqlite::{Connection, TransactionBehavior};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const MIGRATIONS: &[&str] = &[include_str!(
    "../migrations/001_create_initial_issuer_schema.sql"
)];

pub type SharedConnection = Arc<Mutex<Connection>>;

#[derive(Debug, thiserror::Error)]
pub enum SqliteDataDriverError {
    #[error("failed to open sqlite connection")]
    Connection {
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to configure connection")]
    Pragma {
        #[source]
        source: rusqlite::Error,
    },

    #[error("migration failed")]
    Migration {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error(
        "database schema version {found} is newer than the {known} migration(s) this binary knows about — upgrade the binary"
    )]
    UnknownSchemaVersion { found: i64, known: usize },

    #[error("failed to open dry-run savepoint")]
    DryRun {
        #[source]
        source: rusqlite::Error,
    },
}

pub(crate) fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Database {
    conn: SharedConnection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteDataDriverError> {
        let mut conn = Connection::open(path)
            .map_err(|source| SqliteDataDriverError::Connection { source })?;
        Self::init(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> Result<Self, SqliteDataDriverError> {
        let mut conn = Connection::open_in_memory()
            .map_err(|source| SqliteDataDriverError::Connection { source })?;
        Self::init(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init(conn: &mut Connection) -> Result<(), SqliteDataDriverError> {
        configure(conn)?;
        run_migrations(conn)?;
        Ok(())
    }

    pub fn connection(&self) -> SharedConnection {
        Arc::clone(&self.conn)
    }

    pub fn begin_dry_run(&self) -> Result<DryRunGuard, SqliteDataDriverError> {
        DryRunGuard::begin(self.connection())
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Err(e) = lock(&self.conn).execute_batch("PRAGMA optimize;") {
            tracing::warn!(error = %e, "PRAGMA optimize failed on close");
        }
    }
}

fn configure(conn: &Connection) -> Result<(), SqliteDataDriverError> {
    let pragma_err = |source| SqliteDataDriverError::Pragma { source };

    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(pragma_err)?;

    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(pragma_err)?;

    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(pragma_err)?;

    conn.busy_timeout(Duration::from_secs(5))
        .map_err(pragma_err)?;

    conn.pragma_update(None, "cache_size", -64_000i64)
        .map_err(pragma_err)?;

    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(pragma_err)?;

    conn.pragma_update(None, "mmap_size", 256i64 * 1024 * 1024)
        .map_err(pragma_err)?;

    conn.set_prepared_statement_cache_capacity(64);

    Ok(())
}

fn run_migrations(connection: &mut Connection) -> Result<(), SqliteDataDriverError> {
    fn migration_err(
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> SqliteDataDriverError {
        SqliteDataDriverError::Migration {
            source: Box::new(source),
        }
    }

    let current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(migration_err)?;

    if current_version as usize > MIGRATIONS.len() {
        return Err(SqliteDataDriverError::UnknownSchemaVersion {
            found: current_version,
            known: MIGRATIONS.len(),
        });
    }

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let migration_version = (index + 1) as i64;
        if migration_version <= current_version {
            continue;
        }

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(migration_err)?;

        tx.execute_batch(sql).map_err(migration_err)?;

        tx.pragma_update(None, "user_version", migration_version)
            .map_err(migration_err)?;

        tx.commit().map_err(migration_err)?;
    }

    Ok(())
}

pub struct DryRunGuard {
    conn: SharedConnection,
}

impl DryRunGuard {
    fn begin(conn: SharedConnection) -> Result<Self, SqliteDataDriverError> {
        lock(&conn)
            .execute_batch("SAVEPOINT dry_run")
            .map_err(|source| SqliteDataDriverError::DryRun { source })?;
        Ok(Self { conn })
    }
}

impl Drop for DryRunGuard {
    fn drop(&mut self) {
        let result = lock(&self.conn).execute_batch("ROLLBACK TO dry_run; RELEASE dry_run;");
        if let Err(e) = result {
            tracing::error!(
                error = %e,
                "dry-run rollback failed — database may retain uncommitted changes"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_ends_up_at_latest_version() {
        let db = Database::open_in_memory().unwrap();
        let version: i64 = lock(&db.conn)
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn opening_twice_is_a_harmless_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();
    }

    #[test]
    fn schema_from_the_future_is_rejected_rather_than_silently_skipped() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure(&mut conn).unwrap();

        conn.pragma_update(None, "user_version", (MIGRATIONS.len() as i64) + 5)
            .unwrap();

        let result = run_migrations(&mut conn);
        assert!(matches!(
            result,
            Err(SqliteDataDriverError::UnknownSchemaVersion { .. })
        ));
    }

    #[test]
    fn dry_run_guard_rolls_back_writes_on_drop() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.connection();

        {
            let _guard = db.begin_dry_run().unwrap();

            lock(&conn)
                .execute_batch(
                    "INSERT INTO issuer (id, status, created_at)
                     VALUES (randomblob(16), 'ACTIVE', '2026-01-01T00:00:00Z')",
                )
                .unwrap();

            let count: i64 = lock(&conn)
                .query_row("SELECT COUNT(*) FROM issuer", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                count, 1,
                "insert should be visible while the guard is alive"
            );
        } // _guard drops here — rolls back

        let count: i64 = lock(&conn)
            .query_row("SELECT COUNT(*) FROM issuer", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "insert should be gone once the guard drops");
    }

    #[test]
    fn without_a_guard_writes_persist_normally() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.connection();

        lock(&conn)
            .execute_batch(
                "INSERT INTO issuer (id, status, created_at)
                 VALUES (randomblob(16), 'ACTIVE', '2026-01-01T00:00:00Z')",
            )
            .unwrap();

        let count: i64 = lock(&conn)
            .query_row("SELECT COUNT(*) FROM issuer", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn shared_connection_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SharedConnection>();
    }
}
