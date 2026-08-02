//! Backend-agnostic storage errors.
//!
//! These types are the domain's vocabulary for "something in the persistence layer failed". They
//! are deliberately opaque: a driver (SQLite, and future backends) maps its own native error into a
//! [`StorageFault`], preserving the source chain for the application layer to log, without leaking
//! any driver type into the domain contract.
//!
//! Note what is *not* here: `NotFound`, `Conflict`, version mismatches, or uniqueness violations.
//! Those are domain outcomes, not faults — they are modelled as values ([`crate::WriteOutcome`],
//! `Option`, domain services) rather than errors, so they stay backend-neutral and branchable
//! without downcasting to a driver error.

use std::error::Error as StdError;

/// An opaque, source-preserving failure originating below the domain (I/O, a corrupt store, a
/// driver-level error, etc.).
///
/// The domain never interprets a fault; it only propagates it. The boxed source keeps the full
/// error chain (e.g. the underlying `rusqlite::Error`) available to the application layer for
/// logging and diagnostics.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct StorageFault(Box<dyn StdError + Send + Sync>);

impl StorageFault {
    /// Wrap a driver/backend error as an opaque storage fault.
    pub fn new(source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

/// A failure at the store level (opening the engine, running a dry-run transaction) as opposed to a
/// single repository operation.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The store could not be reached or opened.
    #[error("storage is unavailable: {0}")]
    Unavailable(String),

    /// An underlying storage fault occurred.
    #[error(transparent)]
    Fault(#[from] StorageFault),
}
