use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

    pub fn read_json(&self) -> anyhow::Result<Value> {
        let raw = match self {
            InputSource::File(path) => std::fs::read_to_string(path)
                .with_context(|| format!("failed to read input file {}", path.display()))?,
            InputSource::Stdin => {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("failed to read JSON input from stdin")?;
                buf
            }
        };
        serde_json::from_str(&raw).context("failed to parse JSON input")
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

    pub fn write<T: Serialize>(&self, value: &T, pretty: bool) -> anyhow::Result<()> {
        let mut body = if pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .context("failed to serialize JSON output")?;
        body.push('\n');

        match self {
            OutputDest::Stdout => {
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                lock.write_all(body.as_bytes())
                    .and_then(|_| lock.flush())
                    .context("failed to write JSON output to stdout")
            }
            OutputDest::File(path) => std::fs::write(path, body)
                .with_context(|| format!("failed to write JSON output to {}", path.display())),
        }
    }
}
