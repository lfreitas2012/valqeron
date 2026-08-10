pub mod engine;
pub mod issuer;

use crate::commands::issuer::IssuerCommand;
use crate::context::AppContext;
use clap::Subcommand;
use serde_json::Value;
use valqeron_client::Client;

/// A CLI command executed against a connected engine client.
pub trait Command {
    fn execute(&self, client: &Client, ctx: &AppContext) -> anyhow::Result<Value>;
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manages issuers.
    #[command(subcommand)]
    Issuer(IssuerCommand),

    /// Inspect the engine daemon (status, ping).
    #[command(subcommand)]
    Engine(engine::EngineCommand),
}

impl Commands {
    pub fn execute(&self, client: &Client, ctx: &AppContext) -> anyhow::Result<Value> {
        match self {
            Commands::Issuer(cmd) => cmd.as_command().execute(client, ctx),
            Commands::Engine(cmd) => cmd.as_command().execute(client, ctx),
        }
    }
}
