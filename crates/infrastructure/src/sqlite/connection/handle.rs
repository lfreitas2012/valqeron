use crate::sqlite::connection::dry_run::current_dry_run_conn;
use crate::sqlite::connection::guard::{ReadGuard, WriteGuard};
use crate::sqlite::connection::pool::{ReaderPool, ReaderSource, lock_writer};
use crate::sqlite::connection::pragmas::SharedConnection;

pub(crate) trait Db {
    fn write(&self) -> WriteGuard<'_>;

    fn read(&self) -> ReadGuard<'_>;
}

#[derive(Clone)]
pub(crate) enum DbHandle {
    Live {
        writer: SharedConnection,
        readers: ReaderSource,
    },
    DryRun,
}

impl Db for DbHandle {
    fn write(&self) -> WriteGuard<'_> {
        let guard = match self {
            DbHandle::Live { writer, .. } => WriteGuard::Locked(lock_writer(writer)),
            DbHandle::DryRun => WriteGuard::Borrowed(current_dry_run_conn()),
        };
        guard.start_operation();
        guard
    }

    fn read(&self) -> ReadGuard<'_> {
        let guard = match self {
            DbHandle::Live { writer, readers } => match readers {
                ReaderSource::Pool(pool) => ReadGuard::Pooled(ReaderPool::checkout(pool)),
                ReaderSource::SharedWithWriter => ReadGuard::Locked(lock_writer(writer)),
            },
            DbHandle::DryRun => ReadGuard::Borrowed(current_dry_run_conn()),
        };
        guard.start_operation();
        guard
    }
}
