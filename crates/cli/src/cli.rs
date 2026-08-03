use std::path::PathBuf;

use clap::Parser;
use tracing::Level;

use crate::commands::Commands;

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

    #[arg(
        short,
        long,
        global = true,
        value_name = "FILE",
        help = "Write JSON output to FILE instead of stdout."
    )]
    pub output: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        value_name = "FILE",
        help = "Read a JSON input from FILE."
    )]
    pub input: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "Rehearse against the real database, then roll back."
    )]
    pub dry_run: bool,

    #[arg(long, global = true, help = "Pretty-print JSON output.")]
    pub pretty: bool,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        env = "VALQERON_DB",
        help = "Database file path.",
        long_help = "Path to the SQLite database file. Overrides VALQERON_DB and the default."
    )]
    pub db_path: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Log file location.",
        long_help = "Path to the log file. Overrides VALQERON_LOG_FILE and the default.",
        default_missing_value = "",
        num_args = 0..=1
    )]
    pub log_file: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        conflicts_with = "log_file",
        help = "Disable logging to a file.",
        long_help = "Disable logging to a file. Also settable via VALQERON_LOG_FILE=off."
    )]
    pub no_log_file: bool,

    #[arg(
        long,
        global = true,
        value_name = "N",
        default_value_t = 4,
        help = "Number of concurrent reader connections in the engine's reader pool."
    )]
    pub reader_pool_size: usize,

    #[arg(
        long,
        global = true,
        help = "Use strict durability for writes (slower).",
        long_help = "Use strict durability for writes (slower). Writer use relaxed durability (default) \
        durability, which is faster, but the database may be corrupted if the power/OS crashes."
    )]
    pub durable: bool,
}

impl Cli {
    pub fn log_level(&self) -> Level {
        match self.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
            _ => Level::TRACE,
        }
    }

    pub fn log_file_arg(&self) -> Option<Option<PathBuf>> {
        match &self.log_file {
            None => None,
            Some(p) if p.as_os_str().is_empty() => Some(None),
            Some(p) => Some(Some(p.clone())),
        }
    }
}
