use thiserror::Error;
use valqeron_common::UniqueIdentifier;

const BACKGROUND_TASK_NAME_MAX_CHARACTERS: usize = 100;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BackgroundTaskName(String);

#[derive(Error, Debug)]
pub enum BackgroundTaskNameError {
    #[error("task name cannot be empty")]
    Empty,

    #[error("task name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl BackgroundTaskName {
    pub fn new(value: impl Into<String>) -> Result<Self, BackgroundTaskNameError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(BackgroundTaskNameError::Empty);
        }
        if trimmed.chars().count() > BACKGROUND_TASK_NAME_MAX_CHARACTERS {
            return Err(BackgroundTaskNameError::TooLong {
                max: BACKGROUND_TASK_NAME_MAX_CHARACTERS,
            });
        }

        Ok(Self(trimmed.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundTask {
    id: UniqueIdentifier,
    name: BackgroundTaskName,
}

impl BackgroundTask {
    pub fn new(id: UniqueIdentifier, name: BackgroundTaskName) -> Self {
        Self { id, name }
    }
}

#[derive(Error, Debug)]
pub enum TaskBuilderError {
    #[error("max_attempts must be at least 1")]
    ZeroMaxAttempts,

    #[error("Missing task id. Use `BackgroundTaskBuilder::id` to set it")]
    MissingId,

    #[error("Missing task name. Use `BackgroundTaskBuilder::name` to set it")]
    MissingName,
}

#[derive(Default)]
pub struct BackgroundTaskBuilder {
    id: Option<UniqueIdentifier>,
    name: Option<BackgroundTaskName>,
}

impl BackgroundTaskBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: UniqueIdentifier) -> Self {
        self.id = Some(id);
        self
    }

    pub fn name(mut self, name: BackgroundTaskName) -> Self {
        self.name = Some(name);
        self
    }

    pub fn build(self) -> Result<BackgroundTask, TaskBuilderError> {
        let id = self.id.ok_or(TaskBuilderError::MissingId)?;
        let name = self.name.ok_or(TaskBuilderError::MissingName)?;

        Ok(BackgroundTask::new(id, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    const TEST_TASK_NAME: &str = "test_task";

    #[test]
    fn test_task_name_max_length() {
        let name = BackgroundTaskName::new(
            "TEST_TASK_NAME".repeat(BACKGROUND_TASK_NAME_MAX_CHARACTERS + 1),
        );
        assert!(name.is_err());
        assert_matches!(
            name.unwrap_err(),
            BackgroundTaskNameError::TooLong { max: _ }
        )
    }

    #[test]
    fn test_task_name_empty() {
        let name = BackgroundTaskName::new("");
        assert!(name.is_err());
        assert_matches!(name.unwrap_err(), BackgroundTaskNameError::Empty);
    }

    #[test]
    fn test_task_name_trim_spaces() {
        let name = BackgroundTaskName::new(format!("  {}  ", TEST_TASK_NAME));
        assert!(name.is_ok());
        assert_eq!(name.unwrap().as_str(), TEST_TASK_NAME);
    }

    #[test]
    fn test_task_name_trim_newlines() {
        let name = BackgroundTaskName::new(format!("\n{}\n", TEST_TASK_NAME));
        assert!(name.is_ok());
        assert_eq!(name.unwrap().as_str(), TEST_TASK_NAME);
    }

    #[test]
    fn test_task_name_trim_tabs() {
        let name = BackgroundTaskName::new(format!("\t{}\t", TEST_TASK_NAME));
        assert!(name.is_ok());
        assert_eq!(name.unwrap().as_str(), TEST_TASK_NAME);
    }

    #[test]
    fn test_task_builder_builds_successfully() {
        let id = UniqueIdentifier::new();
        let name = BackgroundTaskName::new(TEST_TASK_NAME).unwrap();

        let task_1 = BackgroundTaskBuilder::new()
            .id(id)
            .name(name)
            .build()
            .unwrap();

        assert_eq!(task_1.name.as_str(), TEST_TASK_NAME);
        assert_eq!(task_1.id, id);
    }
}
