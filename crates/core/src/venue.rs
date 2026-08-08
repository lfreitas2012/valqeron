//! Trading venue aggregate: the place where securities are admitted to
//! trading, keyed by its ISO 10383 MIC (e.g. `BVMF` for B3, `XNYS` for NYSE).
//!
//! The MIC is registry-validated ([`valqeron_identifiers::Mic`]), so the ISO
//! 10383 operating/segment hierarchy and the venue's country are **derived**
//! from the embedded registry rather than stored: a segment venue (e.g.
//! `XNGS`, Nasdaq Global Select) reports its market operator (`XNAS`) via
//! [`Venue::operating_mic`], and country-level market grouping (US/BR) comes
//! from [`Venue::country_code`]. The venue itself only records what the
//! registry cannot know: our identity for it, a display name, and its
//! lifecycle status within this system.

use crate::venue::error::{VenueNameError, VenueStatusError};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use uuid::Uuid;
use valqeron_identifiers::{CountryCode, Mic};

pub mod error;
pub mod patch;
pub mod repository;
pub mod service;

const VENUE_NAME_MAX_LEN: usize = 200;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueName(String);

impl VenueName {
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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueId(Uuid);

impl VenueId {
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

impl VenueStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, VenueStatus::Active)
    }

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
    pub fn builder(mic: Mic) -> VenueBuilder {
        VenueBuilder::new(mic)
    }

    pub fn id(&self) -> &VenueId {
        &self.id
    }
    pub fn mic(&self) -> &Mic {
        &self.mic
    }
    pub fn status(&self) -> VenueStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
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
    pub fn country_code(&self) -> Option<CountryCode> {
        self.mic.country_code()
    }

    /// This venue operates its own market per the ISO 10383 registry (its
    /// published operating MIC is itself).
    pub fn is_operating_venue(&self) -> bool {
        self.mic.is_operating()
    }

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
    pub fn new(mic: Mic) -> Self {
        Self {
            mic,
            id: None,
            status: None,
            created_at: None,
            name: None,
        }
    }

    pub fn id(mut self, id: VenueId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn status(mut self, status: VenueStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

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
}
