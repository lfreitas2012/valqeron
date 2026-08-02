use clap::Args;
use serde_json::Value;

use valqeron_core::register_issuer;

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::{IssuerInput, IssuerView};
use crate::error::{AppError, AppResult};
use crate::store::Repos;

#[derive(Args, Debug)]
pub struct RegisterArgs {
    #[arg(long, short = 'n', help = "Human-readable issuer name (required)")]
    pub name: Option<String>,

    #[arg(
        long,
        short = 's',
        help = "Issuer status (ACTIVE or RETIRED)",
        default_value = "ACTIVE"
    )]
    pub status: Option<String>,

    #[arg(long, help = "Brazilian CNPJ (punctuated or compact)")]
    pub cnpj: Option<String>,

    #[arg(long, help = "ISO 17442 LEI (punctuated or compact)")]
    pub lei: Option<String>,

    #[arg(long, help = "Country code (2-letter ISO 3166-1 alpha-2)")]
    pub country_code: Option<String>,
}

impl Command for RegisterArgs {
    fn execute(&self, repos: &Repos, ctx: &AppContext) -> AppResult<Value> {
        let repo = repos.issuers();

        let base: IssuerInput = match ctx.read_input()? {
            Some(value) => {
                serde_json::from_value(value).map_err(|e| AppError::Input(e.to_string()))?
            }
            None => IssuerInput::default(),
        };

        let input = base.merge_flags(
            self.name.clone(),
            self.status.clone(),
            self.cnpj.clone(),
            self.lei.clone(),
            self.country_code.clone(),
        );

        let issuer = input.into_issuer()?;

        register_issuer(repo, &issuer)?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.register",
            id = %issuer.id().value(),
            dry_run = ctx.dry_run(),
            "registered issuer"
        );

        let view = IssuerView::new(&issuer, 1);
        let payload = serde_json::to_value(view).map_err(|e| AppError::Serialize(e.to_string()))?;
        Ok(payload)
    }

    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadWrite
    }
}
