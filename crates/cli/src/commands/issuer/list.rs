//! `valqeron issuer list` — list issuers.
//!
//! # Current scope
//!
//! `valqeron-core`'s `IssuerRepository` exposes lookups (`find_by_id`,
//! `exists`) but **no** bulk listing (`find_all`) yet. Until the core grows one,
//! this command lists by looking up a single issuer via `--id` and returning a
//! JSON array of 0 or 1 items — the shape is already list-friendly, so adding
//! full enumeration later is backwards compatible.
//!
//!
//! provides one, and make `--id` an optional filter.

use clap::Args;
use serde_json::{Value, json};

use valqeron_core::IssuerRepository;

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::IssuerView;
use crate::error::AppResult;

/// Arguments for `issuer list`.
#[derive(Args, Debug)]
pub struct ListArgs {}

// TODO(core): switch to a `find_all`/paged query once the core repository
impl Command for ListArgs {
    fn execute(&self, repo: &dyn IssuerRepository, _ctx: &AppContext) -> AppResult<Value> {
        let found = repo.list_all()?;
        let items: Vec<IssuerView> = found.iter().map(IssuerView::from).collect();

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.list",
            found = !items.is_empty(),
            "list all registered issuers"
        );

        Ok(json!({
            "items": items,
            "count": items.len(),
        }))
    }

    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadOnly
    }
}
