//! Valqeron CLI entry point.
//!
//! Responsibilities, in order: parse args, resolve configuration, initialize
//! logging (stderr always; a file layer when requested), dispatch the command,
//! and render the result. Successful results are written as a JSON success
//! envelope to stdout (or `--output`); failures are rendered as an RFC 9457-style
//! [`ProblemDetail`](crate::error::problem::ProblemDetail) on stderr, and the
//! process exits with a category-specific `sysexits.h` code.

mod cli;
mod commands;
mod config;
mod context;
mod dto;
mod error;
mod io_util;
mod server;

use std::io::IsTerminal;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

use crate::cli::Cli;
use crate::commands::Commands;
use crate::config::ValqeronConfig;
use crate::context::AppContext;
use crate::error::AppResult;
use crate::error::problem::ProblemDetail;
use crate::io_util::{InputSource, OutputDest};
use crate::server::Server;

fn main() {
    let cli = Cli::parse();

    // Keep the file-appender worker guard alive for the whole run.
    let _log_guard = match run(&cli) {
        Ok(guard) => guard,
        Err(err) => {
            let problem = err.problem();
            print_problem(&problem);
            std::process::exit(problem.exit_code());
        }
    };
}

/// Execute the CLI. Returns the log worker guard (if a file layer was set up)
/// so the caller can keep it alive until the process exits.
fn run(cli: &Cli) -> AppResult<Option<tracing_appender::non_blocking::WorkerGuard>> {
    let config = ValqeronConfig::resolve(
        cli.db_path.clone(),
        cli.log_file_arg(),
        cli.no_log_file,
        cli.reader_pool_size,
    )?;

    let guard = init_logging(cli, &config)?;

    if let Err(err) = dispatch(cli, &config) {
        // Record the structured problem once (this is the file-audited error
        // record); the caller renders the JSON envelope to stderr and exits.
        let problem = err.problem();
        tracing::error!(
            target: "valqeron::audit",
            operation = "error",
            error.type = %problem.r#type,
            error.status = problem.status,
            "{}",
            problem.human_summary()
        );
        return Err(err);
    }

    Ok(guard)
}

/// Resolve the output/pretty settings and run the selected command.
fn dispatch(cli: &Cli, config: &ValqeronConfig) -> AppResult<()> {
    let pretty = cli.pretty || std::io::stdout().is_terminal();
    let output = OutputDest::from_arg(cli.output.as_deref());
    let input = cli.input.as_deref().map(InputSource::from_arg);
    let ctx = AppContext::new(output, input, cli.dry_run, pretty);

    // `init` opens the engine itself (to create/migrate); it does not run
    // against an already-open repository.
    if let Commands::Init(args) = &cli.command {
        args.run(config, &ctx)?;
        return Ok(());
    }

    let server = Server::open(config)?;

    tracing::debug!(
        access_mode = ?cli.command.access_mode(),
        dry_run = cli.dry_run,
        "dispatching command"
    );

    // Dry-run rehearses the real write path on an isolated, always-rolled-back
    // connection; otherwise we run against the live engine.
    let payload = if cli.dry_run {
        server.dry_run(|repo| cli.command.execute(repo, &ctx))?
    } else {
        server.with_issuers(|repo| cli.command.execute(repo, &ctx))?
    };

    ctx.write_success(&payload)
}

/// Initialize tracing with two independently-filtered layers:
///
/// * a **stderr** layer at the `-v`/`RUST_LOG` level (default WARN) for the
///   human at the terminal — never writes to stdout, so piped JSON stays clean;
/// * a **file** layer (JSON, non-blocking) at INFO/`VALQERON_LOG_LEVEL` so the
///   full operation trail is persisted regardless of `-v`.
///
/// File logging is best-effort: if the log directory or file cannot be opened
/// we warn on stderr and continue without it rather than failing the command.
fn init_logging(
    cli: &Cli,
    config: &ValqeronConfig,
) -> AppResult<Option<tracing_appender::non_blocking::WorkerGuard>> {
    // stderr: quiet by default, raised by -v; RUST_LOG overrides.
    //
    // By default we mute the `valqeron::audit` target on stderr so it does not
    // interleave with the machine-readable JSON envelope: audit records are the
    // file layer's job. When the user sets RUST_LOG explicitly we honor it as-is
    // (they've opted into full control of stderr).
    let stderr_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(cli.log_level().to_string())
            .add_directive("valqeron::audit=off".parse().expect("valid directive"))
    });

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .compact()
        .with_filter(stderr_filter);

    // Try to set up the file layer; on failure, warn and carry on stderr-only.
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
            // file: always captures operations at INFO+ (VALQERON_LOG_LEVEL / RUST_LOG override).
            let file_filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(config.file_log_level()));

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

/// Open (creating parent dirs) the append-mode log file and wrap it in a
/// non-blocking writer. Returns the writer plus its flush guard.
fn open_log_file(
    path: &std::path::Path,
) -> std::io::Result<(
    tracing_appender::non_blocking::NonBlocking,
    tracing_appender::non_blocking::WorkerGuard,
)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(tracing_appender::non_blocking(file))
}

/// Render a problem as the JSON error envelope on stderr. Pretty-prints when
/// stderr is a terminal.
///
/// This writes exactly one line to stderr and never touches stdout, so piped
/// JSON output stays clean. The structured error itself is logged separately
/// (see [`run`]), which is where it reaches the file layer — we do not also emit
/// a duplicate human line here.
fn print_problem(problem: &ProblemDetail) {
    let envelope = serde_json::json!({ "success": false, "error": problem });
    let rendered = if std::io::stderr().is_terminal() {
        serde_json::to_string_pretty(&envelope)
    } else {
        serde_json::to_string(&envelope)
    }
    .unwrap_or_else(|_| String::from("{\"success\":false}"));

    eprintln!("{rendered}");
}
