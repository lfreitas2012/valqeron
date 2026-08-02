pub mod init;
pub mod issuer;

use clap::Subcommand;
use serde_json::Value;

use crate::context::AppContext;
use crate::error::{AppError, AppResult};
use crate::store::Repos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

pub trait Command {
    fn execute(&self, repos: &Repos, ctx: &AppContext) -> AppResult<Value>;

    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadWrite
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize storage: create the database and apply schema migrations.
    Init(init::InitArgs),

    /// Manages issuers.
    #[command(subcommand)]
    Issuer(issuer::IssuerCommand),
}

impl Commands {
    pub fn access_mode(&self) -> AccessMode {
        match self {
            Commands::Init(_) => AccessMode::ReadWrite,
            Commands::Issuer(cmd) => cmd.as_command().access_mode(),
        }
    }

    pub fn execute(&self, repos: &Repos, ctx: &AppContext) -> AppResult<Value> {
        match self {
            Commands::Init(_) => Err(AppError::Config(
                "init must be dispatched before opening a repository".into(),
            )),
            Commands::Issuer(cmd) => cmd.as_command().execute(repos, ctx),
        }
    }
}
