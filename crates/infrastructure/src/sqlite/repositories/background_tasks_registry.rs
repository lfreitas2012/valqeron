use crate::sqlite::database::{Db, DbHandle};
use crate::sqlite::row::{FromRow, column_datetime, column_index, column_uuid, conversion_failure};
use crate::sqlite::support::backend;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::str::FromStr;
use valqeron_core::common::{RepositoryResult, UniqueIdentifier, Versioned};
use valqeron_core::{
    BackgroundTask, BackgroundTaskName, BackgroundTaskSnapshot, BackgroundTasksRegistryRepository,
};

// ======================== MODEL ========================
#[derive(Debug)]
pub(crate) struct BackgroundTasksRegistryRow(pub Versioned<BackgroundTaskSnapshot>);

impl BackgroundTasksRegistryRow {
    pub(crate) fn into_inner(self) -> Versioned<BackgroundTaskSnapshot> {
        self.0
    }
}

impl FromRow for BackgroundTasksRegistryRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let snapshot = BackgroundTaskSnapshot {
            id: column_background_tasks_registry_id(row, "id")?,
            name: column_background_task_name(row, "name")?,
            created_at: column_datetime(row, "created_at")?,
            last_updated_at: column_datetime(row, "last_updated_at")?,
        };

        Ok(Self(Versioned {
            data: snapshot,
            version: row.get("version")?,
        }))
    }
}

// ======================== REPOSITORY ========================
const BACKGROUND_TASK_REGISTRY_VERSION_SQL: &str =
    "SELECT version FROM background_tasks_registry WHERE id = ?1";

pub struct SqliteBackgroundTasksRepository {
    db: DbHandle,
}

impl SqliteBackgroundTasksRepository {
    pub(crate) fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

fn reconstitute_lazy(row: BackgroundTasksRegistryRow) -> Versioned<BackgroundTask> {
    let Versioned { data, version } = row.into_inner();
    Versioned {
        data: BackgroundTask::reconstitute(data),
        version,
    }
}

impl BackgroundTasksRegistryRepository for SqliteBackgroundTasksRepository {
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn find_by_id(
        &self,
        id: &UniqueIdentifier,
    ) -> RepositoryResult<Option<Versioned<BackgroundTask>>> {
        let conn = self.db.read();
        let Some(row) = find_by_id(&conn, id).map_err(backend)? else {
            return Ok(None);
        };

        Ok(Some(reconstitute_lazy(row)))
    }

    /// # Errors
    ///
    /// Returns `StorageFault`
    fn list_paged(
        &self,
        after: Option<UniqueIdentifier>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<BackgroundTask>>> {
        let conn = self.db.read();
        let rows = list_paged(&conn, after.as_ref(), limit).map_err(backend)?;

        Ok(rows.into_iter().map(reconstitute_lazy).collect())
    }
}

// ======================== QUERIES ========================
const BACKGROUND_TASK_REGISTRY_COLUMNS: &str = "id, name, created_at, last_updated_at, version";

fn find_by_id(
    conn: &Connection,
    id: &UniqueIdentifier,
) -> rusqlite::Result<Option<BackgroundTasksRegistryRow>> {
    let sql = format!(
        "SELECT {BACKGROUND_TASK_REGISTRY_COLUMNS} FROM background_tasks_registry WHERE id = ?1"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.query_row(params![id.as_bytes()], BackgroundTasksRegistryRow::from_row)
        .optional()
}

fn list_paged(
    conn: &Connection,
    after: Option<&UniqueIdentifier>,
    limit: u32,
) -> rusqlite::Result<Vec<BackgroundTasksRegistryRow>> {
    match after {
        Some(id) => {
            let sql = format!(
                "SELECT {BACKGROUND_TASK_REGISTRY_COLUMNS} FROM background_tasks_registry WHERE id > ?1 ORDER BY id LIMIT ?2"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(
                params![id.as_bytes(), limit],
                BackgroundTasksRegistryRow::from_row,
            )?
            .collect()
        }
        None => {
            let sql = format!(
                "SELECT {BACKGROUND_TASK_REGISTRY_COLUMNS} FROM background_tasks_registry ORDER BY id LIMIT ?1"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            stmt.query_map(params![limit], BackgroundTasksRegistryRow::from_row)?
                .collect()
        }
    }
}

// ======================== MAPPINGS ========================
fn column_background_tasks_registry_id(
    row: &Row,
    name: &str,
) -> rusqlite::Result<UniqueIdentifier> {
    column_uuid(row, name).map(UniqueIdentifier::from_uuid)
}

fn column_background_task_name(row: &Row, name: &str) -> rusqlite::Result<BackgroundTaskName> {
    let raw: String = row.get(name)?;
    BackgroundTaskName::from_str(&raw)
        .map_err(|e| conversion_failure(column_index(row, name), Type::Text, e))
}

#[cfg(test)]
mod tests {
    use crate::sqlite::database::{Database, TempDatabase};
    use crate::sqlite::repositories::SqliteBackgroundTasksRepository;
    use valqeron_core::BackgroundTasksRegistryRepository;
    use valqeron_core::common::UniqueIdentifier;

    fn test_repo() -> (TempDatabase, SqliteBackgroundTasksRepository) {
        let db = Database::open_temp();
        let repo = SqliteBackgroundTasksRepository::new(db.handle());
        (db, repo)
    }

    #[test]
    fn test_find_by_id_returns_none_for_nonexistent_id() {
        let (_db, repo) = test_repo();
        let found = repo.find_by_id(&UniqueIdentifier::new()).unwrap();
        assert!(found.is_none());
    }
}
