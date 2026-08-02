//! Issuer persistence for the SQLite backend.
//!
//! Everything specific to storing and querying issuers lives here, grouped so each entity owns its
//! own repository, SQL, row model, and column converters:
//!
//! * [`repository`] — [`SqliteIssuerRepository`], the `IssuerRepository` port implementation.
//! * [`queries`] — the raw SQL statements.
//! * [`model`] — the [`IssuerRow`](model::IssuerRow) row model.
//! * [`mapping`] — issuer-specific column converters (building on [`crate::sqlite::row`]).
//!
//! Future entities get sibling folders (`sqlite/<entity>/`) with the same shape.

mod mapping;
mod model;
mod queries;
mod repository;

pub(crate) use repository::SqliteIssuerRepository;
