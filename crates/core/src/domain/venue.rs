//! Trading venue aggregate: the place where securities are admitted to
//! trading, keyed by its ISO 10383 MIC (e.g. `BVMF` for B3, `XNYS` for NYSE).
//!
//! The MIC is registry-validated ([`Mic`]), so the ISO
//! 10383 operating/segment hierarchy and the venue's country are **derived**
//! from the embedded registry rather than stored: a segment venue (e.g.
//! `XNGS`, Nasdaq Global Select) reports its market operator (`XNAS`) via
//! [`Venue::operating_mic`], and country-level market grouping (US/BR) comes
//! from [`Venue::country_code`]. The venue itself only records what the
//! registry cannot know: our identity for it, a display name, and its
//! lifecycle status within this system.

use crate::StorageFault;
use crate::common::{Empty, NonEmpty, RepositoryResult, Versioned, WriteOutcome};
use chrono::{DateTime, Utc};
use std::{marker::PhantomData, rc::Rc, str::FromStr, sync::Arc};
use uuid::Uuid;
use valqeron_identifiers::{CountryCode, Mic};

const VENUE_NAME_MAX_LEN: usize = 200;

// ================ VENUE DOMAIN ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueName(String);

#[derive(thiserror::Error, Debug)]
pub enum VenueNameError {
    #[error("venue name cannot be empty")]
    Empty,

    #[error("venue name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl VenueName {
    /// # Errors
    ///
    /// Returns `VenueNameError` if value empty or too long.
    pub fn new(value: impl Into<String>) -> Result<Self, VenueNameError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(VenueNameError::Empty);
        }
        if trimmed.chars().count() > VENUE_NAME_MAX_LEN {
            return Err(VenueNameError::TooLong {
                max: VENUE_NAME_MAX_LEN,
            });
        }

        Ok(Self(trimmed.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueId(Uuid);

impl VenueId {
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

impl Default for VenueId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle status of the venue within this system. Distinct from the ISO
/// registry's own active/expired flag ([`Mic::is_active`]): retiring a venue
/// here means we stop accepting listings on it, regardless of what the
/// registry publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VenueStatus {
    #[default]
    Active,
    Retired,
}

#[derive(thiserror::Error, Debug)]
pub enum VenueStatusError {
    #[error("Invalid status. Must be one of: {statuses:?}", statuses = vec!["ACTIVE", "RETIRED"])]
    InvalidStatus,
}

impl VenueStatus {
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, VenueStatus::Active)
    }

    #[must_use]
    pub fn is_retired(&self) -> bool {
        matches!(self, VenueStatus::Retired)
    }
}

impl FromStr for VenueStatus {
    type Err = VenueStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Ok(VenueStatus::Active),
            "RETIRED" => Ok(VenueStatus::Retired),
            _ => Err(VenueStatusError::InvalidStatus),
        }
    }
}

impl From<VenueStatus> for String {
    fn from(val: VenueStatus) -> Self {
        match val {
            VenueStatus::Active => "ACTIVE".into(),
            VenueStatus::Retired => "RETIRED".into(),
        }
    }
}

#[derive(Debug)]
pub struct Venue {
    id: VenueId,
    mic: Mic,
    status: VenueStatus,
    created_at: DateTime<Utc>,

    name: Option<VenueName>,
}

impl Venue {
    #[must_use]
    pub fn builder(mic: Mic) -> VenueBuilder {
        VenueBuilder::new(mic)
    }

    #[must_use]
    pub fn id(&self) -> &VenueId {
        &self.id
    }
    pub fn mic(&self) -> &Mic {
        &self.mic
    }
    #[must_use]
    pub fn status(&self) -> VenueStatus {
        self.status
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn name(&self) -> Option<&VenueName> {
        self.name.as_ref()
    }

    /// The operating MIC that owns this venue, as published in the ISO 10383
    /// registry. Operating venues report themselves.
    pub fn operating_mic(&self) -> Mic {
        self.mic.operating_mic()
    }

    /// The venue's ISO 3166-1 country as published in the ISO 10383 registry;
    /// `None` for the off-exchange pseudo-MICs published under the `ZZ`
    /// marker (e.g. `XOFF`).
    #[must_use]
    pub fn country_code(&self) -> Option<CountryCode> {
        self.mic.country_code()
    }

    /// This venue operates its own market per the ISO 10383 registry (its
    /// published operating MIC is itself).
    #[must_use]
    pub fn is_operating_venue(&self) -> bool {
        self.mic.is_operating()
    }

    #[must_use]
    pub fn reconstitute(
        id: VenueId,
        mic: Mic,
        status: VenueStatus,
        created_at: DateTime<Utc>,
        name: Option<VenueName>,
    ) -> Self {
        Self {
            id,
            mic,
            status,
            created_at,
            name,
        }
    }
}

pub struct VenueBuilder {
    mic: Mic,
    id: Option<VenueId>,
    status: Option<VenueStatus>,
    created_at: Option<DateTime<Utc>>,
    name: Option<VenueName>,
}

impl VenueBuilder {
    #[must_use]
    pub fn new(mic: Mic) -> Self {
        Self {
            mic,
            id: None,
            status: None,
            created_at: None,
            name: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: VenueId) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn status(mut self, status: VenueStatus) -> Self {
        self.status = Some(status);
        self
    }

    #[must_use]
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    #[must_use]
    pub fn name(mut self, name: VenueName) -> Self {
        self.name = Some(name);
        self
    }

    /// Venue has no cross-field invariants: the MIC arrives registry-validated
    /// and the operating/segment relationship and country are derived from the
    /// registry, so building is infallible.
    #[must_use]
    pub fn build(self) -> Venue {
        Venue {
            id: self.id.unwrap_or_default(),
            mic: self.mic,
            status: self.status.unwrap_or_default(),
            created_at: self.created_at.unwrap_or_else(Utc::now),
            name: self.name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VenuePatch {
    pub(crate) name: Option<VenueName>,
    pub(crate) status: Option<VenueStatus>,
}

impl VenuePatch {
    #[must_use]
    pub const fn builder() -> VenuePatchBuilder<Empty> {
        VenuePatchBuilder::new()
    }

    const fn empty() -> Self {
        Self {
            name: None,
            status: None,
        }
    }

    #[must_use]
    pub const fn name(&self) -> Option<&VenueName> {
        self.name.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> Option<VenueStatus> {
        self.status
    }
}

pub struct VenuePatchBuilder<State> {
    inner: VenuePatch,
    _state: PhantomData<State>,
}

impl VenuePatchBuilder<Empty> {
    const fn new() -> Self {
        Self {
            inner: VenuePatch::empty(),
            _state: PhantomData,
        }
    }
}

impl<State> VenuePatchBuilder<State> {
    #[must_use]
    pub fn name(self, name: VenueName) -> VenuePatchBuilder<NonEmpty> {
        VenuePatchBuilder {
            inner: VenuePatch {
                name: Some(name),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn status(self, status: VenueStatus) -> VenuePatchBuilder<NonEmpty> {
        VenuePatchBuilder {
            inner: VenuePatch {
                status: Some(status),
                ..self.inner
            },
            _state: PhantomData,
        }
    }
}

impl VenuePatchBuilder<NonEmpty> {
    #[must_use]
    pub fn build(self) -> VenuePatch {
        self.inner
    }
}

// ================ VENUE REPOSITORY ================
#[cfg_attr(test, mockall::automock)]
pub trait VenueRepository {
    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn find_by_id(&self, id: &VenueId) -> RepositoryResult<Option<Versioned<Venue>>>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn find_by_mic(&self, mic: &Mic) -> RepositoryResult<Option<Versioned<Venue>>>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Venue>>>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn list_by_country(
        &self,
        country_code: &CountryCode,
    ) -> RepositoryResult<Vec<Versioned<Venue>>>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn list_paged(
        &self,
        after: Option<VenueId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Venue>>>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn exists(&self, id: &VenueId) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn exists_by_mic(&self, mic: &Mic) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn insert(&self, venue: &Venue) -> RepositoryResult<()>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn apply_patch(
        &self,
        id: &VenueId,
        expected_version: u32,
        patch: VenuePatch,
    ) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn update(&self, venue: &Venue, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Will return `StorageFault` if a storage fault occurs.
    fn delete(&self, id: &VenueId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

macro_rules! delegate_venue_repository {
    ($ty:ty) => {
        impl<R: VenueRepository + ?Sized> VenueRepository for $ty {
            fn find_by_id(&self, id: &VenueId) -> RepositoryResult<Option<Versioned<Venue>>> {
                (**self).find_by_id(id)
            }
            fn find_by_mic(&self, mic: &Mic) -> RepositoryResult<Option<Versioned<Venue>>> {
                (**self).find_by_mic(mic)
            }
            fn list_all(&self) -> RepositoryResult<Vec<Versioned<Venue>>> {
                (**self).list_all()
            }
            fn list_by_country(
                &self,
                country_code: &CountryCode,
            ) -> RepositoryResult<Vec<Versioned<Venue>>> {
                (**self).list_by_country(country_code)
            }
            fn list_paged(
                &self,
                after: Option<VenueId>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<Venue>>> {
                (**self).list_paged(after, limit)
            }
            fn exists(&self, id: &VenueId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn exists_by_mic(&self, mic: &Mic) -> RepositoryResult<bool> {
                (**self).exists_by_mic(mic)
            }
            fn insert(&self, venue: &Venue) -> RepositoryResult<()> {
                (**self).insert(venue)
            }
            fn apply_patch(
                &self,
                id: &VenueId,
                expected_version: u32,
                patch: VenuePatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(
                &self,
                venue: &Venue,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(venue, expected_version)
            }
            fn delete(
                &self,
                id: &VenueId,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_venue_repository!(Box<R>);
delegate_venue_repository!(Rc<R>);
delegate_venue_repository!(Arc<R>);

// ================ VENUE SERVICE ================
#[derive(Debug, thiserror::Error)]
pub enum RegisterVenueError {
    #[error("a venue with this MIC already exists")]
    DuplicateMic,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Will return `RegisterVenueError` if ducplicated MIC or storage fault.
pub fn register_venue<R: VenueRepository + ?Sized>(
    repo: &R,
    venue: &Venue,
) -> Result<(), RegisterVenueError> {
    if repo.exists_by_mic(venue.mic())? {
        return Err(RegisterVenueError::DuplicateMic);
    }

    repo.insert(venue)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mic(code: &str) -> Option<Mic> {
        Mic::parse(code).ok()
    }

    #[test]
    fn venue_name_trims_and_validates() {
        let name_result = VenueName::new(" B3 S.A. - Brasil, Bolsa, Balcao ");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        assert_eq!(name.as_str(), "B3 S.A. - Brasil, Bolsa, Balcao");
    }

    #[test]
    fn venue_name_empty_fails() {
        assert!(matches!(VenueName::new("  "), Err(VenueNameError::Empty)));
    }

    #[test]
    fn venue_name_too_long_fails() {
        let long_string = "A".repeat(VENUE_NAME_MAX_LEN.saturating_add(1));
        assert!(matches!(
            VenueName::new(long_string),
            Err(VenueNameError::TooLong { max: 200 })
        ));
    }

    #[test]
    fn venue_id_creation_and_conversions() {
        let original_uuid = Uuid::now_v7();
        let id = VenueId::from_uuid(original_uuid);

        assert_eq!(id.as_uuid(), &original_uuid);
        assert_eq!(id.value(), original_uuid.to_string());
        assert_eq!(id.as_bytes(), original_uuid.as_bytes());
        assert_ne!(VenueId::new(), VenueId::new());
    }

    #[test]
    fn venue_status_round_trips() {
        assert!(VenueStatus::default().is_active());

        let active_str: String = VenueStatus::Active.into();
        assert_eq!(active_str, "ACTIVE");
        let retired_str: String = VenueStatus::Retired.into();
        assert_eq!(retired_str, "RETIRED");

        assert!(matches!(
            VenueStatus::from_str("active"),
            Ok(VenueStatus::Active)
        ));
        assert!(matches!(
            VenueStatus::from_str("Retired"),
            Ok(VenueStatus::Retired)
        ));
        assert!(matches!(
            VenueStatus::from_str("UNKNOWN"),
            Err(VenueStatusError::InvalidStatus)
        ));
    }

    #[test]
    fn builder_resolves_defaults_and_derives_registry_facts() {
        let Some(bvmf) = mic("BVMF") else {
            return;
        };
        let venue = Venue::builder(bvmf).build();

        assert_eq!(venue.mic().as_str(), "BVMF");
        assert!(venue.status().is_active());
        assert!(venue.name().is_none());
        assert!(venue.created_at() <= Utc::now());

        // Registry-derived facts: B3 operates its own market in Brazil.
        assert!(venue.is_operating_venue());
        assert_eq!(venue.operating_mic().as_str(), "BVMF");
        assert!(matches!(venue.country_code(), Some(c) if c.as_str() == "BR"));
    }

    #[test]
    fn operating_venue_reports_itself_as_operator() {
        let Some(xnas) = mic("XNAS") else {
            return;
        };
        let venue = Venue::builder(xnas).build();

        assert!(
            venue.is_operating_venue(),
            "the registry publishes XNAS as its own operating MIC"
        );
        assert_eq!(venue.operating_mic().as_str(), "XNAS");
    }

    #[test]
    fn segment_venue_derives_its_operator_from_the_registry() {
        let Some(xngs) = mic("XNGS") else {
            return;
        };

        let venue = Venue::builder(xngs).build();

        assert!(!venue.is_operating_venue());
        assert_eq!(venue.operating_mic().as_str(), "XNAS");
        assert!(matches!(venue.country_code(), Some(c) if c.as_str() == "US"));
    }

    #[test]
    fn single_field_patch_only_sets_that_field() {
        let status_patch = VenuePatch::builder().status(VenueStatus::Retired).build();

        assert!(matches!(status_patch.status(), Some(VenueStatus::Retired)));
        assert!(status_patch.name().is_none());
    }
}

#[cfg(test)]
mod tests_service {
    use super::*;
    use valqeron_identifiers::Mic;

    fn b3_venue() -> Option<Venue> {
        let mic = Mic::parse("BVMF").ok()?;
        Some(Venue::builder(mic).build())
    }

    #[test]
    fn register_inserts_when_mic_is_free() {
        let mut repo = MockVenueRepository::new();
        repo.expect_exists_by_mic().returning(|_| Ok(false));
        repo.expect_insert().returning(|_| Ok(()));

        let Some(venue) = b3_venue() else {
            return;
        };
        assert!(register_venue(&repo, &venue).is_ok());
    }

    #[test]
    fn register_rejects_duplicate_mic_before_insert() {
        let mut repo = MockVenueRepository::new();
        repo.expect_exists_by_mic().returning(|_| Ok(true));
        // insert must never be called on a duplicate.
        repo.expect_insert().never();

        let Some(venue) = b3_venue() else {
            return;
        };
        assert!(matches!(
            register_venue(&repo, &venue),
            Err(RegisterVenueError::DuplicateMic)
        ));
    }
}
