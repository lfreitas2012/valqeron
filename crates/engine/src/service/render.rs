// use std::path::Path;
// 
// use crate::error::{EngineError, EngineResult};
// 
// /// Render a `{{KEY}}` template. Any placeholder left after substitution is
// /// an error — this catches template/variable drift at install time (and in
// /// unit tests) instead of shipping a broken service definition.
// pub fn render(template: &str, vars: &[(&str, &str)]) -> EngineResult<String> {
//     let mut out = template.to_string();
//     for (key, value) in vars {
//         let needle = format!("{{{{{key}}}}}");
//         out = out.replace(&needle, value);
//     }
//     if out.contains("{{") {
//         let leftovers: Vec<&str> = out
//             .lines()
//             .filter(|line| line.contains("{{"))
//             .map(str::trim)
//             .collect();
//         return Err(EngineError::Service(format!(
//             "service template contains unreplaced placeholders: {}",
//             leftovers.join(" | ")
//         )));
//     }
//     Ok(out)
// }
// 
// /// Minimal escaping for text nodes in the launchd plist.
// pub fn xml_escape(value: &str) -> String {
//     value
//         .replace('&', "&amp;")
//         .replace('<', "&lt;")
//         .replace('>', "&gt;")
// }
// 
// /// Build the launchd `EnvironmentVariables` dict for explicit overrides
// /// (flags or env at install time). Defaults need nothing: the engine
// /// resolves them the same way clients do.
// pub fn launchd_env_block(db_path: Option<&Path>, socket: Option<&Path>) -> String {
//     let mut entries = String::new();
//     for (key, path) in [("VALQERON_DB", db_path), ("VALQERON_SOCKET", socket)] {
//         if let Some(path) = path {
//             entries.push_str(&format!(
//                 "    <key>{key}</key>\n    <string>{}</string>\n",
//                 xml_escape(&path.display().to_string())
//             ));
//         }
//     }
//     if entries.is_empty() {
//         String::new()
//     } else {
//         format!("  <key>EnvironmentVariables</key>\n  <dict>\n{entries}  </dict>\n")
//     }
// }
// 
// /// Build the systemd `Environment=` block for explicit overrides.
// ///
// /// Compiled everywhere (like the templates) so its tests run on every
// /// platform; only the Linux build has a non-test caller.
// #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
// pub fn systemd_env_block(db_path: Option<&Path>, socket: Option<&Path>) -> String {
//     let mut block = String::new();
//     for (key, path) in [("VALQERON_DB", db_path), ("VALQERON_SOCKET", socket)] {
//         if let Some(path) = path {
//             block.push_str(&format!(
//                 "Environment=\"{key}={}\"\n",
//                 systemd_env_escape(&path.display().to_string())
//             ));
//         }
//     }
//     block
// }
// 
// /// Escape a value for a double-quoted systemd assignment: `%` would trigger
// /// specifier expansion (`%h`, `%i`, …), and backslashes/quotes carry C-style
// /// escape semantics inside quoted unit-file strings.
// #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
// pub fn systemd_env_escape(value: &str) -> String {
//     value
//         .replace('\\', "\\\\")
//         .replace('"', "\\\"")
//         .replace('%', "%%")
// }
// 
// #[cfg(test)]
// mod tests {
//     use super::*;
// 
//     #[test]
//     fn substitutes_every_occurrence() {
//         let out = render("a {{X}} b {{X}} {{Y}}", &[("X", "1"), ("Y", "2")]).unwrap();
//         assert_eq!(out, "a 1 b 1 2");
//     }
// 
//     #[test]
//     fn leftover_placeholders_are_an_error() {
//         let err = render("hello {{MISSING}}", &[]).unwrap_err();
//         assert!(err.to_string().contains("MISSING"), "{err}");
//     }
// 
//     #[test]
//     fn xml_escape_handles_ampersand_first() {
//         assert_eq!(xml_escape("a&b<c>d"), "a&amp;b&lt;c&gt;d");
//         assert_eq!(xml_escape("&lt;"), "&amp;lt;");
//     }
// 
//     #[test]
//     fn launchd_template_renders_without_leftovers() {
//         let out = render(
//             super::super::launchd_template(),
//             &[
//                 ("LABEL", "io.valqeron.engine"),
//                 ("EXE", "/usr/local/bin/valqeron-engine"),
//                 ("STDOUT_PATH", "/tmp/out.log"),
//                 ("STDERR_PATH", "/tmp/err.log"),
//                 ("ENV_BLOCK", ""),
//             ],
//         )
//         .unwrap();
//         assert!(out.contains("<string>/usr/local/bin/valqeron-engine</string>"));
//         assert!(out.contains("SuccessfulExit"));
//         assert!(!out.contains("{{"));
//     }
// 
//     #[test]
//     fn systemd_template_renders_without_leftovers() {
//         let out = render(
//             super::super::systemd_template(),
//             &[
//                 ("EXE", "/usr/local/bin/valqeron-engine"),
//                 ("RW_PATHS", "\"/data\" \"/logs\""),
//                 ("ENV_BLOCK", ""),
//             ],
//         )
//         .unwrap();
//         assert!(out.contains("Restart=on-failure"));
//         assert!(out.contains("WantedBy=default.target"));
//         assert!(!out.contains("{{"));
//     }
// 
//     #[test]
//     fn launchd_env_block_is_empty_without_overrides() {
//         assert!(launchd_env_block(None, None).is_empty());
//         assert!(systemd_env_block(None, None).is_empty());
//     }
// 
//     #[test]
//     fn launchd_env_block_escapes_and_lists_overrides() {
//         let block = launchd_env_block(
//             Some(Path::new("/data/a&b.db")),
//             Some(Path::new("/run/v.sock")),
//         );
//         assert!(block.contains("<key>EnvironmentVariables</key>"));
//         assert!(block.contains("<key>VALQERON_DB</key>"));
//         assert!(block.contains("a&amp;b.db"), "{block}");
//         assert!(block.contains("<key>VALQERON_SOCKET</key>"));
//         assert!(block.contains("/run/v.sock"));
//     }
// 
//     #[test]
//     fn systemd_env_block_lists_overrides() {
//         let block = systemd_env_block(Some(Path::new("/data/v.db")), None);
//         assert_eq!(block, "Environment=\"VALQERON_DB=/data/v.db\"\n");
//     }
// 
//     #[test]
//     fn systemd_env_values_are_escaped_against_specifier_expansion() {
//         let block = systemd_env_block(Some(Path::new("/data/100%/v.db")), None);
//         assert_eq!(block, "Environment=\"VALQERON_DB=/data/100%%/v.db\"\n");
//         assert_eq!(systemd_env_escape(r#"a\b"c%d"#), r#"a\\b\"c%%d"#);
//     }
// 
//     #[test]
//     fn templates_contain_no_developer_machine_paths() {
//         for template in [
//             super::super::launchd_template(),
//             super::super::systemd_template(),
//         ] {
//             assert!(
//                 !template.contains("/Users/") && !template.contains("/home/"),
//                 "template must not hardcode a developer path"
//             );
//         }
//     }
// }
