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
pub struct Pending;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Success {
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retrying {
    pub last_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancelled;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskAttemptTracker {
    max_attempts: u32,
    current_attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundTask<State> {
    id: UniqueIdentifier,
    name: BackgroundTaskName,
    attempts: TaskAttemptTracker,
    state: State,
}

impl<S> BackgroundTask<S> {
    pub fn id(&self) -> &UniqueIdentifier {
        &self.id
    }

    pub fn name(&self) -> &BackgroundTaskName {
        &self.name
    }

    pub fn max_attempts(&self) -> u32 {
        self.attempts.max_attempts
    }

    pub fn current_attempt(&self) -> u32 {
        self.attempts.current_attempt
    }
}

impl BackgroundTask<Pending> {
    pub fn start(self) -> BackgroundTask<Running> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: TaskAttemptTracker {
                current_attempt: self.attempts.current_attempt + 1,
                ..self.attempts
            },
            state: Running,
        }
    }

    pub fn cancel(self) -> BackgroundTask<Cancelled> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl BackgroundTask<Running> {
    pub fn complete(self, output: impl Into<String>) -> BackgroundTask<Success> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Success {
                output: output.into(),
            },
        }
    }

    pub fn fail(
        self,
        error: impl Into<String>,
    ) -> Result<BackgroundTask<Retrying>, BackgroundTask<Failed>> {
        let error = error.into();
        if self.attempts.current_attempt < self.attempts.max_attempts {
            Ok(BackgroundTask {
                id: self.id,
                name: self.name,
                attempts: self.attempts,
                state: Retrying { last_error: error },
            })
        } else {
            Err(BackgroundTask {
                id: self.id,
                name: self.name,
                attempts: self.attempts,
                state: Failed { error },
            })
        }
    }

    pub fn cancel(self) -> BackgroundTask<Cancelled> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl BackgroundTask<Retrying> {
    pub fn last_error(&self) -> &str {
        &self.state.last_error
    }

    pub fn start(self) -> BackgroundTask<Running> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: TaskAttemptTracker {
                current_attempt: self.attempts.current_attempt + 1,
                ..self.attempts
            },
            state: Running,
        }
    }

    pub fn cancel(self) -> BackgroundTask<Cancelled> {
        BackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl BackgroundTask<Success> {
    pub fn output(&self) -> &str {
        &self.state.output
    }
}

impl BackgroundTask<Failed> {
    pub fn error(&self) -> &str {
        &self.state.error
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

pub struct BackgroundTaskBuilder {
    id: Option<UniqueIdentifier>,
    name: Option<BackgroundTaskName>,
    max_attempts: u32,
}

impl Default for BackgroundTaskBuilder {
    fn default() -> Self {
        Self {
            id: None,
            name: None,
            max_attempts: 1,
        }
    }
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

    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    pub fn build(self) -> Result<BackgroundTask<Pending>, TaskBuilderError> {
        if self.max_attempts == 0 {
            return Err(TaskBuilderError::ZeroMaxAttempts);
        }
        let id = self.id.ok_or(TaskBuilderError::MissingId)?;
        let name = self.name.ok_or(TaskBuilderError::MissingName)?;

        Ok(BackgroundTask {
            id,
            name,
            attempts: TaskAttemptTracker {
                max_attempts: self.max_attempts,
                current_attempt: 0,
            },
            state: Pending,
        })
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

    #[test]
    fn test_task_builder_zero_attempts_fails() {
        let id = UniqueIdentifier::new();
        let name = BackgroundTaskName::new(TEST_TASK_NAME).unwrap();

        let res = BackgroundTaskBuilder::new()
            .id(id)
            .name(name)
            .max_attempts(0)
            .build();

        assert_matches!(res.unwrap_err(), TaskBuilderError::ZeroMaxAttempts);
    }

    #[test]
    fn test_task_successful_lifecycle() {
        let id = UniqueIdentifier::new();
        let name = BackgroundTaskName::new(TEST_TASK_NAME).unwrap();

        let task: BackgroundTask<Pending> = BackgroundTaskBuilder::new()
            .id(id)
            .name(name)
            .max_attempts(3)
            .build()
            .unwrap();

        assert_eq!(task.current_attempt(), 0);

        let task: BackgroundTask<Running> = task.start();
        assert_eq!(task.current_attempt(), 1);

        let task: BackgroundTask<Success> = task.complete("done");
        assert_eq!(task.output(), "done");
    }

    #[test]
    fn test_task_retry_and_fail_lifecycle() {
        let id = UniqueIdentifier::new();
        let name = BackgroundTaskName::new(TEST_TASK_NAME).unwrap();

        // 2 maximum execution attempts total
        let task = BackgroundTaskBuilder::new()
            .id(id)
            .name(name)
            .max_attempts(2)
            .build()
            .unwrap();

        // First attempt
        let task = task.start();
        assert_eq!(task.current_attempt(), 1);

        // Fails but yields a Retrying state because max_attempts is 2
        let task = task.fail("first failure").unwrap();
        assert_eq!(task.last_error(), "first failure");

        // Second attempt
        let task = task.start();
        assert_eq!(task.current_attempt(), 2);

        // Fails completely because we reached max_attempts
        let final_err = task.fail("second failure").unwrap_err();
        assert_eq!(final_err.error(), "second failure");
    }
}
