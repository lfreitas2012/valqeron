use crate::commands::Command;
use crate::context::AppContext;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use valqeron_engine_client::Client;

#[derive(Subcommand, Debug)]
pub enum EngineCommand {
    /// Check the engine's health (version handshake round-trip).
    Ping(PingArgs),

    /// Run engine diagnostics and return its status.
    Status(StatusArgs),

    /// Manages background tasks
    ListBackgroundTasks(ListBackgroundTasksArgs),
}

impl EngineCommand {
    pub fn as_command(&self) -> &dyn Command {
        match self {
            EngineCommand::Ping(args) => args,
            EngineCommand::Status(args) => args,
            EngineCommand::ListBackgroundTasks(args) => args,
        }
    }
}

#[derive(Args, Debug)]
pub struct PingArgs {}

impl Command for PingArgs {
    fn execute(&self, client: &Client, _ctx: &AppContext) -> anyhow::Result<Value> {
        let info = client.admin().health()?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine.ping",
            engine_version = %info.engine_version,
            "engine ping"
        );

        Ok(json!({
            "engine_version": info.engine_version,
            "protocol_version": info.protocol_version,
            "socket": client.socket().display().to_string(),
        }))
    }
}

#[derive(Args, Debug)]
pub struct StatusArgs {}

impl Command for StatusArgs {
    fn execute(&self, client: &Client, _ctx: &AppContext) -> anyhow::Result<Value> {
        let status = client.admin().status()?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine.status",
            engine_version = %status.engine_version,
            "engine status"
        );

        Ok(json!({
            "engine_version": status.engine_version,
            "protocol_version": status.protocol_version,
            "pid": status.pid,
            "socket": client.socket().display().to_string(),
        }))
    }
}

#[derive(Args, Debug)]
pub struct ListBackgroundTasksArgs {}

impl Command for ListBackgroundTasksArgs {
    fn execute(&self, client: &Client, _ctx: &AppContext) -> anyhow::Result<Value> {
        let tasks = client.admin().list_background_tasks()?;

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine.list_background_tasks",
            "list background tasks"
        );

        Ok(json!({
            "background_tasks": tasks
        }))
    }
}
