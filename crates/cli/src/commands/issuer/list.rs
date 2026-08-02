use std::str::FromStr;

use clap::Args;
use serde_json::{Value, json};
use uuid::Uuid;

use valqeron_core::IssuerId;

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::IssuerView;
use crate::error::{AppError, AppResult};
use crate::store::Repos;

/// Default page size when `--limit` is not supplied.
const DEFAULT_LIMIT: u32 = 50;

/// Maximum allowable page size to prevent memory exhaustion.
const MAX_LIMIT: u32 = 1000;

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(
        long,
        default_value_t = DEFAULT_LIMIT,
        value_parser = clap::value_parser!(u32).range(1..=i64::from(MAX_LIMIT)),
        help = &format!("Maximum number of issuers to return in this page (default: {})", DEFAULT_LIMIT)
    )]
    pub limit: u32,

    #[arg(
        long,
        help = "Return issuers whose id sorts after this one (UUID). Use the last id of the previous page to fetch the next page."
    )]
    pub after: Option<String>,
}

impl Command for ListArgs {
    fn execute(&self, repos: &Repos, _ctx: &AppContext) -> AppResult<Value> {
        let repo = repos.issuers();

        let after: Option<IssuerId> = match &self.after {
            Some(raw) => {
                let uuid = Uuid::from_str(raw).map_err(|e| AppError::InvalidId(e.to_string()))?;
                Some(IssuerId::from_uuid(uuid))
            }
            None => None,
        };

        let found = repo.list_paged(after, self.limit)?;
        let items: Vec<IssuerView> = found.iter().map(IssuerView::from).collect();

        // Cursor for the next page: the last id in this page, if any.
        let next_after: Option<String> = found.last().map(|v| v.data.id().value());

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.list",
            count = items.len(),
            limit = self.limit,
            "list registered issuers (paged)"
        );

        Ok(json!({
            "items": items,
            "count": items.len(),
            "next_after": next_after,
        }))
    }

    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadOnly
    }
}
