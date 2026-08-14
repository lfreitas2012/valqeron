// //! Service management: install/uninstall/status for the login service.
// //!
// //! Platform dispatch: launchd LaunchAgent on macOS, systemd user unit on
// //! Linux. Both are **user-bounded** services (the database is user-scoped),
// //! started at login — see the platform modules for the restart policies.
//
// mod render;
//
// #[cfg(target_os = "macos")]
// mod launchd;
// #[cfg(target_os = "linux")]
// mod systemd;
//
// #[cfg(target_os = "macos")]
// use launchd as platform;
// #[cfg(target_os = "linux")]
// use systemd as platform;
//
// use std::path::PathBuf;
//
// use crate::cli::{Cli, InstallArgs, StatusArgs};
// use crate::error::{EngineError, EngineResult};
// use crate::runtime::{
//     config_err, default_log_file, lock_path_for, resolve_db_path, resolve_socket_path,
// };
//
// /// launchd label / reverse-DNS identity of the engine service.
// pub const LABEL: &str = "io.valqeron.engine";
//
// /// Both templates are embedded on every platform (same pattern as the SQL
// /// migrations) so template tests run everywhere, not only on the target OS.
// /// On the non-native platform each getter is only reached from tests.
// #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
// pub fn launchd_template() -> &'static str {
//     include_str!("templates/io.valqeron.engine.plist")
// }
//
// #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
// pub fn systemd_template() -> &'static str {
//     include_str!("templates/valqeron-engine.service")
// }
//
// pub fn install(cli: &Cli, args: &InstallArgs) -> EngineResult<()> {
//     #[cfg(any(target_os = "macos", target_os = "linux"))]
//     {
//         platform::install(cli, args)
//     }
//     #[cfg(not(any(target_os = "macos", target_os = "linux")))]
//     {
//         let _ = (cli, args);
//         Err(EngineError::UnsupportedPlatform)
//     }
// }
//
// pub fn uninstall() -> EngineResult<()> {
//     #[cfg(any(target_os = "macos", target_os = "linux"))]
//     {
//         platform::uninstall()
//     }
//     #[cfg(not(any(target_os = "macos", target_os = "linux")))]
//     {
//         Err(EngineError::UnsupportedPlatform)
//     }
// }
//
// /// Report engine state and exit non-zero when the engine is not running,
// /// so scripts can use `valqeron-engine status` as a liveness probe.
// /// `--json` swaps the human report for one machine-readable object on
// /// stdout; the exit-code contract is identical.
// pub fn status(cli: &Cli, args: &StatusArgs) -> EngineResult<()> {
//     let db_path = resolve_db_path(cli.db_path.clone()).map_err(config_err)?;
//     let lock_path = lock_path_for(&db_path);
//     let socket_path = resolve_socket_path(cli)?;
//     let pid = crate::lockfile::read_lock_pid(&lock_path);
//     let alive = pid.as_deref().is_some_and(process_alive);
//
//     let service_state = {
//         #[cfg(any(target_os = "macos", target_os = "linux"))]
//         {
//             platform::service_state().unwrap_or_else(|e| format!("unknown ({e})"))
//         }
//         #[cfg(not(any(target_os = "macos", target_os = "linux")))]
//         {
//             "unavailable on this platform".to_string()
//         }
//     };
//     let unit_file: Option<PathBuf> = {
//         #[cfg(any(target_os = "macos", target_os = "linux"))]
//         {
//             platform::unit_file_path().ok()
//         }
//         #[cfg(not(any(target_os = "macos", target_os = "linux")))]
//         {
//             None
//         }
//     };
//
//     if args.json {
//         let report = serde_json::json!({
//             "database": db_path.display().to_string(),
//             "lock_file": lock_path.display().to_string(),
//             "socket": socket_path.display().to_string(),
//             "unit_file": unit_file.as_ref().map(|p| p.display().to_string()),
//             "service": service_state,
//             "engine": {
//                 "running": alive,
//                 "pid": pid.as_deref().and_then(|p| p.parse::<u32>().ok()),
//                 "stale_lock": pid.is_some() && !alive,
//             },
//         });
//         println!("{report}");
//         return if alive {
//             Ok(())
//         } else {
//             Err(EngineError::NotRunning)
//         };
//     }
//
//     println!("database:   {}", db_path.display());
//     println!("lock file:  {}", lock_path.display());
//     println!("socket:     {}", socket_path.display());
//     if let Some(unit) = &unit_file {
//         println!("unit file:  {}", unit.display());
//     }
//     println!("service:    {service_state}");
//     match (&pid, alive) {
//         (Some(pid), true) => {
//             println!("engine:     running (pid {pid})");
//             Ok(())
//         }
//         (Some(pid), false) => {
//             println!("engine:     not running (stale lock file, last pid {pid})");
//             Err(EngineError::NotRunning)
//         }
//         (None, _) => {
//             println!("engine:     not running");
//             Err(EngineError::NotRunning)
//         }
//     }
// }
//
// /// Best-effort process liveness via `kill -0` (signal 0 = permission/existence
// /// probe only). Diagnostic: PIDs can be recycled; the kernel flock held by
// /// the engine remains the real authority.
// fn process_alive(pid: &str) -> bool {
//     if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
//         return false;
//     }
//     run_tool("kill", &["-0", pid])
//         .map(|output| output.status.success())
//         .unwrap_or(false)
// }
//
// /// Run an external service-manager tool, capturing its output.
// pub(crate) fn run_tool(program: &str, args: &[&str]) -> EngineResult<std::process::Output> {
//     std::process::Command::new(program)
//         .args(args)
//         .output()
//         .map_err(|e| EngineError::Service(format!("running {program} {}: {e}", args.join(" "))))
// }
//
// /// Directory receiving the service manager's captured stdout/stderr; derived
// /// from the engine's default log file so everything lands in one place.
// #[cfg(any(target_os = "macos", target_os = "linux"))]
// pub(crate) fn engine_log_dir() -> EngineResult<PathBuf> {
//     let log_file = default_log_file().map_err(|e| EngineError::Config(e.to_string()))?;
//     Ok(log_file
//         .parent()
//         .map(std::path::Path::to_path_buf)
//         .unwrap_or_else(|| PathBuf::from(".")))
// }
//
// /// Absolute path of the running binary, recorded into the service
// /// definition. Moving or rebuilding the binary elsewhere requires
// /// `install --force` to re-render.
// #[cfg(any(target_os = "macos", target_os = "linux"))]
// pub(crate) fn current_exe_path() -> EngineResult<PathBuf> {
//     std::env::current_exe()
//         .map_err(|e| EngineError::Io(format!("resolving the engine binary path: {e}")))
// }
//
// /// What `install` should do after comparing the rendered definition with
// /// whatever is on disk.
// #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
// #[derive(Debug, PartialEq, Eq)]
// pub(crate) enum Preflight {
//     /// Nothing installed yet, or `--force`: write and register.
//     Write,
//     /// A byte-identical definition is already installed: skip the write,
//     /// only ensure registration.
//     UpToDate,
// }
//
// /// Idempotency gate: re-running `install` with an unchanged configuration is
// /// a no-op success; a *different* existing definition still demands `--force`
// /// so a divergent unit is never silently replaced.
// #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
// pub(crate) fn preflight(
//     path: &std::path::Path,
//     rendered: &str,
//     force: bool,
// ) -> EngineResult<Preflight> {
//     match std::fs::read_to_string(path) {
//         Ok(existing) if existing == rendered => Ok(Preflight::UpToDate),
//         Ok(_) if !force => Err(EngineError::Service(format!(
//             "{} already exists with a different definition; inspect with \
//              `install --print`, overwrite with `install --force`",
//             path.display()
//         ))),
//         Ok(_) => Ok(Preflight::Write),
//         Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preflight::Write),
//         Err(e) => Err(EngineError::Io(format!("reading {}: {e}", path.display()))),
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn liveness_probe_rejects_garbage_pids() {
//         assert!(!process_alive(""));
//         assert!(!process_alive("abc"));
//         assert!(!process_alive("12x"));
//     }
//
//     #[test]
//     fn our_own_pid_is_alive() {
//         assert!(process_alive(&std::process::id().to_string()));
//     }
//
//     #[test]
//     fn preflight_writes_when_nothing_is_installed() {
//         let dir = tempfile::tempdir().unwrap();
//         let path = dir.path().join("unit");
//         assert_eq!(preflight(&path, "def", false).unwrap(), Preflight::Write);
//     }
//
//     #[test]
//     fn preflight_is_a_noop_for_identical_definitions() {
//         let dir = tempfile::tempdir().unwrap();
//         let path = dir.path().join("unit");
//         std::fs::write(&path, "def").unwrap();
//         assert_eq!(preflight(&path, "def", false).unwrap(), Preflight::UpToDate);
//     }
//
//     #[test]
//     fn preflight_demands_force_for_divergent_definitions() {
//         let dir = tempfile::tempdir().unwrap();
//         let path = dir.path().join("unit");
//         std::fs::write(&path, "old").unwrap();
//         let err = preflight(&path, "new", false).unwrap_err();
//         assert!(err.to_string().contains("--force"), "{err}");
//         assert_eq!(preflight(&path, "new", true).unwrap(), Preflight::Write);
//     }
// }
