//! `valqeron issuer register` — validate and persist a new issuer.

use clap::Args;
use serde_json::Value;

use valqeron_core::IssuerRepository;

use crate::commands::{AccessMode, Command};
use crate::context::AppContext;
use crate::dto::{IssuerInput, IssuerView};
use crate::error::{AppError, AppResult};

/// Arguments for `issuer register`.
///
/// Fields may be supplied as flags and/or via a JSON document on `--input`
/// (or stdin with `-`). Flags take precedence over the document.
#[derive(Args, Debug)]
pub struct RegisterArgs {
    /// Human-readable issuer name.
    #[arg(long)]
    pub name: Option<String>,

    /// Issuer status: ACTIVE or RETIRED (default: ACTIVE).
    #[arg(long)]
    pub status: Option<String>,

    /// Brazilian CNPJ (punctuated or compact).
    #[arg(long)]
    pub cnpj: Option<String>,

    /// Legal Entity Identifier (ISO 17442).
    #[arg(long)]
    pub lei: Option<String>,

    /// ISO 3166-1 alpha-2 country code.
    #[arg(long)]
    pub country_code: Option<String>,
}

impl Command for RegisterArgs {
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadWrite
    }

    fn execute(&self, repo: &dyn IssuerRepository, ctx: &AppContext) -> AppResult<Value> {
        // Start from the optional --input document, then overlay flags.
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
        repo.insert(&issuer)?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "issuer.register",
            id = %issuer.id().value(),
            dry_run = ctx.dry_run(),
            "registered issuer"
        );

        // A freshly inserted issuer is at version 1.
        let view = IssuerView::new(&issuer, 1);
        let payload = serde_json::to_value(view).map_err(|e| AppError::Serialize(e.to_string()))?;
        Ok(payload)
    }
}
