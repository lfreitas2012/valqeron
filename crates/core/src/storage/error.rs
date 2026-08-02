//! Errors produced by the storage boundary.
//!
//! This module defines the domain's driver-independent vocabulary for failures in the persistence
//! layer. Storage drivers translate their native errors into [`StorageFault`] values before those
//! errors cross into the domain or application layers. The concrete driver type therefore remains
//! an implementation detail of the infrastructure layer.
//!
//! [`StorageFault`] deliberately preserves the wrapped error as its source. Callers can propagate
//! the fault without inspecting it, while the application boundary can still traverse the source
//! chain when it records diagnostics. Expected persistence outcomes—such as a missing record or a
//! failed optimistic-lock check—are returned as ordinary values by repository APIs and are not
//! represented by these errors.

use std::error::Error as StdError;

/// An opaque failure originating below the domain boundary.
///
/// A storage driver uses this type to translate failures such as I/O errors, a corrupt database,
/// or a driver-level error into the driver-independent error vocabulary exposed by the domain.
/// The domain does not interpret the fault; it only propagates it.
///
/// The wrapped source is retained, so application code can inspect the error chain for logging and
/// diagnostics. The source must be [`Send`] and [`Sync`] so that a fault can safely cross common
/// thread and async-task boundaries without exposing a concrete driver error in the public API.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct StorageFault(Box<dyn StdError + Send + Sync>);

impl StorageFault {
    /// Wrap a storage-driver error as an opaque storage fault.
    ///
    /// The original error is kept as the fault's source and remains available to error-reporting
    /// code through the standard error-chain APIs.
    pub fn new(source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

/// A failure while managing the storage engine itself.
///
/// This error is used for store-level operations such as opening the engine or starting and
/// completing a dry-run transaction. Failures from an individual repository operation are exposed
/// as [`StorageFault`] through the repository result type instead.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The storage engine could not be reached or opened.
    ///
    /// The message describes the availability failure because no driver-independent source error
    /// is available at this layer.
    #[error("storage is unavailable: {0}")]
    Unavailable(String),

    /// An underlying storage operation failed.
    #[error(transparent)]
    Fault(#[from] StorageFault),
}
