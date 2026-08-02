//! Command definitions and dispatch.

pub mod init;
pub mod issuer;

use clap::Subcommand;
use serde_json::Value;

use crate::context::AppContext;
use crate::error::AppResult;
use crate::store::Repos;

/// Whether a command only reads persisted state or may mutate it.
///
/// This is advisory today: the core routes each repository call to the right
/// connection automatically. It informs logging and documents intent, and lets
/// the dispatcher keep read-only commands out of the write path conceptually.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// The command only issues reads and never mutates persisted state.
    ReadOnly,
    /// The command may mutate persisted state.
    ReadWrite,
}

/// A runnable command. Implementors receive the backend-agnostic [`Repos`] accessor and the app
/// context, pull the repository port(s) they need, and return the JSON payload to embed in the
/// success envelope.
pub trait Command {
    /// Execute against the given repositories, producing the result payload.
    fn execute(&self, repos: &Repos, ctx: &AppContext) -> AppResult<Value>;

    /// The command's access mode. Defaults to read-write.
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadWrite
    }
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize storage: create the database and apply schema migrations.
    Init(init::InitArgs),

    /// Manage issuers.
    #[command(subcommand)]
    Issuer(issuer::IssuerCommand),
}

impl Commands {
    /// The access mode of the selected command.
    pub fn access_mode(&self) -> AccessMode {
        match self {
            Commands::Init(_) => AccessMode::ReadWrite,
            Commands::Issuer(cmd) => cmd.as_command().access_mode(),
        }
    }

    /// Dispatch a repository-backed command. `init` is handled separately in
    /// `main` and must not reach here.
    pub fn execute(&self, repos: &Repos, ctx: &AppContext) -> AppResult<Value> {
        match self {
            Commands::Init(_) => {
                unreachable!("init is dispatched before opening a repository")
            }
            Commands::Issuer(cmd) => cmd.as_command().execute(repos, ctx),
        }
    }
}
