//! `valqeron init` — create the database and apply schema migrations.

use clap::Args;
use serde_json::{Value, json};

use crate::config::ValqeronConfig;
use crate::context::AppContext;
use crate::error::AppResult;
use crate::server::Server;

/// Arguments for `init`. The database path and pool size come from the global
/// options, so this command currently takes no positional arguments.
#[derive(Args, Debug)]
pub struct InitArgs {}

impl InitArgs {
    /// Ensure the data directory exists, open the engine (which runs any
    /// pending migrations), and report the resolved location.
    ///
    /// Opening the engine is enough to create and migrate the database; there
    /// is no separate write, so `--dry-run` has nothing to roll back here.
    pub fn run(&self, config: &ValqeronConfig, ctx: &AppContext) -> AppResult<Value> {
        config.ensure_db_parent()?;

        // Opening applies migrations. We immediately drop the engine; the file
        // now exists at the latest schema version.
        let _server = Server::open(config)?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "init",
            db_path = %config.db_path().display(),
            "initialized storage"
        );

        let payload = json!({
            "db_path": config.db_path().display().to_string(),
            "reader_pool_size": config.reader_pool_size(),
            "initialized": true,
        });

        ctx.write_success(&payload)?;
        Ok(payload)
    }
}
