//! Command-line surface: the root parser and its global options.

use std::path::PathBuf;

use clap::Parser;
use tracing::Level;

use crate::commands::Commands;

/// Valqeron command-line interface.
#[derive(Parser, Debug)]
#[command(
    name = "valqeron",
    bin_name = "valqeron",
    author,
    version,
    about = "Valqeron CLI",
    long_about = "Valqeron CLI — manage issuers and storage on top of valqeron-core.\n\n\
                  Results are emitted as JSON on stdout (or --output FILE); logs and \
                  errors go to stderr.",
    alias = "vq",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Increase *stderr* log verbosity (-v=info, -vv=debug, -vvv=trace; default: warn).
    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::Count,
        long_help = "Increase stderr log verbosity:\n\
                     -v     info\n\
                     -vv    debug\n\
                     -vvv   trace\n\
                     (default: warn; RUST_LOG overrides)\n\n\
                     This affects stderr only. The log file (on by default) always\n\
                     records operations at info level (VALQERON_LOG_LEVEL overrides)."
    )]
    pub verbose: u8,

    /// Write JSON output to FILE instead of stdout.
    #[arg(short, long, global = true, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Read a JSON input document from FILE (use `-` for stdin).
    #[arg(short, long, global = true, value_name = "FILE")]
    pub input: Option<PathBuf>,

    /// Rehearse against the real database, then roll back (persists nothing).
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Pretty-print JSON output (default: on when stdout is a terminal).
    #[arg(long, global = true)]
    pub pretty: bool,

    /// Path to the SQLite database file (overrides VALQERON_DB and the default).
    #[arg(long, global = true, value_name = "PATH", env = "VALQERON_DB")]
    pub db_path: Option<PathBuf>,

    /// Log file location. File logging is on by default (per-binary logs dir);
    /// with no value uses the default location, pass a PATH to pin it.
    /// Overrides VALQERON_LOG_FILE.
    #[arg(long, global = true, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    pub log_file: Option<PathBuf>,

    /// Disable writing logs to a file (also settable via VALQERON_LOG_FILE=off).
    #[arg(long, global = true, conflicts_with = "log_file")]
    pub no_log_file: bool,

    /// Number of concurrent read connections in the engine's reader pool.
    #[arg(long, global = true, value_name = "N", default_value_t = 4)]
    pub reader_pool_size: usize,

    /// Use strict, power-loss-safe durability for writes (slower). By default,
    /// writes use relaxed durability: faster, and the database is never
    /// corrupted, but the most recent commit may be lost on a power/OS crash.
    #[arg(long, global = true)]
    pub durable: bool,
}

impl Cli {
    /// Map `-v` occurrences to a tracing [`Level`].
    pub fn log_level(&self) -> Level {
        match self.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
            _ => Level::TRACE,
        }
    }

    /// Interpret the `--log-file` argument into the tri-state the config layer
    /// expects: absent, present-without-path, or present-with-path.
    ///
    /// clap gives us `None` when the flag is absent, `Some("")` when it was
    /// passed bare (via `default_missing_value`), and `Some(path)` otherwise.
    pub fn log_file_arg(&self) -> Option<Option<PathBuf>> {
        match &self.log_file {
            None => None,
            Some(p) if p.as_os_str().is_empty() => Some(None),
            Some(p) => Some(Some(p.clone())),
        }
    }
}
