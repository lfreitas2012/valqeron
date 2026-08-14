// //! launchd user agent management (macOS).
// //!
// //! The engine installs as a **LaunchAgent** (`~/Library/LaunchAgents`), not a
// //! system daemon: the database is user-scoped, so the service is
// //! user-bounded and starts at login.
// //!
// //! Restart policy: `KeepAlive = { SuccessfulExit = false }` restarts crashes
// //! but leaves clean exits stopped; `ThrottleInterval` prevents restart
// //! storms when startup keeps failing (e.g. the lock is held).
//
// use std::path::{Path, PathBuf};
//
// use crate::cli::{Cli, InstallArgs};
// use crate::engine::EngineResult;
// use crate::error::{EngineError, EngineResult};
// use crate::service::render::{launchd_env_block, render, xml_escape};
// use crate::service::{LABEL, Preflight, current_exe_path, engine_log_dir, preflight, run_tool};
//
// fn plist_path() -> EngineResult<PathBuf> {
//     let home = std::env::var_os("HOME")
//         .map(PathBuf::from)
//         .ok_or_else(|| EngineError::Config("HOME is not set".to_string()))?;
//     Ok(home
//         .join("Library")
//         .join("LaunchAgents")
//         .join(format!("{LABEL}.plist")))
// }
//
// fn gui_domain() -> EngineResult<String> {
//     let output = run_tool("id", &["-u"])?;
//     let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
//     if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
//         return Err(EngineError::Service(format!(
//             "could not determine the current uid (got {uid:?})"
//         )));
//     }
//     Ok(format!("gui/{uid}"))
// }
//
// /// Resolve every path and render the agent definition. Pure: no filesystem
// /// writes, no launchctl — shared by `install` and `install --print`.
// fn render_service(cli: &Cli) -> EngineResult<(PathBuf, String)> {
//     let exe = current_exe_path()?;
//     let log_dir = engine_log_dir()?;
//     let stdout_path = log_dir.join("launchd.stdout.log");
//     let stderr_path = log_dir.join("launchd.stderr.log");
//     let env_block = launchd_env_block(cli.db_path.as_deref(), cli.socket.as_deref());
//
//     let rendered = render(
//         crate::service::launchd_template(),
//         &[
//             ("LABEL", LABEL),
//             ("EXE", &xml_escape(&exe.display().to_string())),
//             (
//                 "STDOUT_PATH",
//                 &xml_escape(&stdout_path.display().to_string()),
//             ),
//             (
//                 "STDERR_PATH",
//                 &xml_escape(&stderr_path.display().to_string()),
//             ),
//             ("ENV_BLOCK", &env_block),
//         ],
//     )?;
//     Ok((plist_path()?, rendered))
// }
//
// /// Location of the installed agent definition, for `status`.
// pub fn unit_file_path() -> EngineResult<PathBuf> {
//     plist_path()
// }
//
// fn is_loaded(domain: &str) -> bool {
//     run_tool("launchctl", &["print", &format!("{domain}/{LABEL}")])
//         .map(|output| output.status.success())
//         .unwrap_or(false)
// }
//
// fn bootstrap_agent(domain: &str, plist: &Path) -> EngineResult<()> {
//     let plist_str = plist.display().to_string();
//     let output = run_tool("launchctl", &["bootstrap", domain, &plist_str])?;
//     if !output.status.success() {
//         return Err(EngineError::Service(format!(
//             "launchctl bootstrap failed: {}",
//             String::from_utf8_lossy(&output.stderr).trim()
//         )));
//     }
//     Ok(())
// }
//
// pub fn install(cli: &Cli, args: &InstallArgs) -> EngineResult<()> {
//     let (plist, rendered) = render_service(cli)?;
//
//     if args.print {
//         eprintln!(
//             "# launchd agent definition; install target: {}",
//             plist.display()
//         );
//         print!("{rendered}");
//         return Ok(());
//     }
//
//     let domain = gui_domain()?;
//
//     if preflight(&plist, &rendered, args.force)? == Preflight::UpToDate {
//         println!("service definition already up to date: {}", plist.display());
//         if args.no_start {
//             println!("left untouched (--no-start)");
//         } else if is_loaded(&domain) {
//             println!("agent already loaded ({domain}/{LABEL})");
//         } else {
//             bootstrap_agent(&domain, &plist)?;
//             println!("agent loaded ({domain}/{LABEL})");
//         }
//         return Ok(());
//     }
//
//     // The agent definition captures stdout/stderr into the log directory;
//     // make sure it exists before launchd needs it.
//     let log_dir = engine_log_dir()?;
//     std::fs::create_dir_all(&log_dir)
//         .map_err(|e| EngineError::Io(format!("creating {}: {e}", log_dir.display())))?;
//
//     if let Some(parent) = plist.parent() {
//         std::fs::create_dir_all(parent)
//             .map_err(|e| EngineError::Io(format!("creating {}: {e}", parent.display())))?;
//     }
//     std::fs::write(&plist, &rendered)
//         .map_err(|e| EngineError::Io(format!("writing {}: {e}", plist.display())))?;
//
//     if args.no_start {
//         println!("installed launchd agent: {}", plist.display());
//         println!("not started (--no-start); the engine starts at the next login");
//         println!("a currently running instance keeps its previous definition until restarted");
//         return Ok(());
//     }
//
//     // Best-effort unload of a previous registration, then load the new one.
//     let _ = run_tool("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
//     bootstrap_agent(&domain, &plist)?;
//
//     println!("installed launchd agent: {}", plist.display());
//     println!("the engine starts now and at every login");
//     println!("inspect with:  launchctl print {domain}/{LABEL}");
//     println!(
//         "logs:          {}",
//         engine_log_dir()?.join("launchd.stderr.log").display()
//     );
//     Ok(())
// }
//
// pub fn uninstall() -> EngineResult<()> {
//     let domain = gui_domain()?;
//     // Best-effort: the agent may not be loaded (e.g., after a manual bootout).
//     let _ = run_tool("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
//
//     let plist = plist_path()?;
//     match std::fs::remove_file(&plist) {
//         Ok(()) => println!("removed {}", plist.display()),
//         Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
//             println!("nothing to remove: {} does not exist", plist.display());
//         }
//         Err(e) => {
//             return Err(EngineError::Io(format!(
//                 "removing {}: {e}",
//                 plist.display()
//             )));
//         }
//     }
//     println!("engine service uninstalled");
//     Ok(())
// }
//
// /// Human-readable service registration state for `status`.
// pub fn service_state() -> EngineResult<String> {
//     let domain = gui_domain()?;
//     if is_loaded(&domain) {
//         Ok(format!("loaded ({domain}/{LABEL})"))
//     } else {
//         Ok(format!("not loaded ({domain}/{LABEL})"))
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
//     fn rendered_agent_points_at_this_binary_and_runs() {
//         let (path, rendered) = render_service(&cli(&[])).unwrap();
//         assert!(path.ends_with(format!("{LABEL}.plist")));
//         let exe = current_exe_path().unwrap();
//         assert!(rendered.contains(&xml_escape(&exe.display().to_string())));
//         assert!(rendered.contains("<string>run</string>"));
//         // The env block only appears for explicit overrides; the calling
//         // shell may leak the env vars, so only assert when it is clean.
//         if std::env::var_os("VALQERON_DB").is_none()
//             && std::env::var_os("VALQERON_SOCKET").is_none()
//         {
//             assert!(!rendered.contains("EnvironmentVariables"));
//         }
//     }
//
//     #[test]
//     fn overrides_are_propagated_into_the_agent_environment() {
//         let (_, rendered) = render_service(&cli(&[
//             "--db-path",
//             "/tmp/custom.db",
//             "--socket",
//             "/tmp/custom.sock",
//         ]))
//         .unwrap();
//         assert!(rendered.contains("<key>VALQERON_DB</key>"));
//         assert!(rendered.contains("/tmp/custom.db"));
//         assert!(rendered.contains("<key>VALQERON_SOCKET</key>"));
//         assert!(rendered.contains("/tmp/custom.sock"));
//     }
//
//     #[test]
//     fn rendering_is_deterministic() {
//         let a = render_service(&cli(&[])).unwrap();
//         let b = render_service(&cli(&[])).unwrap();
//         assert_eq!(a, b, "identical config must render identically");
//     }
// }
