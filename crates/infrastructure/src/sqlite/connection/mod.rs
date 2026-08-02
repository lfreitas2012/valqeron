//! The SQLite connection layer.
//!
//! Everything about *talking to SQLite connections* lives here, split by responsibility so each
//! file owns one cohesive concern and the dependency graph is one-directional:
//!
//! ```text
//! sync ─┬─► pragmas ─┐
//!       └─► pool ────┼─► guard ─► handle ─► database
//!           dry_run ─┘            ▲
//!           config ───────────────┴─► database
//! ```
//!
//! * [`sync`] — loom/std synchronization shim.
//! * [`config`] — [`DatabaseConfig`].
//! * [`pragmas`] — opening a single connection and applying its pragma profile.
//! * [`pool`] — the reader pool, its RAII handle, and the writer-lock helpers.
//! * [`guard`] — the read/write guards handed to repositories.
//! * [`dry_run`] — thread-local pinning of the dry-run connection.
//! * [`handle`] — the [`Db`] trait and [`DbHandle`] connection source.
//! * [`database`] — the [`Database`] lifecycle (open, handle, dry_run, drop).
//!
//! The crate-internal surface used by the rest of `sqlite` (the engine, repositories) is re-exported
//! below: [`Database`], [`DbHandle`], [`Db`], and [`DatabaseConfig`]. [`Synchronous`] is re-exported
//! publicly by the parent `sqlite` module.

pub(crate) mod sync;

mod config;
mod database;
mod dry_run;
mod guard;
mod handle;
mod pool;
mod pragmas;

pub use config::DatabaseConfig;
pub use pragmas::Synchronous;

pub(crate) use database::Database;
pub(crate) use handle::{Db, DbHandle};
