#![allow(dead_code)]

mod error;

use crate::common::{RepositoryResult, UniqueIdentifier, Versioned};
use crate::tasks::error::BackgroundTasksError;
use chrono::{DateTime, Utc};
use std::fmt;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

const BACKGROUND_TASK_NAME_MAX_CHARACTERS: usize = 100;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BackgroundTaskName(String);

#[derive(Error, Debug, PartialEq, Eq)]
pub enum BackgroundTaskNameError {
    #[error("task name cannot be empty")]
    Empty,

    #[error("task name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl BackgroundTaskName {
    /// # Errors
    ///
    /// Returns `BackgroundTaskNameError`.
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

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for BackgroundTaskName {
    type Err = BackgroundTaskNameError;

    /// # Errors
    ///
    /// Returns `BackgroundTaskNameError`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
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
pub struct ScheduledBackgroundTask<State> {
    id: UniqueIdentifier,
    name: BackgroundTaskName,
    attempts: TaskAttemptTracker,
    state: State,
}

impl<S> ScheduledBackgroundTask<S> {
    #[must_use]
    pub fn id(&self) -> &UniqueIdentifier {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &BackgroundTaskName {
        &self.name
    }

    #[must_use]
    pub fn max_attempts(&self) -> u32 {
        self.attempts.max_attempts
    }

    #[must_use]
    pub fn current_attempt(&self) -> u32 {
        self.attempts.current_attempt
    }
}

impl ScheduledBackgroundTask<Pending> {
    #[must_use]
    pub fn start(self) -> ScheduledBackgroundTask<Running> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: TaskAttemptTracker {
                current_attempt: self.attempts.current_attempt.saturating_add(1),
                ..self.attempts
            },
            state: Running,
        }
    }

    #[must_use]
    pub fn cancel(self) -> ScheduledBackgroundTask<Cancelled> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl ScheduledBackgroundTask<Running> {
    #[must_use]
    pub fn complete(self, output: impl Into<String>) -> ScheduledBackgroundTask<Success> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Success {
                output: output.into(),
            },
        }
    }

    /// # Errors
    ///
    /// Returns `Err(ScheduledBackgroundTask<Failed>)` if maximum attempts have been reached.
    pub fn fail(
        self,
        error: impl Into<String>,
    ) -> Result<ScheduledBackgroundTask<Retrying>, ScheduledBackgroundTask<Failed>> {
        let error = error.into();
        if self.attempts.current_attempt < self.attempts.max_attempts {
            Ok(ScheduledBackgroundTask {
                id: self.id,
                name: self.name,
                attempts: self.attempts,
                state: Retrying { last_error: error },
            })
        } else {
            Err(ScheduledBackgroundTask {
                id: self.id,
                name: self.name,
                attempts: self.attempts,
                state: Failed { error },
            })
        }
    }

    #[must_use]
    pub fn cancel(self) -> ScheduledBackgroundTask<Cancelled> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl ScheduledBackgroundTask<Retrying> {
    #[must_use]
    pub fn last_error(&self) -> &str {
        &self.state.last_error
    }

    #[must_use]
    pub fn start(self) -> ScheduledBackgroundTask<Running> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: TaskAttemptTracker {
                current_attempt: self.attempts.current_attempt.saturating_add(1),
                ..self.attempts
            },
            state: Running,
        }
    }

    #[must_use]
    pub fn cancel(self) -> ScheduledBackgroundTask<Cancelled> {
        ScheduledBackgroundTask {
            id: self.id,
            name: self.name,
            attempts: self.attempts,
            state: Cancelled,
        }
    }
}

impl ScheduledBackgroundTask<Success> {
    #[must_use]
    pub fn output(&self) -> &str {
        &self.state.output
    }
}

impl ScheduledBackgroundTask<Failed> {
    #[must_use]
    pub fn error(&self) -> &str {
        &self.state.error
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn id(mut self, id: UniqueIdentifier) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn name(mut self, name: BackgroundTaskName) -> Self {
        self.name = Some(name);
        self
    }

    #[must_use]
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// # Errors
    ///
    /// Returns `TaskBuilderError` if required fields are missing or validation fails.
    pub fn build(self) -> Result<ScheduledBackgroundTask<Pending>, TaskBuilderError> {
        if self.max_attempts == 0 {
            return Err(TaskBuilderError::ZeroMaxAttempts);
        }
        let id = self.id.ok_or(TaskBuilderError::MissingId)?;
        let name = self.name.ok_or(TaskBuilderError::MissingName)?;

        Ok(ScheduledBackgroundTask {
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

/// Unifies the three terminal typestates into one value a spawned future can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Success(ScheduledBackgroundTask<Success>),
    Failed(ScheduledBackgroundTask<Failed>),
    Cancelled(ScheduledBackgroundTask<Cancelled>),
}

impl TaskOutcome {
    #[must_use]
    pub fn id(&self) -> &UniqueIdentifier {
        match self {
            Self::Success(t) => t.id(),
            Self::Failed(t) => t.id(),
            Self::Cancelled(t) => t.id(),
        }
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }
}

/// Data-less mirror of the lifecycle states, for broadcasting live status to observers that don't
/// own the typestate value itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Retrying,
    Success,
    Failed,
    Cancelled,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Retrying => "retrying",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        })
    }
}

#[derive(Debug, Default)]
struct BackgroundTaskManager {
    tasks: Vec<ScheduledBackgroundTask<Pending>>,
}

impl BackgroundTaskManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Represents a background task definition.
#[derive(Debug, Clone)]
pub struct BackgroundTask {
    id: UniqueIdentifier,
    name: BackgroundTaskName,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl BackgroundTask {
    #[must_use]
    pub fn reconstitute(snapshot: &BackgroundTaskSnapshot) -> Self {
        Self {
            id: snapshot.id,
            name: snapshot.name.clone(),
            created_at: snapshot.created_at,
            last_updated_at: snapshot.last_updated_at,
        }
    }

    #[must_use]
    pub fn id(&self) -> UniqueIdentifier {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &BackgroundTaskName {
        &self.name
    }

    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub fn last_updated_at(&self) -> DateTime<Utc> {
        self.last_updated_at
    }
}

#[derive(Debug)]
pub struct BackgroundTaskSnapshot {
    pub id: UniqueIdentifier,
    pub name: BackgroundTaskName,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
}

// ================ BACKGROUND TASKS REPOSITORY ================
macro_rules! delegate_background_tasks_repository {
    ($ty:ty) => {
        impl<R: BackgroundTasksRegistryRepository + ?Sized> BackgroundTasksRegistryRepository
            for $ty
        {
            fn find_by_id(
                &self,
                id: &UniqueIdentifier,
            ) -> RepositoryResult<Option<Versioned<BackgroundTask>>> {
                (**self).find_by_id(id)
            }
            fn list_paged(
                &self,
                after: Option<UniqueIdentifier>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<BackgroundTask>>> {
                (**self).list_paged(after, limit)
            }
        }
    };
}

delegate_background_tasks_repository!(Box<R>);
delegate_background_tasks_repository!(Rc<R>);
delegate_background_tasks_repository!(Arc<R>);

#[cfg_attr(test, mockall::automock)]
pub trait BackgroundTasksRegistryRepository {
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn find_by_id(
        &self,
        id: &UniqueIdentifier,
    ) -> RepositoryResult<Option<Versioned<BackgroundTask>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_paged(
        &self,
        after: Option<UniqueIdentifier>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<BackgroundTask>>>;
}

// ================ BACKGROUND TASKS SERVICE ================
/// # Errors
///
/// Returns `BackgroundTasksError`.
pub fn get_background_task<R: BackgroundTasksRegistryRepository + ?Sized>(
    repo: &R,
    id: &UniqueIdentifier,
) -> Result<Option<Versioned<BackgroundTask>>, BackgroundTasksError> {
    Ok(repo.find_by_id(id)?)
}

/// # Errors
///
/// Returns `BackgroundTasksError`.
pub fn list_background_tasks<R: BackgroundTasksRegistryRepository + ?Sized>(
    repo: &R,
    after: Option<UniqueIdentifier>,
    limit: u32,
) -> Result<Vec<Versioned<BackgroundTask>>, BackgroundTasksError> {
    Ok(repo.list_paged(after, limit)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    const TEST_TASK_NAME: &str = "test_task";

    #[test]
    fn test_task_name_max_length() {
        let max_len = BACKGROUND_TASK_NAME_MAX_CHARACTERS.saturating_add(1);
        let name = BackgroundTaskName::new("TEST_TASK_NAME".repeat(max_len));
        assert_matches!(name, Err(BackgroundTaskNameError::TooLong { max: _ }));
    }

    #[test]
    fn test_task_name_empty() {
        let name = BackgroundTaskName::new("");
        assert_matches!(name, Err(BackgroundTaskNameError::Empty));
    }

    #[test]
    fn test_task_name_trim_spaces() {
        let name = BackgroundTaskName::new(format!("  {TEST_TASK_NAME}  "));
        assert_eq!(
            name.as_ref().map(BackgroundTaskName::as_str),
            Ok(TEST_TASK_NAME)
        );
    }

    #[test]
    fn test_task_name_trim_newlines() {
        let name = BackgroundTaskName::new(format!("\n{TEST_TASK_NAME}\n"));
        assert_eq!(
            name.as_ref().map(BackgroundTaskName::as_str),
            Ok(TEST_TASK_NAME)
        );
    }

    #[test]
    fn test_task_name_trim_tabs() {
        let name = BackgroundTaskName::new(format!("\t{TEST_TASK_NAME}\t"));
        assert_eq!(
            name.as_ref().map(BackgroundTaskName::as_str),
            Ok(TEST_TASK_NAME)
        );
    }

    #[test]
    fn test_task_builder_builds_successfully() {
        let id = UniqueIdentifier::new();
        let name_res = BackgroundTaskName::new(TEST_TASK_NAME);
        assert_matches!(name_res, Ok(_));

        if let Ok(name) = name_res {
            let task_res = BackgroundTaskBuilder::new().id(id).name(name).build();

            assert_matches!(
                task_res,
                Ok(ref task_1) if task_1.name.as_str() == TEST_TASK_NAME && task_1.id == id
            );
        }
    }

    #[test]
    fn test_task_builder_zero_attempts_fails() {
        let id = UniqueIdentifier::new();
        let name_res = BackgroundTaskName::new(TEST_TASK_NAME);
        assert_matches!(name_res, Ok(_));

        if let Ok(name) = name_res {
            let res = BackgroundTaskBuilder::new()
                .id(id)
                .name(name)
                .max_attempts(0)
                .build();

            assert_matches!(res, Err(TaskBuilderError::ZeroMaxAttempts));
        }
    }

    #[test]
    fn test_task_successful_lifecycle() {
        let id = UniqueIdentifier::new();
        let name_res = BackgroundTaskName::new(TEST_TASK_NAME);
        assert_matches!(name_res, Ok(_));

        if let Ok(name) = name_res {
            let task_res = BackgroundTaskBuilder::new()
                .id(id)
                .name(name)
                .max_attempts(3)
                .build();

            assert_matches!(task_res, Ok(_));

            if let Ok(task) = task_res {
                assert_eq!(task.current_attempt(), 0);

                let task: ScheduledBackgroundTask<Running> = task.start();
                assert_eq!(task.current_attempt(), 1);

                let task: ScheduledBackgroundTask<Success> = task.complete("done");
                assert_eq!(task.output(), "done");
            }
        }
    }

    #[test]
    fn test_task_retry_and_fail_lifecycle() {
        let id = UniqueIdentifier::new();
        let name_res = BackgroundTaskName::new(TEST_TASK_NAME);
        assert_matches!(name_res, Ok(_));

        if let Ok(name) = name_res {
            let task_res = BackgroundTaskBuilder::new()
                .id(id)
                .name(name)
                .max_attempts(2)
                .build();

            assert_matches!(task_res, Ok(_));

            if let Ok(task) = task_res {
                // First attempt
                let task = task.start();
                assert_eq!(task.current_attempt(), 1);

                // Fails but yields a Retrying state because max_attempts is 2
                let retry_res = task.fail("first failure");
                assert_matches!(retry_res, Ok(_));

                if let Ok(task) = retry_res {
                    assert_eq!(task.last_error(), "first failure");

                    // Second attempt
                    let task = task.start();
                    assert_eq!(task.current_attempt(), 2);

                    // Fails completely because we reached max_attempts
                    let fail_res = task.fail("second failure");
                    assert_matches!(fail_res, Err(_));

                    if let Err(final_err) = fail_res {
                        assert_eq!(final_err.error(), "second failure");
                    }
                }
            }
        }
    }
}
