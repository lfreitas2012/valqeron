use crate::storage::StorageFault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub data: T,
    pub version: u32,
}

/// Result alias shared by every repository port.
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

/// Typestate markers shared by the patch builders: a patch can only be built once at least one
/// field has been set (`NonEmpty`).
pub struct Empty;
pub struct NonEmpty;

/// Whether related child entities are fetched together with an aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadMode {
    /// Do not fetch related entities; collections stay [`Loading::NotLoaded`].
    #[default]
    Lazy,
    /// Fetch related entities alongside the aggregate.
    Eager,
}

/// Explicit placeholder for related data that may or may not have been fetched. Keeps the domain
/// free of storage handles: a `NotLoaded` collection is loaded on demand through the owning
/// repository, never by the entity itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loading<T> {
    /// The relation was not fetched (lazy read); its state is unknown.
    NotLoaded,
    /// The relation was fetched; `Loaded` with an empty collection means "known to have none".
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
