use std::ffi::OsStr;
use std::fmt::Display;
use uuid::Uuid;

pub fn os_str_is_off(value: &OsStr) -> bool {
    value
        .to_str()
        .map(|s| {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "off" | "false" | "0" | "none"
            )
        })
        .unwrap_or(false)
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct UniqueIdentifier(Uuid);

impl UniqueIdentifier {
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
        write!(f, "{}", self.0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::{UniqueIdentifier, os_str_is_off};
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
