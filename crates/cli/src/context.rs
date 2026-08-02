use crate::error::AppResult;
use crate::io_util::{InputSource, OutputDest};
use serde::Serialize;
use serde_json::{Value, json};

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

    pub fn read_input(&self) -> AppResult<Option<Value>> {
        match &self.input {
            Some(source) => source.read_json().map(Some),
            None => Ok(None),
        }
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn write_success<T: Serialize>(&self, data: &T) -> AppResult<()> {
        let envelope = json!({
            "success": true,
            "dry_run": self.dry_run,
            "data": data,
        });
        self.output.write(&envelope, self.pretty)
    }
}
