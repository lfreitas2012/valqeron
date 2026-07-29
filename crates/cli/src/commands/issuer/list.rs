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
//! TODO(core): switch to a `find_all`/paged query once the core repository
//! provides one, and make `--id` an optional filter.

use std::str::FromStr;

use clap::Args;
use serde_json::{Value, json};
use uuid::Uuid;

use valqeron_core::{IssuerId, IssuerRepository};

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::IssuerView;
use crate::error::{AppError, AppResult};

/// Arguments for `issuer list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Issuer id (UUID) to look up. Required until the core supports full
    /// enumeration.
    #[arg(long)]
    pub id: String,
}

impl Command for ListArgs {
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadOnly
    }

    fn execute(&self, repo: &dyn IssuerRepository, _ctx: &AppContext) -> AppResult<Value> {
        let uuid = Uuid::from_str(&self.id).map_err(|e| AppError::InvalidId(e.to_string()))?;
        let id = IssuerId::from_uuid(uuid);

        let found = repo.find_by_id(&id)?;
        let items: Vec<IssuerView> = found.iter().map(IssuerView::from).collect();

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.get",
            id = %self.id,
            found = !items.is_empty(),
            "issuer lookup"
        );

        Ok(json!({
            "items": items,
            "count": items.len(),
        }))
    }
}
