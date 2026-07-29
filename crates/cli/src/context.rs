//! The per-invocation application context threaded through command execution.

use crate::error::AppResult;
use crate::io_util::{InputSource, OutputDest};
use serde::Serialize;
use serde_json::{Value, json};

/// Carries invocation-scoped output/behaviour settings and provides the single
/// path through which commands emit their successful JSON result.
pub struct AppContext {
    output: OutputDest,
    input: Option<InputSource>,
    dry_run: bool,
    pretty: bool,
}

impl AppContext {
    pub fn new(
        output: OutputDest,
        input: Option<InputSource>,
        dry_run: bool,
        pretty: bool,
    ) -> Self {
        Self {
            output,
            input,
            dry_run,
            pretty,
        }
    }

    /// Read the optional JSON input document, if `--input` was provided.
    /// Returns `Ok(None)` when no input source is configured.
    pub fn read_input(&self) -> AppResult<Option<Value>> {
        match &self.input {
            Some(source) => source.read_json().map(Some),
            None => Ok(None),
        }
    }

    /// Whether this invocation is a dry-run (writes rolled back).
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Write a successful result as the standard success envelope
    /// (`{ "success": true, "dry_run": <bool>, "data": <payload> }`) to the
    /// configured output destination.
    pub fn write_success<T: Serialize>(&self, data: &T) -> AppResult<()> {
        let envelope = json!({
            "success": true,
            "dry_run": self.dry_run,
            "data": data,
        });
        self.output.write(&envelope, self.pretty)
    }
}
