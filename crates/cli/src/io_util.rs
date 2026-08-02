use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub enum InputSource {
    File(PathBuf),
    Stdin,
}

impl InputSource {
    pub fn from_arg(arg: &Path) -> Self {
        if arg.as_os_str() == "-" {
            InputSource::Stdin
        } else {
            InputSource::File(arg.to_path_buf())
        }
    }

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

#[derive(Debug, Clone)]
pub enum OutputDest {
    Stdout,
    File(PathBuf),
}

impl OutputDest {
    pub fn from_arg(arg: Option<&Path>) -> Self {
        match arg {
            Some(path) => OutputDest::File(path.to_path_buf()),
            None => OutputDest::Stdout,
        }
    }

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
