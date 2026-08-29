use crate::StorageFault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub data: T,
    pub version: u32,
}

pub type RepositoryResult<T> = Result<T, StorageFault>;

#[must_use = "a write outcome may indicate the write did not apply"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Applied,

    VersionMismatch { expected: u32, actual: u32 },

    Missing,
}

impl WriteOutcome {
    #[must_use]
    pub const fn applied(self) -> bool {
        matches!(self, Self::Applied)
    }
}

pub struct Empty;
pub struct NonEmpty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadMode {
    #[default]
    Lazy,
    Eager,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loading<T> {
    NotLoaded,
    Loaded(T),
}

impl<T> Loading<T> {
    #[must_use]
    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    #[must_use]
    pub const fn as_loaded(&self) -> Option<&T> {
        match self {
            Self::Loaded(value) => Some(value),
            Self::NotLoaded => None,
        }
    }
}
