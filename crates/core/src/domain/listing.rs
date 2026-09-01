use crate::{
    StorageFault,
    common::{Empty, NonEmpty, RepositoryResult, Versioned, WriteOutcome},
    domain::{
        security::{SecurityId, SecurityRepository},
        venue::{VenueId, VenueRepository},
    },
};
use chrono::{DateTime, NaiveDate, Utc};
use std::{fmt, marker::PhantomData, rc::Rc, str::FromStr, sync::Arc};
use uuid::Uuid;

const TICKER_SYMBOL_MAX_LEN: usize = 12;
const MARKET_SEGMENT_MAX_LEN: usize = 100;

// ================ LISTING DOMAIN ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TickerSymbol(String);

#[derive(thiserror::Error, Debug)]
pub enum TickerSymbolError {
    #[error("ticker symbol cannot be empty")]
    Empty,

    #[error("ticker symbol exceeds maximum length of {max} characters")]
    TooLong { max: usize },

    #[error("ticker symbol must contain only ASCII letters, digits, '.' or '-'")]
    InvalidCharacter,
}

impl TickerSymbol {

    /// # Errors
    ///
    /// Returns `TickerSymbolError`.
    pub fn new(value: impl Into<String>) -> Result<Self, TickerSymbolError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(TickerSymbolError::Empty);
        }
        if trimmed.chars().count() > TICKER_SYMBOL_MAX_LEN {
            return Err(TickerSymbolError::TooLong {
                max: TICKER_SYMBOL_MAX_LEN,
            });
        }

        let normalized = trimmed.to_ascii_uppercase();
        if !normalized
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
        {
            return Err(TickerSymbolError::InvalidCharacter);
        }

        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarketSegment(String);

#[derive(thiserror::Error, Debug)]
pub enum MarketSegmentError {
    #[error("market segment cannot be empty")]
    Empty,

    #[error("market segment exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl MarketSegment {
    /// # Errors
    ///
    /// Returns `MarketSegmentError`.
    pub fn new(value: impl Into<String>) -> Result<Self, MarketSegmentError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(MarketSegmentError::Empty);
        }
        if trimmed.chars().count() > MARKET_SEGMENT_MAX_LEN {
            return Err(MarketSegmentError::TooLong {
                max: MARKET_SEGMENT_MAX_LEN,
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
pub struct ListingId(Uuid);

impl ListingId {
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

impl Default for ListingId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ListingRole {
    #[default]
    Primary,
    Secondary,
}

#[derive(thiserror::Error, Debug)]
pub enum ListingRoleError {
    #[error("Invalid role. Must be one of: {roles:?}", roles = vec!["PRIMARY", "SECONDARY"])]
    InvalidRole,
}

impl ListingRole {
    #[must_use]
    pub fn is_primary(&self) -> bool {
        matches!(self, ListingRole::Primary)
    }

    #[must_use]
    pub fn is_secondary(&self) -> bool {
        matches!(self, ListingRole::Secondary)
    }
}

impl FromStr for ListingRole {
    type Err = ListingRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "PRIMARY" => Ok(ListingRole::Primary),
            "SECONDARY" => Ok(ListingRole::Secondary),
            _ => Err(ListingRoleError::InvalidRole),
        }
    }
}

impl From<ListingRole> for String {
    fn from(val: ListingRole) -> Self {
        match val {
            ListingRole::Primary => "PRIMARY".into(),
            ListingRole::Secondary => "SECONDARY".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ListingStatus {
    #[default]
    Active,
    Suspended,
    Delisted,
}

#[derive(thiserror::Error, Debug)]
pub enum ListingStatusError {
    #[error(
        "Invalid status. Must be one of: {statuses:?}",
        statuses = vec!["ACTIVE", "SUSPENDED", "DELISTED"]
    )]
    InvalidStatus,
}

impl ListingStatus {
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, ListingStatus::Active)
    }

    #[must_use]
    pub fn is_suspended(&self) -> bool {
        matches!(self, ListingStatus::Suspended)
    }

    #[must_use]
    pub fn is_delisted(&self) -> bool {
        matches!(self, ListingStatus::Delisted)
    }
}

impl FromStr for ListingStatus {
    type Err = ListingStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Ok(ListingStatus::Active),
            "SUSPENDED" => Ok(ListingStatus::Suspended),
            "DELISTED" => Ok(ListingStatus::Delisted),
            _ => Err(ListingStatusError::InvalidStatus),
        }
    }
}

impl From<ListingStatus> for String {
    fn from(val: ListingStatus) -> Self {
        match val {
            ListingStatus::Active => "ACTIVE".into(),
            ListingStatus::Suspended => "SUSPENDED".into(),
            ListingStatus::Delisted => "DELISTED".into(),
        }
    }
}

const CURRENCY_CODE_LEN: usize = 3;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CurrencyCodeError {
    #[error("currency code cannot be empty")]
    Empty,

    #[error("currency code must be exactly {expected} characters", expected = CURRENCY_CODE_LEN)]
    InvalidLength,

    #[error("currency code must contain only ASCII letters")]
    InvalidCharacter,
}

/// ISO 4217 alphabetic currency code, e.g. `BRL` or `USD`.
///
/// Stored as normalized uppercase ASCII.
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Debug)]
pub struct CurrencyCode([u8; CURRENCY_CODE_LEN]);

impl CurrencyCode {
    /// Parses a currency code, trimming surrounding whitespace and normalizing
    /// to uppercase ASCII.
    ///
    /// # Errors
    ///
    /// Returns `CurrencyCodeError`.
    pub fn parse(input: &str) -> Result<Self, CurrencyCodeError> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Err(CurrencyCodeError::Empty);
        }
        if trimmed.chars().count() != CURRENCY_CODE_LEN {
            return Err(CurrencyCodeError::InvalidLength);
        }
        if !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(CurrencyCodeError::InvalidCharacter);
        }

        let mut bytes = [0u8; CURRENCY_CODE_LEN];
        for (slot, byte) in bytes.iter_mut().zip(trimmed.bytes()) {
            *slot = byte.to_ascii_uppercase();
        }
        Ok(Self(bytes))
    }

    /// Alias for [`CurrencyCode::parse`], mirroring the `valqeron-identifiers`
    /// API.
    ///
    /// # Errors
    ///
    /// Returns `CurrencyCodeError`.
    pub fn new(input: &str) -> Result<Self, CurrencyCodeError> {
        Self::parse(input)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        // The constructor guarantees uppercase ASCII, so this never falls back.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; CURRENCY_CODE_LEN] {
        &self.0
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyCodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct Listing {
    id: ListingId,
    security_id: SecurityId,
    venue_id: VenueId,
    symbol: TickerSymbol,
    role: ListingRole,
    status: ListingStatus,
    created_at: DateTime<Utc>,

    currency: Option<CurrencyCode>,
    segment: Option<MarketSegment>,
    listed_on: Option<NaiveDate>,
    delisted_on: Option<NaiveDate>,
}

#[derive(Debug)]
pub struct ListingSnapshot {
    pub id: ListingId,
    pub security_id: SecurityId,
    pub venue_id: VenueId,
    pub symbol: TickerSymbol,
    pub role: ListingRole,
    pub status: ListingStatus,
    pub created_at: DateTime<Utc>,
    pub currency: Option<CurrencyCode>,
    pub segment: Option<MarketSegment>,
    pub listed_on: Option<NaiveDate>,
    pub delisted_on: Option<NaiveDate>,
}

impl Listing {
    #[must_use]
    pub fn builder(
        security_id: SecurityId,
        venue_id: VenueId,
        symbol: TickerSymbol,
    ) -> ListingBuilder {
        ListingBuilder::new(security_id, venue_id, symbol)
    }

    #[must_use]
    pub fn id(&self) -> &ListingId {
        &self.id
    }
    #[must_use]
    pub fn security_id(&self) -> &SecurityId {
        &self.security_id
    }
    #[must_use]
    pub fn venue_id(&self) -> &VenueId {
        &self.venue_id
    }
    #[must_use]
    pub fn symbol(&self) -> &TickerSymbol {
        &self.symbol
    }
    #[must_use]
    pub fn role(&self) -> ListingRole {
        self.role
    }
    #[must_use]
    pub fn status(&self) -> ListingStatus {
        self.status
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn currency(&self) -> Option<&CurrencyCode> {
        self.currency.as_ref()
    }
    #[must_use]
    pub fn segment(&self) -> Option<&MarketSegment> {
        self.segment.as_ref()
    }
    #[must_use]
    pub fn listed_on(&self) -> Option<NaiveDate> {
        self.listed_on
    }
    #[must_use]
    pub fn delisted_on(&self) -> Option<NaiveDate> {
        self.delisted_on
    }
    #[must_use]
    pub fn reconstitute(snapshot: ListingSnapshot) -> Self {
        Self {
            id: snapshot.id,
            security_id: snapshot.security_id,
            venue_id: snapshot.venue_id,
            symbol: snapshot.symbol,
            role: snapshot.role,
            status: snapshot.status,
            created_at: snapshot.created_at,
            currency: snapshot.currency,
            segment: snapshot.segment,
            listed_on: snapshot.listed_on,
            delisted_on: snapshot.delisted_on,
        }
    }
}

pub struct ListingBuilder {
    security_id: SecurityId,
    venue_id: VenueId,
    symbol: TickerSymbol,
    id: Option<ListingId>,
    role: Option<ListingRole>,
    status: Option<ListingStatus>,
    created_at: Option<DateTime<Utc>>,
    currency: Option<CurrencyCode>,
    segment: Option<MarketSegment>,
    listed_on: Option<NaiveDate>,
    delisted_on: Option<NaiveDate>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListingBuilderError {
    #[error("A delisting date requires the DELISTED status. Found: {0}")]
    DelistedDateRequiresDelistedStatus(String),

    #[error("delisting date {delisted_on} precedes listing date {listed_on}")]
    DelistedBeforeListed {
        listed_on: NaiveDate,
        delisted_on: NaiveDate,
    },
}

impl ListingBuilder {
    #[must_use]
    pub fn new(security_id: SecurityId, venue_id: VenueId, symbol: TickerSymbol) -> Self {
        Self {
            security_id,
            venue_id,
            symbol,
            id: None,
            role: None,
            status: None,
            created_at: None,
            currency: None,
            segment: None,
            listed_on: None,
            delisted_on: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: ListingId) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn role(mut self, role: ListingRole) -> Self {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub fn status(mut self, status: ListingStatus) -> Self {
        self.status = Some(status);
        self
    }

    #[must_use]
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    #[must_use]
    pub fn currency(mut self, currency: CurrencyCode) -> Self {
        self.currency = Some(currency);
        self
    }

    #[must_use]
    pub fn segment(mut self, segment: MarketSegment) -> Self {
        self.segment = Some(segment);
        self
    }

    #[must_use]
    pub fn listed_on(mut self, listed_on: NaiveDate) -> Self {
        self.listed_on = Some(listed_on);
        self
    }

    #[must_use]
    pub fn delisted_on(mut self, delisted_on: NaiveDate) -> Self {
        self.delisted_on = Some(delisted_on);
        self
    }

    /// # Errors
    ///
    /// Returns `ListingBuilderError`.
    pub fn build(self) -> Result<Listing, ListingBuilderError> {
        let id = self.id.unwrap_or_default();
        let role = self.role.unwrap_or_default();
        let status = self.status.unwrap_or_default();
        let created_at = self.created_at.unwrap_or_else(Utc::now);

        // Cross-field validation: a delisting date implies the listing is
        // delisted (the reverse is allowed - the date may be unknown).
        if self.delisted_on.is_some() && !status.is_delisted() {
            return Err(ListingBuilderError::DelistedDateRequiresDelistedStatus(
                status.into(),
            ));
        }

        // Cross-field validation: chronology.
        if let Some(listed_on) = self.listed_on
            && let Some(delisted_on) = self.delisted_on
            && delisted_on < listed_on
        {
            return Err(ListingBuilderError::DelistedBeforeListed {
                listed_on,
                delisted_on,
            });
        }

        Ok(Listing {
            id,
            security_id: self.security_id,
            venue_id: self.venue_id,
            symbol: self.symbol,
            role,
            status,
            created_at,
            currency: self.currency,
            segment: self.segment,
            listed_on: self.listed_on,
            delisted_on: self.delisted_on,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ListingPatch {
    pub(crate) symbol: Option<TickerSymbol>,
    pub(crate) role: Option<ListingRole>,
    pub(crate) status: Option<ListingStatus>,
    pub(crate) currency: Option<CurrencyCode>,
    pub(crate) segment: Option<MarketSegment>,
    pub(crate) listed_on: Option<NaiveDate>,
    pub(crate) delisted_on: Option<NaiveDate>,
}

impl ListingPatch {
    #[must_use]
    pub const fn builder() -> ListingPatchBuilder<Empty> {
        ListingPatchBuilder::new()
    }

    const fn empty() -> Self {
        Self {
            symbol: None,
            role: None,
            status: None,
            currency: None,
            segment: None,
            listed_on: None,
            delisted_on: None,
        }
    }

    #[must_use]
    pub const fn symbol(&self) -> Option<&TickerSymbol> {
        self.symbol.as_ref()
    }

    #[must_use]
    pub const fn role(&self) -> Option<ListingRole> {
        self.role
    }

    #[must_use]
    pub const fn status(&self) -> Option<ListingStatus> {
        self.status
    }

    #[must_use]
    pub const fn currency(&self) -> Option<&CurrencyCode> {
        self.currency.as_ref()
    }

    #[must_use]
    pub const fn segment(&self) -> Option<&MarketSegment> {
        self.segment.as_ref()
    }

    #[must_use]
    pub const fn listed_on(&self) -> Option<NaiveDate> {
        self.listed_on
    }

    #[must_use]
    pub const fn delisted_on(&self) -> Option<NaiveDate> {
        self.delisted_on
    }
}

pub struct ListingPatchBuilder<State> {
    inner: ListingPatch,
    _state: PhantomData<State>,
}

impl ListingPatchBuilder<Empty> {
    const fn new() -> Self {
        Self {
            inner: ListingPatch::empty(),
            _state: PhantomData,
        }
    }
}

impl<State> ListingPatchBuilder<State> {
    #[must_use]
    pub fn symbol(self, symbol: TickerSymbol) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                symbol: Some(symbol),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn role(self, role: ListingRole) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                role: Some(role),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn status(self, status: ListingStatus) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                status: Some(status),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn currency(self, currency: CurrencyCode) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                currency: Some(currency),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn segment(self, segment: MarketSegment) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                segment: Some(segment),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn listed_on(self, listed_on: NaiveDate) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                listed_on: Some(listed_on),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn delisted_on(self, delisted_on: NaiveDate) -> ListingPatchBuilder<NonEmpty> {
        ListingPatchBuilder {
            inner: ListingPatch {
                delisted_on: Some(delisted_on),
                ..self.inner
            },
            _state: PhantomData,
        }
    }
}

impl ListingPatchBuilder<NonEmpty> {
    #[must_use]
    pub fn build(self) -> ListingPatch {
        self.inner
    }
}

// ================ LISTING REPOSITORY ================
macro_rules! delegate_listing_repository {
    ($ty:ty) => {
        impl<R: ListingRepository + ?Sized> ListingRepository for $ty {
            fn find_by_id(&self, id: &ListingId) -> RepositoryResult<Option<Versioned<Listing>>> {
                (**self).find_by_id(id)
            }
            fn find_active_by_venue_and_symbol(
                &self,
                venue_id: &VenueId,
                symbol: &TickerSymbol,
            ) -> RepositoryResult<Option<Versioned<Listing>>> {
                (**self).find_active_by_venue_and_symbol(venue_id, symbol)
            }
            fn list_all(&self) -> RepositoryResult<Vec<Versioned<Listing>>> {
                (**self).list_all()
            }
            fn list_by_security(
                &self,
                security_id: &SecurityId,
            ) -> RepositoryResult<Vec<Versioned<Listing>>> {
                (**self).list_by_security(security_id)
            }
            fn list_by_venue(
                &self,
                venue_id: &VenueId,
            ) -> RepositoryResult<Vec<Versioned<Listing>>> {
                (**self).list_by_venue(venue_id)
            }
            fn list_paged(
                &self,
                after: Option<ListingId>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<Listing>>> {
                (**self).list_paged(after, limit)
            }
            fn exists(&self, id: &ListingId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn exists_active_by_venue_and_symbol(
                &self,
                venue_id: &VenueId,
                symbol: &TickerSymbol,
            ) -> RepositoryResult<bool> {
                (**self).exists_active_by_venue_and_symbol(venue_id, symbol)
            }
            fn exists_active_for_security_on_venue(
                &self,
                security_id: &SecurityId,
                venue_id: &VenueId,
            ) -> RepositoryResult<bool> {
                (**self).exists_active_for_security_on_venue(security_id, venue_id)
            }
            fn exists_active_primary_for_security(
                &self,
                security_id: &SecurityId,
            ) -> RepositoryResult<bool> {
                (**self).exists_active_primary_for_security(security_id)
            }
            fn insert(&self, listing: &Listing) -> RepositoryResult<()> {
                (**self).insert(listing)
            }
            fn apply_patch(
                &self,
                id: &ListingId,
                expected_version: u32,
                patch: ListingPatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(
                &self,
                listing: &Listing,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(listing, expected_version)
            }
            fn delete(
                &self,
                id: &ListingId,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_listing_repository!(Box<R>);
delegate_listing_repository!(Rc<R>);
delegate_listing_repository!(Arc<R>);

#[cfg_attr(test, mockall::automock)]
pub trait ListingRepository {
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn find_by_id(&self, id: &ListingId) -> RepositoryResult<Option<Versioned<Listing>>>;

    /// Resolves an actively traded ticker on a venue, e.g. `VALE3` on B3.
    ///
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn find_active_by_venue_and_symbol(
        &self,
        venue_id: &VenueId,
        symbol: &TickerSymbol,
    ) -> RepositoryResult<Option<Versioned<Listing>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Listing>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_by_security(
        &self,
        security_id: &SecurityId,
    ) -> RepositoryResult<Vec<Versioned<Listing>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_by_venue(&self, venue_id: &VenueId) -> RepositoryResult<Vec<Versioned<Listing>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_paged(
        &self,
        after: Option<ListingId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Listing>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists(&self, id: &ListingId) -> RepositoryResult<bool>;

    /// An active listing already occupies this ticker on the venue. Delisted
    /// tickers may be recycled, hence "active".
    ///
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists_active_by_venue_and_symbol(
        &self,
        venue_id: &VenueId,
        symbol: &TickerSymbol,
    ) -> RepositoryResult<bool>;

    /// The security already has an active listing on the venue.
    ///
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists_active_for_security_on_venue(
        &self,
        security_id: &SecurityId,
        venue_id: &VenueId,
    ) -> RepositoryResult<bool>;

    /// The security already has an active primary listing (on any venue).
    ///
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists_active_primary_for_security(
        &self,
        security_id: &SecurityId,
    ) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn insert(&self, listing: &Listing) -> RepositoryResult<()>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn apply_patch(
        &self,
        id: &ListingId,
        expected_version: u32,
        patch: ListingPatch,
    ) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn update(&self, listing: &Listing, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn delete(&self, id: &ListingId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

// ================ LISTING SERVICE ================
#[derive(Debug, thiserror::Error)]
pub enum RegisterListingError {
    #[error("the referenced security does not exist")]
    UnknownSecurity,

    #[error("the referenced venue does not exist or is not active")]
    VenueNotActive,

    #[error("this ticker symbol is already actively listed on the venue")]
    TickerAlreadyListed,

    #[error("this security is already actively listed on the venue")]
    SecurityAlreadyListedOnVenue,

    #[error("this security already has an active primary listing")]
    PrimaryListingAlreadyExists,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// Registers a new listing after validating its cross-aggregate references
/// and market invariants:
///
/// 1. the security must exist;
/// 2. the venue must exist and be active;
/// 3. the ticker must not be actively listed on the venue (delisted tickers
///    may be recycled);
/// 4. the security must not already be actively listed on the venue;
/// 5. a primary listing requires the security to have no other active
///    primary listing.
///
/// # Errors
///
/// Returns `RegisterListingError`.
pub fn register_listing<L, S, V>(
    listings: &L,
    securities: &S,
    venues: &V,
    listing: &Listing,
) -> Result<(), RegisterListingError>
where
    L: ListingRepository + ?Sized,
    S: SecurityRepository + ?Sized,
    V: VenueRepository + ?Sized,
{
    if !securities.exists(listing.security_id())? {
        return Err(RegisterListingError::UnknownSecurity);
    }

    let Some(venue) = venues.find_by_id(listing.venue_id())? else {
        return Err(RegisterListingError::VenueNotActive);
    };
    if !venue.data.status().is_active() {
        return Err(RegisterListingError::VenueNotActive);
    }

    if listings.exists_active_by_venue_and_symbol(listing.venue_id(), listing.symbol())? {
        return Err(RegisterListingError::TickerAlreadyListed);
    }

    if listings.exists_active_for_security_on_venue(listing.security_id(), listing.venue_id())? {
        return Err(RegisterListingError::SecurityAlreadyListedOnVenue);
    }

    if listing.role().is_primary()
        && listings.exists_active_primary_for_security(listing.security_id())?
    {
        return Err(RegisterListingError::PrimaryListingAlreadyExists);
    }

    listings.insert(listing)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(value: &str) -> Option<TickerSymbol> {
        TickerSymbol::new(value).ok()
    }

    fn date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(year, month, day)
    }

    #[test]
    fn ticker_symbol_trims_and_normalizes_to_uppercase() {
        let symbol_result = TickerSymbol::new(" vale3 ");
        assert!(symbol_result.is_ok());
        let Some(symbol) = symbol_result.ok() else {
            return;
        };
        assert_eq!(symbol.as_str(), "VALE3");
    }

    #[test]
    fn ticker_symbol_accepts_class_separators() {
        assert!(matches!(
            TickerSymbol::new("BRK.B"),
            Ok(s) if s.as_str() == "BRK.B"
        ));
        assert!(matches!(
            TickerSymbol::new("bf-b"),
            Ok(s) if s.as_str() == "BF-B"
        ));
    }

    #[test]
    fn ticker_symbol_rejects_empty() {
        assert!(matches!(
            TickerSymbol::new("  "),
            Err(TickerSymbolError::Empty)
        ));
    }

    #[test]
    fn ticker_symbol_rejects_too_long() {
        let long_symbol = "A".repeat(TICKER_SYMBOL_MAX_LEN.saturating_add(1));
        assert!(matches!(
            TickerSymbol::new(long_symbol),
            Err(TickerSymbolError::TooLong { max: 12 })
        ));
    }

    #[test]
    fn ticker_symbol_rejects_invalid_characters() {
        assert!(matches!(
            TickerSymbol::new("VALE 3"),
            Err(TickerSymbolError::InvalidCharacter)
        ));
        assert!(matches!(
            TickerSymbol::new("VALÉ3"),
            Err(TickerSymbolError::InvalidCharacter)
        ));
    }

    #[test]
    fn market_segment_trims_and_validates() {
        let segment_result = MarketSegment::new(" Novo Mercado ");
        assert!(segment_result.is_ok());
        let Some(segment) = segment_result.ok() else {
            return;
        };
        assert_eq!(segment.as_str(), "Novo Mercado");

        assert!(matches!(
            MarketSegment::new("  "),
            Err(MarketSegmentError::Empty)
        ));
        let long_segment = "A".repeat(MARKET_SEGMENT_MAX_LEN.saturating_add(1));
        assert!(matches!(
            MarketSegment::new(long_segment),
            Err(MarketSegmentError::TooLong { max: 100 })
        ));
    }

    #[test]
    fn listing_id_creation_and_conversions() {
        let original_uuid = Uuid::now_v7();
        let id = ListingId::from_uuid(original_uuid);

        assert_eq!(id.as_uuid(), &original_uuid);
        assert_eq!(id.value(), original_uuid.to_string());
        assert_eq!(id.as_bytes(), original_uuid.as_bytes());
        assert_ne!(ListingId::new(), ListingId::new());
    }

    #[test]
    fn listing_role_round_trips() {
        assert!(ListingRole::default().is_primary());

        let primary_str: String = ListingRole::Primary.into();
        assert_eq!(primary_str, "PRIMARY");
        let secondary_str: String = ListingRole::Secondary.into();
        assert_eq!(secondary_str, "SECONDARY");

        assert!(matches!(
            ListingRole::from_str("secondary"),
            Ok(ListingRole::Secondary)
        ));
        assert!(matches!(
            ListingRole::from_str("TERTIARY"),
            Err(ListingRoleError::InvalidRole)
        ));
    }

    #[test]
    fn listing_status_round_trips() {
        assert!(ListingStatus::default().is_active());

        for (status, canonical) in [
            (ListingStatus::Active, "ACTIVE"),
            (ListingStatus::Suspended, "SUSPENDED"),
            (ListingStatus::Delisted, "DELISTED"),
        ] {
            let as_string: String = status.into();
            assert_eq!(as_string, canonical);
            assert!(matches!(
                ListingStatus::from_str(canonical),
                Ok(parsed) if parsed == status
            ));
        }

        assert!(matches!(
            ListingStatus::from_str("HALTED"),
            Err(ListingStatusError::InvalidStatus)
        ));
    }

    #[test]
    fn builder_resolves_defaults() {
        let Some(vale3) = symbol("VALE3") else {
            return;
        };
        let listing_result = Listing::builder(SecurityId::new(), VenueId::new(), vale3).build();
        assert!(listing_result.is_ok());
        let Some(listing) = listing_result.ok() else {
            return;
        };

        assert_eq!(listing.symbol().as_str(), "VALE3");
        assert!(listing.role().is_primary());
        assert!(listing.status().is_active());
        assert!(listing.currency().is_none());
        assert!(listing.segment().is_none());
        assert!(listing.listed_on().is_none());
        assert!(listing.delisted_on().is_none());
        assert!(listing.created_at() <= Utc::now());
    }

    #[test]
    fn builder_accepts_full_brazilian_listing() {
        let Some(vale3) = symbol("VALE3") else {
            return;
        };
        let currency_result = CurrencyCode::parse("BRL");
        assert!(currency_result.is_ok());
        let Some(brl) = currency_result.ok() else {
            return;
        };
        let segment_result = MarketSegment::new("Novo Mercado");
        assert!(segment_result.is_ok());
        let Some(novo_mercado) = segment_result.ok() else {
            return;
        };
        let Some(listed_on) = date(2008, 5, 12) else {
            return;
        };

        let listing_result = Listing::builder(SecurityId::new(), VenueId::new(), vale3)
            .role(ListingRole::Primary)
            .currency(brl)
            .segment(novo_mercado)
            .listed_on(listed_on)
            .build();
        assert!(listing_result.is_ok());
        let Some(listing) = listing_result.ok() else {
            return;
        };

        assert!(matches!(listing.currency(), Some(c) if c.as_str() == "BRL"));
        assert!(matches!(listing.segment(), Some(s) if s.as_str() == "Novo Mercado"));
        assert_eq!(listing.listed_on(), Some(listed_on));
    }

    #[test]
    fn builder_rejects_delisted_date_without_delisted_status() {
        let Some(vale3) = symbol("VALE3") else {
            return;
        };
        let Some(delisted_on) = date(2020, 1, 2) else {
            return;
        };

        let result = Listing::builder(SecurityId::new(), VenueId::new(), vale3)
            .delisted_on(delisted_on)
            .build();

        assert!(
            matches!(
                result,
                Err(ListingBuilderError::DelistedDateRequiresDelistedStatus(status))
                    if status == "ACTIVE"
            ),
            "A delisting date requires the DELISTED status"
        );
    }

    #[test]
    fn builder_accepts_delisted_listing_with_dates_in_order() {
        let Some(vale3) = symbol("VALE3") else {
            return;
        };
        let Some(listed_on) = date(2008, 5, 12) else {
            return;
        };
        let Some(delisted_on) = date(2020, 1, 2) else {
            return;
        };

        let listing_result = Listing::builder(SecurityId::new(), VenueId::new(), vale3)
            .status(ListingStatus::Delisted)
            .listed_on(listed_on)
            .delisted_on(delisted_on)
            .build();

        assert!(listing_result.is_ok());
    }

    #[test]
    fn builder_rejects_delisting_before_listing() {
        let Some(vale3) = symbol("VALE3") else {
            return;
        };
        let Some(listed_on) = date(2020, 1, 2) else {
            return;
        };
        let Some(delisted_on) = date(2008, 5, 12) else {
            return;
        };

        let result = Listing::builder(SecurityId::new(), VenueId::new(), vale3)
            .status(ListingStatus::Delisted)
            .listed_on(listed_on)
            .delisted_on(delisted_on)
            .build();

        assert!(matches!(
            result,
            Err(ListingBuilderError::DelistedBeforeListed { .. })
        ));
    }

    #[test]
    fn single_field_patch_only_sets_that_field() {
        let symbol_result = TickerSymbol::new("META");
        assert!(symbol_result.is_ok());
        let Some(symbol) = symbol_result.ok() else {
            return;
        };
        let patch = ListingPatch::builder().symbol(symbol.clone()).build();

        assert_eq!(patch.symbol, Some(symbol));
        assert!(patch.role.is_none());
        assert!(patch.status.is_none());
        assert!(patch.currency.is_none());
        assert!(patch.segment.is_none());
        assert!(patch.listed_on.is_none());
        assert!(patch.delisted_on.is_none());
    }
}

#[cfg(test)]
mod tests_service {
    use super::*;
    use crate::{
        common::Versioned,
        domain::{
            listing::{ListingRole, TickerSymbol},
            security::{MockSecurityRepository, SecurityId},
            venue::{MockVenueRepository, Venue, VenueId, VenueStatus},
        },
    };
    use valqeron_identifiers::Mic;

    fn vale3_listing(role: ListingRole) -> Option<Listing> {
        let symbol = TickerSymbol::new("VALE3").ok()?;
        Listing::builder(SecurityId::new(), VenueId::new(), symbol)
            .role(role)
            .build()
            .ok()
    }

    fn securities_with_existing_security() -> MockSecurityRepository {
        let mut securities = MockSecurityRepository::new();
        securities.expect_exists().returning(|_| Ok(true));
        securities
    }

    fn venues_returning_status(status: VenueStatus) -> MockVenueRepository {
        let mut venues = MockVenueRepository::new();
        venues.expect_find_by_id().returning(move |_| {
            let Some(mic) = Mic::parse("BVMF").ok() else {
                return Ok(None);
            };
            Ok(Some(Versioned {
                data: Venue::builder(mic).status(status).build(),
                version: 1,
            }))
        });
        venues
    }

    #[test]
    fn register_inserts_when_all_invariants_hold() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Active);

        let mut listings = MockListingRepository::new();
        listings
            .expect_exists_active_by_venue_and_symbol()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_for_security_on_venue()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_primary_for_security()
            .returning(|_| Ok(false));
        listings.expect_insert().returning(|_| Ok(()));

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(register_listing(&listings, &securities, &venues, &listing).is_ok());
    }

    #[test]
    fn register_rejects_unknown_security_before_insert() {
        let mut securities = MockSecurityRepository::new();
        securities.expect_exists().returning(|_| Ok(false));
        let venues = MockVenueRepository::new();

        let mut listings = MockListingRepository::new();
        // insert must never be called for an unknown security.
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::UnknownSecurity)
        ));
    }

    #[test]
    fn register_rejects_missing_venue_before_insert() {
        let securities = securities_with_existing_security();
        let mut venues = MockVenueRepository::new();
        venues.expect_find_by_id().returning(|_| Ok(None));

        let mut listings = MockListingRepository::new();
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::VenueNotActive)
        ));
    }

    #[test]
    fn register_rejects_retired_venue_before_insert() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Retired);

        let mut listings = MockListingRepository::new();
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::VenueNotActive)
        ));
    }

    #[test]
    fn register_rejects_taken_ticker_before_insert() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Active);

        let mut listings = MockListingRepository::new();
        listings
            .expect_exists_active_by_venue_and_symbol()
            .returning(|_, _| Ok(true));
        // insert must never be called when the ticker is taken.
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::TickerAlreadyListed)
        ));
    }

    #[test]
    fn register_rejects_security_already_listed_on_venue() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Active);

        let mut listings = MockListingRepository::new();
        listings
            .expect_exists_active_by_venue_and_symbol()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_for_security_on_venue()
            .returning(|_, _| Ok(true));
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::SecurityAlreadyListedOnVenue)
        ));
    }

    #[test]
    fn register_rejects_second_active_primary_listing() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Active);

        let mut listings = MockListingRepository::new();
        listings
            .expect_exists_active_by_venue_and_symbol()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_for_security_on_venue()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_primary_for_security()
            .returning(|_| Ok(true));
        listings.expect_insert().never();

        let Some(listing) = vale3_listing(ListingRole::Primary) else {
            return;
        };
        assert!(matches!(
            register_listing(&listings, &securities, &venues, &listing),
            Err(RegisterListingError::PrimaryListingAlreadyExists)
        ));
    }

    #[test]
    fn register_allows_secondary_listing_when_primary_exists() {
        let securities = securities_with_existing_security();
        let venues = venues_returning_status(VenueStatus::Active);

        let mut listings = MockListingRepository::new();
        listings
            .expect_exists_active_by_venue_and_symbol()
            .returning(|_, _| Ok(false));
        listings
            .expect_exists_active_for_security_on_venue()
            .returning(|_, _| Ok(false));
        // The primary-uniqueness check must not run for secondary listings.
        listings.expect_exists_active_primary_for_security().never();
        listings.expect_insert().returning(|_| Ok(()));

        let Some(listing) = vale3_listing(ListingRole::Secondary) else {
            return;
        };
        assert!(register_listing(&listings, &securities, &venues, &listing).is_ok());
    }
}

#[cfg(test)]
mod tests_currency_code {
    use crate::domain::listing::{CurrencyCode, CurrencyCodeError};

    #[test]
    fn currency_code_parses_and_normalizes() {
        let currency_result = CurrencyCode::parse(" brl ");
        assert!(currency_result.is_ok());
        let Some(currency) = currency_result.ok() else {
            return;
        };
        assert_eq!(currency.as_str(), "BRL");
        assert_eq!(currency.as_bytes(), b"BRL");
        assert_eq!(currency.to_string(), "BRL");
    }

    #[test]
    fn currency_code_rejects_empty() {
        assert!(matches!(
            CurrencyCode::parse(""),
            Err(CurrencyCodeError::Empty)
        ));
    }

    #[test]
    fn currency_code_rejects_wrong_length() {
        assert!(matches!(
            CurrencyCode::parse("US"),
            Err(CurrencyCodeError::InvalidLength)
        ));
        assert!(matches!(
            CurrencyCode::parse("USDT"),
            Err(CurrencyCodeError::InvalidLength)
        ));
    }

    #[test]
    fn currency_code_rejects_non_letters() {
        assert!(matches!(
            CurrencyCode::parse("US1"),
            Err(CurrencyCodeError::InvalidCharacter)
        ));
    }

    #[test]
    fn currency_code_from_str_round_trips() {
        let currency_result = "usd".parse::<CurrencyCode>();
        assert!(matches!(currency_result, Ok(c) if c.as_str() == "USD"));
    }
}
