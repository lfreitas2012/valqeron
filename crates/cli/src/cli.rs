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
        help = "Rehearse against the real database, then roll back.",
        long_help = "Rehearse against the real database, then roll back. The engine runs \
                     the command inside a savepoint that is always rolled back — nothing \
                     persists."
    )]
    pub dry_run: bool,

    #[arg(long, global = true, help = "Pretty-print JSON output.")]
    pub pretty: bool,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        env = "VALQERON_SOCKET",
        help = "Engine socket path.",
        long_help = "Path of the engine's Unix domain socket. Overrides VALQERON_SOCKET \
                     and the platform default. Must resolve to the same path the engine \
                     binds."
    )]
    pub socket: Option<PathBuf>,

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

    pub(crate) fn log_file_arg(&self) -> Option<Option<PathBuf>> {
        match &self.log_file {
            None => None,
            Some(p) if p.as_os_str().is_empty() => Some(None),
            Some(p) => Some(Some(p.clone())),
        }
    }
}
