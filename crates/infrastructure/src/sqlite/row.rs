use rusqlite::Row;
use rusqlite::types::Type;

pub trait FromRow: Sized {
    fn from_row(row: &Row) -> rusqlite::Result<Self>;
}

pub(crate) fn conversion_failure(
    column: usize,
    kind: Type,
    err: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, kind, Box::new(err))
}

pub(crate) fn column_index(row: &Row, name: &str) -> usize {
    row.as_ref().column_index(name).unwrap_or(0)
}
