use crate::commands::Command;
use crate::context::AppContext;
use anyhow::Context;
use clap::{Args, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};
use std::str::FromStr;
use uuid::Uuid;
use valqeron_core::BackgroundTask;
use valqeron_core::common::{UniqueIdentifier, Versioned};
use valqeron_engine_client::{BackgroundTaskDetail, Client};

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

/// Default page size when `--limit` is not supplied.
const DEFAULT_LIMIT: u32 = 50;

/// Maximum allowable page size to prevent memory exhaustion.
const MAX_LIMIT: u32 = 1000;

#[derive(Args, Debug)]
pub struct ListBackgroundTasksArgs {
    #[arg(
        long,
        default_value_t = DEFAULT_LIMIT,
        value_parser = clap::value_parser!(u32).range(1..=i64::from(MAX_LIMIT)),
        help = &format!("Maximum number of background tasks to return in this page (default: {})", DEFAULT_LIMIT)
    )]
    pub limit: u32,

    #[arg(
        long,
        help = "Return background tasks whose id sorts after this one (UUID). Use the last id of the previous page to fetch the next page."
    )]
    pub after: Option<String>,
}

impl Command for ListBackgroundTasksArgs {
    fn execute(&self, client: &Client, _ctx: &AppContext) -> anyhow::Result<Value> {
        let after: Option<UniqueIdentifier> = match &self.after {
            Some(raw) => {
                let uuid = Uuid::from_str(raw).with_context(|| {
                    format!("invalid background tasks pagination cursor: {raw}")
                })?;
                Some(UniqueIdentifier::from_uuid(uuid))
            }
            None => None,
        };

        let found = client
            .admin()
            .list_background_tasks(after.as_ref(), self.limit)?;

        let items: Vec<BackgroundTaskDefinitionView> = found
            .iter()
            .map(BackgroundTaskDefinitionView::from)
            .collect();

        tracing::info!(
            target: "valqeron::audit",
            operation = "engine.list_background_tasks",
            "list background tasks"
        );

        Ok(json!({
            "background_tasks": items
        }))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundTaskDefinitionView {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_updated_at: String,
    pub version: u32,
}

impl BackgroundTaskDefinitionView {
    pub fn new(task: &BackgroundTask, version: u32) -> Self {
        Self {
            id: task.id().value(),
            name: task.name().as_str().to_string(),
            created_at: task.created_at().to_rfc3339(),
            last_updated_at: task.last_updated_at().to_rfc3339(),
            version,
        }
    }
}

impl From<&BackgroundTaskDetail> for BackgroundTaskDefinitionView {
    fn from(value: &BackgroundTaskDetail) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name.clone(),
            created_at: value.created_at.to_rfc3339(),
            last_updated_at: value.last_updated_at.to_rfc3339(),
            version: 1,
        }
    }
}

impl From<&Versioned<BackgroundTask>> for BackgroundTaskDefinitionView {
    fn from(value: &Versioned<BackgroundTask>) -> Self {
        BackgroundTaskDefinitionView::new(&value.data, value.version)
    }
}
