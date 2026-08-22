use crate::StorageFault;
use std::ffi::OsStr;
use std::fmt::Display;
use uuid::Uuid;

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

#[must_use]
pub fn os_str_is_off(value: &OsStr) -> bool {
    value.to_str().is_some_and(|s| {
        matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "off" | "false" | "0" | "none"
        )
    })
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct UniqueIdentifier(Uuid);

impl UniqueIdentifier {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub fn value(&self) -> String {
        self.0.to_string()
    }

    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Default for UniqueIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for UniqueIdentifier {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Display for UniqueIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::common::{UniqueIdentifier, os_str_is_off};
    use std::ffi::OsStr;

    #[test]
    fn is_off_recognizes_disable_values() {
        for v in ["off", "OFF", "Off", "false", "0", "none", "  off  "] {
            assert!(os_str_is_off(OsStr::new(v)), "{v:?} should disable");
        }
        for v in ["on", "1", "true", "/tmp/logs/x.log"] {
            assert!(!os_str_is_off(OsStr::new(v)), "{v:?} should not disable");
        }
    }

    #[test]
    fn test_unique_identifier_uuid_v7() {
        let id = UniqueIdentifier::new();
        assert_eq!(id.value(), id.as_uuid().to_string());
    }
}
