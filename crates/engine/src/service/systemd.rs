// //! systemd user unit management (Linux).
// //!
// //! The engine installs as a **user unit** (`~/.config/systemd/user`), not a
// //! system service: the database is user-scoped, so the service is
// //! user-bounded and starts at login.
// //!
// //! Note: user units stop at logout and only start at login. For boot-time
// //! start without a session, enable lingering: `loginctl enable-linger $USER`.
// //!
// //! Restart policy: `Restart=on-failure` restarts crashes but leaves clean
// //! exits stopped; the start-limit settings prevent restart storms when
// //! startup keeps failing (e.g. the lock is held).
// 
// use std::path::PathBuf;
// 
// use crate::cli::{Cli, InstallArgs};
// use crate::error::{EngineError, EngineResult};
// use crate::runtime::{config_err, resolve_db_path, resolve_socket_path};
// use crate::service::render::{render, systemd_env_block};
// use crate::service::{Preflight, current_exe_path, engine_log_dir, preflight, run_tool};
// 
// const UNIT_NAME: &str = "valqeron-engine.service";
// 
// fn unit_path() -> EngineResult<PathBuf> {
//     let config_home = std::env::var_os("XDG_CONFIG_HOME")
//         .map(PathBuf::from)
//         .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
//         .ok_or_else(|| EngineError::Config("neither XDG_CONFIG_HOME nor HOME is set".into()))?;
//     Ok(config_home.join("systemd").join("user").join(UNIT_NAME))
// }
// 
// /// Location of the installed unit definition, for `status`.
// pub fn unit_file_path() -> EngineResult<PathBuf> {
//     unit_path()
// }
// 
// fn systemctl_ok(args: &[&str]) -> EngineResult<()> {
//     let output = run_tool("systemctl", args)?;
//     if !output.status.success() {
//         return Err(EngineError::Service(format!(
//             "systemctl {} failed: {}",
//             args.join(" "),
//             String::from_utf8_lossy(&output.stderr).trim()
//         )));
//     }
//     Ok(())
// }
// 
// /// Resolve every path and render the unit definition. Pure: no filesystem
// /// writes, no systemctl — shared by `install` and `install --print`.
// fn render_service(cli: &Cli) -> EngineResult<(PathBuf, String)> {
//     let exe = current_exe_path()?;
//     let log_dir = engine_log_dir()?;
// 
//     // The sandbox (ProtectSystem=strict / ProtectHome=read-only) must be
//     // punched through for exactly what the engine writes: the database
//     // directory (db + WAL + lock file), the gRPC socket directory, and its
//     // log directory.
//     let db_path = resolve_db_path(cli.db_path.clone()).map_err(config_err)?;
//     let db_dir = db_path
//         .parent()
//         .map(std::path::Path::to_path_buf)
//         .unwrap_or_else(|| PathBuf::from("."));
//     let socket_path = resolve_socket_path(cli)?;
//     let socket_dir = socket_path
//         .parent()
//         .map(std::path::Path::to_path_buf)
//         .unwrap_or_else(|| PathBuf::from("."));
//     let rw_paths = format!(
//         "\"{}\" \"{}\" \"{}\"",
//         db_dir.display(),
//         socket_dir.display(),
//         log_dir.display()
//     );
// 
//     let env_block = systemd_env_block(cli.db_path.as_deref(), cli.socket.as_deref());
// 
//     let rendered = render(
//         crate::service::systemd_template(),
//         &[
//             ("EXE", &exe.display().to_string()),
//             ("RW_PATHS", &rw_paths),
//             ("ENV_BLOCK", &env_block),
//         ],
//     )?;
//     Ok((unit_path()?, rendered))
// }
// 
// pub fn install(cli: &Cli, args: &InstallArgs) -> EngineResult<()> {
//     let (unit, rendered) = render_service(cli)?;
// 
//     if args.print {
//         eprintln!(
//             "# systemd user unit definition; install target: {}",
//             unit.display()
//         );
//         print!("{rendered}");
//         return Ok(());
//     }
// 
//     if preflight(&unit, &rendered, args.force)? == Preflight::UpToDate {
//         println!("service definition already up to date: {}", unit.display());
//         if args.no_start {
//             println!("left untouched (--no-start)");
//         } else {
//             // `enable --now` is idempotent: it starts a stopped unit and
//             // leaves a running one untouched (no restart).
//             systemctl_ok(&["--user", "enable", "--now", UNIT_NAME])?;
//             println!("unit enabled ({UNIT_NAME})");
//         }
//         return Ok(());
//     }
// 
//     let log_dir = engine_log_dir()?;
//     std::fs::create_dir_all(&log_dir)
//         .map_err(|e| EngineError::Io(format!("creating {}: {e}", log_dir.display())))?;
// 
//     if let Some(parent) = unit.parent() {
//         std::fs::create_dir_all(parent)
//             .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
//     }
//     std::fs::write(&unit, &rendered)
//         .map_err(|e| EngineError::Io(format!("writing {}: {e}", unit.display())))?;
// 
//     systemctl_ok(&["--user", "daemon-reload"])?;
// 
//     if args.no_start {
//         systemctl_ok(&["--user", "enable", UNIT_NAME])?;
//         println!("installed systemd user unit: {}", unit.display());
//         println!("not started (--no-start); the engine starts at the next login");
//         println!("a currently running instance keeps its previous definition until restarted");
//         println!("start now with:  systemctl --user start {UNIT_NAME}");
//         return Ok(());
//     }
// 
//     systemctl_ok(&["--user", "enable", "--now", UNIT_NAME])?;
// 
//     println!("installed systemd user unit: {}", unit.display());
//     println!("the engine starts now and at every login");
//     println!("for boot start without a session: loginctl enable-linger $USER");
//     println!("inspect with:  systemctl --user status {UNIT_NAME}");
//     println!("logs:          journalctl --user -u {UNIT_NAME}");
//     Ok(())
// }
// 
// pub fn uninstall() -> EngineResult<()> {
//     // Best-effort: the unit may not be enabled or even installed.
//     let _ = run_tool("systemctl", &["--user", "disable", "--now", UNIT_NAME]);
// 
//     let unit = unit_path()?;
//     match std::fs::remove_file(&unit) {
//         Ok(()) => println!("removed {}", unit.display()),
//         Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
//             println!("nothing to remove: {} does not exist", unit.display());
//         }
//         Err(e) => {
//             return Err(EngineError::Io(format!("removing {}: {e}", unit.display())));
//         }
//     }
//     let _ = run_tool("systemctl", &["--user", "daemon-reload"]);
//     println!("engine service uninstalled");
//     Ok(())
// }
// 
// /// Human-readable service registration state for `status`.
// pub fn service_state() -> EngineResult<String> {
//     let output = run_tool("systemctl", &["--user", "is-active", UNIT_NAME])?;
//     let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
//     if state.is_empty() {
//         Ok("unknown".to_string())
//     } else {
//         Ok(format!("{state} ({UNIT_NAME})"))
//     }
// }
// 
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use clap::Parser;
// 
//     fn cli(args: &[&str]) -> Cli {
//         let mut argv = vec!["valqeron-engine"];
//         argv.extend_from_slice(args);
//         argv.push("install");
//         Cli::try_parse_from(argv).unwrap()
//     }
// 
//     #[test]
//     fn rendered_unit_points_at_this_binary_and_punches_the_sandbox() {
//         let (path, rendered) = render_service(&cli(&[
//             "--db-path",
//             "/data/v.db",
//             "--socket",
//             "/run/v.sock",
//         ]))
//         .unwrap();
//         assert!(path.ends_with(UNIT_NAME));
//         let exe = current_exe_path().unwrap();
//         assert!(rendered.contains(&exe.display().to_string()));
//         // Sandbox punch-through must cover the db and socket directories.
//         assert!(rendered.contains("\"/data\""), "{rendered}");
//         assert!(rendered.contains("\"/run\""), "{rendered}");
//         // Overrides propagate into the unit environment.
//         assert!(rendered.contains("Environment=\"VALQERON_DB=/data/v.db\""));
//         assert!(rendered.contains("Environment=\"VALQERON_SOCKET=/run/v.sock\""));
//         assert!(rendered.contains("Type=notify"));
//     }
// 
//     #[test]
//     fn rendering_is_deterministic() {
//         let a = render_service(&cli(&["--db-path", "/data/v.db"])).unwrap();
//         let b = render_service(&cli(&["--db-path", "/data/v.db"])).unwrap();
//         assert_eq!(a, b, "identical config must render identically");
//     }
// }
