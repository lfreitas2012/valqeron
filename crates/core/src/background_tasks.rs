use thiserror::Error;
use uuid::Uuid;

// ================ TASK ID ================
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct BackgroundTaskId(Uuid);

impl BackgroundTaskId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn value(&self) -> String {
        self.0.to_string()
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Default for BackgroundTaskId {
    fn default() -> Self {
        Self::new()
    }
}

// ================ TASK NAME ================
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

// ================ TASK STATUS ================
