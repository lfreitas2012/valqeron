use std::str::FromStr;

use clap::Args;
use serde_json::{Value, json};
use uuid::Uuid;

use valqeron_core::{IssuerId, IssuerRepository};

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::IssuerView;
use crate::error::{AppError, AppResult};

#[derive(Args, Debug)]
pub struct InfoArgs {
    /// Issuer id (UUID).
    #[arg(long)]
    pub id: String,
}

impl Command for InfoArgs {
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
            operation = "issuer.info",
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
