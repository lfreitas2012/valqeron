//! JSON input/output helpers shared by commands.
//!
//! Input can come from a file, stdin (`-`), or be absent. Output goes to stdout
//! or a file, pretty-printed or compact.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// Where a command should read its JSON document from.
#[derive(Debug, Clone)]
pub enum InputSource {
    /// Read from the given file path.
    File(PathBuf),
    /// Read from standard input (the `-` sentinel).
    Stdin,
}

impl InputSource {
    /// Interpret a `--input` argument: `-` means stdin, anything else is a path.
    pub fn from_arg(arg: &Path) -> Self {
        if arg.as_os_str() == "-" {
            InputSource::Stdin
        } else {
            InputSource::File(arg.to_path_buf())
        }
    }

    /// Read and parse the source into a JSON value.
    pub fn read_json(&self) -> AppResult<Value> {
        let raw = match self {
            InputSource::File(path) => std::fs::read_to_string(path)
                .map_err(|e| AppError::Input(format!("reading {}: {e}", path.display())))?,
            InputSource::Stdin => {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| AppError::Input(format!("reading stdin: {e}")))?;
                buf
            }
        };
        serde_json::from_str(&raw).map_err(|e| AppError::Input(e.to_string()))
    }
}

/// Where serialized JSON output should be written.
#[derive(Debug, Clone)]
pub enum OutputDest {
    /// Write to standard output.
    Stdout,
    /// Write to the given file path.
    File(PathBuf),
}

impl OutputDest {
    /// Build from an optional `--output` argument.
    pub fn from_arg(arg: Option<&Path>) -> Self {
        match arg {
            Some(path) => OutputDest::File(path.to_path_buf()),
            None => OutputDest::Stdout,
        }
    }

    /// Serialize `value` as JSON and write it to this destination.
    pub fn write<T: Serialize>(&self, value: &T, pretty: bool) -> AppResult<()> {
        let mut body = if pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .map_err(|e| AppError::Serialize(e.to_string()))?;
        body.push('\n');

        match self {
            OutputDest::Stdout => {
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                lock.write_all(body.as_bytes())
                    .and_then(|_| lock.flush())
                    .map_err(|e| AppError::Io(format!("writing stdout: {e}")))
            }
            OutputDest::File(path) => std::fs::write(path, body)
                .map_err(|e| AppError::Io(format!("writing {}: {e}", path.display()))),
        }
    }
}
