#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

//! `valqeron-engine` — the daemon that owns the Valqeron database.
//!
//! The engine holds the SQLite file exclusively behind a single-instance
//! lock, runs migrations at startup, serves the gRPC API over a Unix domain
//! socket (`IssuerService` + `AdminService`), performs periodic maintenance
//! (`PRAGMA optimize` + passive WAL checkpoints), and shuts down gracefully.
//! Clients (`valqeron` CLI, future desktop app) reach the database only
//! through `valqeron-client` — no other process opens the file.
//!
//! Async containment: the gRPC edge runs on a `multi_thread` tokio runtime,
//! but every storage call crosses to tokio's blocking pool through the
//! bounded [`storage::AsyncStorage`] facade. `valqeron-core` and
//! `valqeron-infrastructure` stay fully synchronous and async-free.

mod cli;
mod engine;
mod error;
mod grpc;
mod jobs;
mod notify;
mod service;
mod storage;

use crate::cli::Cli;
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    // if let Err(err) = dispatch(&cli) {
    //     eprintln!("error: {err}");
    //     std::process::exit(err.exit_code());
    // }
}
//
// fn dispatch(cli: &Cli) -> EngineResult<()> {
//     match &cli.command {
//         Command::Run(args) => {
//             let config = EngineConfig::resolve(cli, args)?;
//             let _log_guard = logging::init(cli.log_level(), config.log_file());
//             runtime::run(&config)
//         }
//         Command::Install(args) => service::install(cli, args),
//         Command::Uninstall => service::uninstall(),
//         Command::Status(args) => service::status(cli, args),
//     }
// }
