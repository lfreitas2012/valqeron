use clap::Args;
use serde_json::{Value, json};

use crate::config::ValqeronConfig;
use crate::context::AppContext;
use crate::error::AppResult;
use crate::store;

#[derive(Args, Debug)]
pub struct InitArgs {}

impl InitArgs {
    pub fn run(&self, config: &ValqeronConfig, ctx: &AppContext) -> AppResult<Value> {
        config.ensure_db_parent()?;

        let _store = store::open(config)?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "init",
            db_path = %config.db_path().display(),
            "initialized storage"
        );

        let payload = json!({
            "db_path": config.db_path().display().to_string(),
            "reader_pool_size": config.reader_pool_size(),
            "durability": config.durability_label(),
            "initialized": true,
        });

        ctx.write_success(&payload)?;
        Ok(payload)
    }
}
