#![cfg_attr(
    test,
    allow(
        clippy::as_conversions,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::unwrap_used
    )
)]

mod cli;
mod commands;
mod config;
mod context;
mod io_util;

use crate::cli::Cli;
use crate::config::ValqeronConfig;
use crate::context::AppContext;
use crate::io_util::{InputSource, OutputDest};
use clap::Parser;
use non_blocking::WorkerGuard;
use std::io::IsTerminal;
use tracing_appender::non_blocking;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};
use valqeron_engine_client::{Client, ClientOptions};

fn main() {
    let cli = Cli::parse();

    let _log_guard = match run(&cli) {
        Ok(guard) => guard,
        Err(err) => {
            print_error(&err);
            std::process::exit(1);
        }
    };
}

fn run(cli: &Cli) -> anyhow::Result<Option<WorkerGuard>> {
    let config = ValqeronConfig::resolve(cli.log_file_arg(), cli.no_log_file)?;

    let guard = init_logging(cli, &config)?;

    if let Err(err) = dispatch(cli) {
        tracing::error!(
            target: "valqeron::audit",
            operation = "error",
            error = %err,
            "command failed"
        );
        return Err(err);
    }

    Ok(guard)
}

fn dispatch(cli: &Cli) -> anyhow::Result<()> {
    let pretty = cli.pretty || std::io::stdout().is_terminal();
    let output = OutputDest::from_arg(cli.output.as_deref());
    let input = cli.input.as_deref().map(InputSource::from_arg);
    let ctx = AppContext::new(output, input, cli.dry_run, pretty);

    let mut options = ClientOptions::default();
    if let Some(socket) = cli.socket.as_deref() {
        options = options.with_socket(socket);
    }

    let client = Client::connect(options)?;

    tracing::debug!(
        socket = %client.socket().display(),
        engine_version = %client.engine_info().engine_version,
        dry_run = cli.dry_run,
        "dispatching command"
    );

    let payload = cli.command.execute(&client, &ctx)?;

    ctx.write_success(&payload)
}

fn init_logging(cli: &Cli, config: &ValqeronConfig) -> anyhow::Result<Option<WorkerGuard>> {
    let stderr_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let mut filter = EnvFilter::new(cli.log_level().to_string());
        if let Ok(directive) = "valqeron::audit=off".parse() {
            filter = filter.add_directive(directive);
        }
        filter
    });

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .compact()
        .with_filter(stderr_filter);

    let file = config
        .log_file()
        .and_then(|path| match open_log_file(path) {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("warning: file logging disabled: {e}");
                None
            }
        });

    match file {
        Some((non_blocking, guard)) => {
            let file_filter = EnvFilter::new(config.file_log_level());

            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .json()
                .with_filter(file_filter);

            tracing_subscriber::registry()
                .with(stderr_layer)
                .with(file_layer)
                .init();
            Ok(Some(guard))
        }
        None => {
            tracing_subscriber::registry().with(stderr_layer).init();
            Ok(None)
        }
    }
}

fn open_log_file(
    path: &std::path::Path,
) -> std::io::Result<(non_blocking::NonBlocking, WorkerGuard)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(tracing_appender::non_blocking(file))
}

fn print_error(error: &anyhow::Error) {
    let envelope = serde_json::json!({
        "success": false,
        "error": {
            "message": error.to_string()
        }
    });

    let rendered = if std::io::stderr().is_terminal() {
        serde_json::to_string_pretty(&envelope)
    } else {
        serde_json::to_string(&envelope)
    }
    .unwrap_or_else(|_| String::from("{\"success\":false}"));

    eprintln!("{rendered}");
}
