use crate::identifiers::{Cnpj, CountryCode, CountryCodeError, Lei};
use crate::{
    StorageFault,
    common::{Empty, LoadMode, Loading, NonEmpty, RepositoryResult, Versioned, WriteOutcome},
    domain::security::Security,
};
use chrono::{DateTime, Utc};
use std::marker::PhantomData;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

const ISSUER_NAME_MAX_LEN: usize = 200;
const BRAZIL_COUNTRY_CODE: &str = "BR";

// ================ ISSUER DOMAIN ================
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct IssuerName(String);

#[derive(thiserror::Error, Debug)]
pub enum IssuerNameError {
    #[error("issuer name cannot be empty")]
    Empty,

    #[error("issuer name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl IssuerName {
    /// # Errors
    ///
    /// Returns `IssuerNameError`.
    pub fn new(value: impl Into<String>) -> Result<Self, IssuerNameError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(IssuerNameError::Empty);
        }
        if trimmed.chars().count() > ISSUER_NAME_MAX_LEN {
            return Err(IssuerNameError::TooLong {
                max: ISSUER_NAME_MAX_LEN,
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
pub struct IssuerId(Uuid);

impl IssuerId {
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

impl Default for IssuerId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IssuerStatus {
    #[default]
    Active,
    Retired,
}

#[derive(thiserror::Error, Debug)]
pub enum IssuerStatusError {
    #[error("Invalid status. Must be one of: {statuses:?}", statuses = vec!["ACTIVE", "RETIRED"])]
    InvalidStatus,
}

impl IssuerStatus {
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, IssuerStatus::Active)
    }

    #[must_use]
    pub fn is_retired(&self) -> bool {
        matches!(self, IssuerStatus::Retired)
    }
}

impl FromStr for IssuerStatus {
    type Err = IssuerStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Ok(IssuerStatus::Active),
            "RETIRED" => Ok(IssuerStatus::Retired),
            _ => Err(IssuerStatusError::InvalidStatus),
        }
    }
}

impl From<IssuerStatus> for String {
    fn from(val: IssuerStatus) -> Self {
        match val {
            IssuerStatus::Active => "ACTIVE".into(),
            IssuerStatus::Retired => "RETIRED".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Issuer {
    id: IssuerId,
    status: IssuerStatus,
    created_at: DateTime<Utc>,

    name: Option<IssuerName>,
    cnpj: Option<Cnpj>,
    lei: Option<Lei>,
    country_code: Option<CountryCode>,

    securities: Loading<Vec<Security>>,
}

#[derive(Debug)]
pub struct IssuerSnapshot {
    pub id: IssuerId,
    pub status: IssuerStatus,
    pub created_at: DateTime<Utc>,
    pub name: Option<IssuerName>,
    pub cnpj: Option<Cnpj>,
    pub lei: Option<Lei>,
    pub country_code: Option<CountryCode>,
    pub securities: Loading<Vec<Security>>,
}

impl Issuer {
    #[must_use]
    pub fn builder() -> IssuerBuilder {
        IssuerBuilder::new()
    }

    #[must_use]
    pub fn id(&self) -> &IssuerId {
        &self.id
    }
    #[must_use]
    pub fn status(&self) -> IssuerStatus {
        self.status
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn name(&self) -> Option<&IssuerName> {
        self.name.as_ref()
    }
    #[must_use]
    pub fn cnpj(&self) -> Option<&Cnpj> {
        self.cnpj.as_ref()
    }
    #[must_use]
    pub fn lei(&self) -> Option<&Lei> {
        self.lei.as_ref()
    }
    #[must_use]
    pub fn country_code(&self) -> Option<&CountryCode> {
        self.country_code.as_ref()
    }

    #[must_use]
    pub fn securities(&self) -> Option<&[Security]> {
        self.securities.as_loaded().map(Vec::as_slice)
    }

    #[must_use]
    pub fn reconstitute(snapshot: IssuerSnapshot) -> Self {
        Self {
            id: snapshot.id,
            status: snapshot.status,
            created_at: snapshot.created_at,
            name: snapshot.name,
            cnpj: snapshot.cnpj,
            lei: snapshot.lei,
            country_code: snapshot.country_code,
            securities: snapshot.securities,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IssuerBuilderError {
    #[error("If a CNPJ is provided, the country code must be BR (Brazil). Found: {0}")]
    InvalidCountryForCnpj(String),

    #[error("Issuer name validation failed: {0}")]
    NameError(#[from] IssuerNameError),

    #[error("country code validation failed: {0}")]
    CountryCodeError(#[from] CountryCodeError),
}

#[derive(Default)]
pub struct IssuerBuilder {
    id: Option<IssuerId>,
    status: Option<IssuerStatus>,
    created_at: Option<DateTime<Utc>>,
    name: Option<IssuerName>,
    cnpj: Option<Cnpj>,
    lei: Option<Lei>,
    country_code: Option<CountryCode>,
}

impl IssuerBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn id(mut self, id: IssuerId) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn status(mut self, status: IssuerStatus) -> Self {
        self.status = Some(status);
        self
    }

    #[must_use]
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    #[must_use]
    pub fn name(mut self, name: IssuerName) -> Self {
        self.name = Some(name);
        self
    }

    #[must_use]
    pub fn cnpj(mut self, cnpj: Cnpj) -> Self {
        self.cnpj = Some(cnpj);
        self
    }

    #[must_use]
    pub fn lei(mut self, lei: Lei) -> Self {
        self.lei = Some(lei);
        self
    }

    #[must_use]
    pub fn country_code(mut self, country_code: CountryCode) -> Self {
        self.country_code = Some(country_code);
        self
    }

    /// # Errors
    ///
    /// Returns `IssuerBuilderError`.
    pub fn build(self) -> Result<Issuer, IssuerBuilderError> {
        let id = self.id.unwrap_or_default();
        let status = self.status.unwrap_or_default();
        let created_at = self.created_at.unwrap_or_else(Utc::now);

        let mut country_code = self.country_code;

        // Cross-field Validation: CNPJ implies BR
        if self.cnpj.is_some() {
            match &country_code {
                Some(code) if code.as_str() != BRAZIL_COUNTRY_CODE => {
                    return Err(IssuerBuilderError::InvalidCountryForCnpj(
                        code.as_str().to_string(),
                    ));
                }
                None => {
                    country_code = Some(CountryCode::from_str(BRAZIL_COUNTRY_CODE)?);
                }
                _ => {}
            }
        }

        Ok(Issuer {
            id,
            status,
            created_at,
            name: self.name,
            cnpj: self.cnpj,
            lei: self.lei,
            country_code,
            // A freshly registered issuer factually has no securities yet.
            securities: Loading::Loaded(Vec::new()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct IssuerPatch {
    pub(crate) name: Option<IssuerName>,
    pub(crate) status: Option<IssuerStatus>,
    pub(crate) cnpj: Option<Cnpj>,
    pub(crate) lei: Option<Lei>,
    pub(crate) country_code: Option<CountryCode>,
}

impl IssuerPatch {
    #[must_use]
    pub const fn builder() -> IssuerPatchBuilder<Empty> {
        IssuerPatchBuilder::new()
    }

    const fn empty() -> Self {
        Self {
            name: None,
            status: None,
            cnpj: None,
            lei: None,
            country_code: None,
        }
    }

    #[must_use]
    pub const fn name(&self) -> Option<&IssuerName> {
        self.name.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> Option<IssuerStatus> {
        self.status
    }

    #[must_use]
    pub const fn cnpj(&self) -> Option<&Cnpj> {
        self.cnpj.as_ref()
    }

    #[must_use]
    pub const fn lei(&self) -> Option<&Lei> {
        self.lei.as_ref()
    }

    #[must_use]
    pub const fn country_code(&self) -> Option<&CountryCode> {
        self.country_code.as_ref()
    }
}

pub struct IssuerPatchBuilder<State> {
    inner: IssuerPatch,
    _state: PhantomData<State>,
}

impl IssuerPatchBuilder<Empty> {
    const fn new() -> Self {
        Self {
            inner: IssuerPatch::empty(),
            _state: PhantomData,
        }
    }
}

impl<State> IssuerPatchBuilder<State> {
    #[must_use]
    pub fn name(self, name: IssuerName) -> IssuerPatchBuilder<NonEmpty> {
        IssuerPatchBuilder {
            inner: IssuerPatch {
                name: Some(name),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn status(self, status: IssuerStatus) -> IssuerPatchBuilder<NonEmpty> {
        IssuerPatchBuilder {
            inner: IssuerPatch {
                status: Some(status),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn cnpj(self, cnpj: Cnpj) -> IssuerPatchBuilder<NonEmpty> {
        IssuerPatchBuilder {
            inner: IssuerPatch {
                cnpj: Some(cnpj),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn lei(self, lei: Lei) -> IssuerPatchBuilder<NonEmpty> {
        IssuerPatchBuilder {
            inner: IssuerPatch {
                lei: Some(lei),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn country_code(self, country_code: CountryCode) -> IssuerPatchBuilder<NonEmpty> {
        IssuerPatchBuilder {
            inner: IssuerPatch {
                country_code: Some(country_code),
                ..self.inner
            },
            _state: PhantomData,
        }
    }
}

impl IssuerPatchBuilder<NonEmpty> {
    #[must_use]
    pub fn build(self) -> IssuerPatch {
        self.inner
    }
}

// ================ ISSUER REPOSITORY ================
macro_rules! delegate_issuer_repository {
    ($ty:ty) => {
        impl<R: IssuerRepository + ?Sized> IssuerRepository for $ty {
            fn find_by_id(
                &self,
                id: &IssuerId,
                mode: LoadMode,
            ) -> RepositoryResult<Option<Versioned<Issuer>>> {
                (**self).find_by_id(id, mode)
            }
            fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_all(mode)
            }
            fn list_paged(
                &self,
                after: Option<IssuerId>,
                limit: u32,
                mode: LoadMode,
            ) -> RepositoryResult<Vec<Versioned<Issuer>>> {
                (**self).list_paged(after, limit, mode)
            }
            fn exists(&self, id: &IssuerId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool> {
                (**self).exists_by_cnpj(cnpj)
            }
            fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool> {
                (**self).exists_by_lei(lei)
            }
            fn insert(&self, issuer: &Issuer) -> RepositoryResult<()> {
                (**self).insert(issuer)
            }
            fn apply_patch(
                &self,
                id: &IssuerId,
                expected_version: u32,
                patch: IssuerPatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(
                &self,
                issuer: &Issuer,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(issuer, expected_version)
            }
            fn delete(
                &self,
                id: &IssuerId,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_issuer_repository!(Box<R>);
delegate_issuer_repository!(Rc<R>);
delegate_issuer_repository!(Arc<R>);

#[cfg_attr(test, mockall::automock)]
pub trait IssuerRepository {
    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn find_by_id(
        &self,
        id: &IssuerId,
        mode: LoadMode,
    ) -> RepositoryResult<Option<Versioned<Issuer>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_all(&self, mode: LoadMode) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn list_paged(
        &self,
        after: Option<IssuerId>,
        limit: u32,
        mode: LoadMode,
    ) -> RepositoryResult<Vec<Versioned<Issuer>>>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists(&self, id: &IssuerId) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists_by_cnpj(&self, cnpj: &Cnpj) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn exists_by_lei(&self, lei: &Lei) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn insert(&self, issuer: &Issuer) -> RepositoryResult<()>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn apply_patch(
        &self,
        id: &IssuerId,
        expected_version: u32,
        patch: IssuerPatch,
    ) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn update(&self, issuer: &Issuer, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Returns `StorageFault`.
    fn delete(&self, id: &IssuerId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

// ================ ISSUER SERVICE ================
#[derive(Debug, thiserror::Error)]
pub enum RegisterIssuerError {
    #[error("an issuer with this CNPJ already exists")]
    DuplicateCnpj,

    #[error("an issuer with this LEI already exists")]
    DuplicateLei,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Returns `RegisterIssuerError`.
pub fn register_issuer<R: IssuerRepository + ?Sized>(
    repo: &R,
    issuer: &Issuer,
) -> Result<(), RegisterIssuerError> {
    if let Some(cnpj) = issuer.cnpj()
        && repo.exists_by_cnpj(cnpj)?
    {
        return Err(RegisterIssuerError::DuplicateCnpj);
    }

    if let Some(lei) = issuer.lei()
        && repo.exists_by_lei(lei)?
    {
        return Err(RegisterIssuerError::DuplicateLei);
    }

    repo.insert(issuer)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum GetIssuerError {
    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Returns `GetIssuerError`.
pub fn get_issuer<R: IssuerRepository + ?Sized>(
    repo: &R,
    id: &IssuerId,
    mode: LoadMode,
) -> Result<Option<Versioned<Issuer>>, GetIssuerError> {
    Ok(repo.find_by_id(id, mode)?)
}

#[derive(Debug, thiserror::Error)]
pub enum ListIssuersError {
    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Returns `ListIssuersError`.
pub fn list_issuers<R: IssuerRepository + ?Sized>(
    repo: &R,
    after: Option<IssuerId>,
    limit: u32,
    mode: LoadMode,
) -> Result<Vec<Versioned<Issuer>>, ListIssuersError> {
    Ok(repo.list_paged(after, limit, mode)?)
}

#[derive(Debug, thiserror::Error)]
pub enum PatchIssuerError {
    #[error("an issuer with this CNPJ already exists")]
    DuplicateCnpj,

    #[error("an issuer with this LEI already exists")]
    DuplicateLei,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Returns `PatchIssuerError`.
pub fn patch_issuer<R: IssuerRepository + ?Sized>(
    repo: &R,
    id: &IssuerId,
    expected_version: u32,
    patch: IssuerPatch,
) -> Result<WriteOutcome, PatchIssuerError> {
    // Optional domain-level guardrails before hitting the repo
    if let Some(cnpj) = patch.cnpj()
        && repo.exists_by_cnpj(cnpj)?
    {
        return Err(PatchIssuerError::DuplicateCnpj);
    }

    if let Some(lei) = patch.lei()
        && repo.exists_by_lei(lei)?
    {
        return Err(PatchIssuerError::DuplicateLei);
    }

    Ok(repo.apply_patch(id, expected_version, patch)?)
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteIssuerError {
    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Returns `DeleteIssuerError`.
pub fn delete_issuer<R: IssuerRepository + ?Sized>(
    repo: &R,
    id: &IssuerId,
    expected_version: u32,
) -> Result<WriteOutcome, DeleteIssuerError> {
    Ok(repo.delete(id, expected_version)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const US_COUNTRY_CODE: &str = "US";

    #[test]
    fn test_issuer_name_valid() {
        let name_result = IssuerName::new(" Acme Corp ");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        // Ensure it trims whitespace automatically
        assert_eq!(name.as_str(), "Acme Corp");
    }

    #[test]
    fn test_issuer_name_empty_fails() {
        let result = IssuerName::new("   ");
        assert!(matches!(result, Err(IssuerNameError::Empty)));
    }

    #[test]
    fn test_issuer_name_too_long_fails() {
        let long_string = "A".repeat(ISSUER_NAME_MAX_LEN + 1);
        let result = IssuerName::new(long_string);
        assert!(matches!(result, Err(IssuerNameError::TooLong { max: 200 })));
    }

    #[test]
    fn test_issuer_name_exact_max_len_succeeds() {
        let max_string = "A".repeat(ISSUER_NAME_MAX_LEN);
        let result = IssuerName::new(max_string);
        assert!(result.is_ok());
    }

    #[test]
    fn test_issuer_id_creation() {
        let id1 = IssuerId::new();
        let id2 = IssuerId::new();
        assert_ne!(id1, id2, "UUIDs should be unique");
    }

    #[test]
    fn test_issuer_id_conversions() {
        let original_uuid = Uuid::now_v7();
        let id = IssuerId::from_uuid(original_uuid);

        assert_eq!(id.as_uuid(), &original_uuid);
        assert_eq!(id.value(), original_uuid.to_string());
        assert_eq!(id.as_bytes(), original_uuid.as_bytes());
    }

    #[test]
    fn test_issuer_status_traits() {
        assert!(IssuerStatus::default().is_active());

        let active_str: String = IssuerStatus::Active.into();
        assert_eq!(active_str, "ACTIVE");

        let retired_str: String = IssuerStatus::Retired.into();
        assert_eq!(retired_str, "RETIRED");
    }

    #[test]
    fn test_issuer_status_from_str() {
        assert!(matches!(
            IssuerStatus::from_str("ACTIVE"),
            Ok(IssuerStatus::Active)
        ));
        assert!(matches!(
            IssuerStatus::from_str("active"),
            Ok(IssuerStatus::Active)
        ));
        assert!(matches!(
            IssuerStatus::from_str("Retired"),
            Ok(IssuerStatus::Retired)
        ));

        assert!(matches!(
            IssuerStatus::from_str("UNKNOWN"),
            Err(IssuerStatusError::InvalidStatus)
        ));
    }

    #[test]
    fn test_builder_resolves_defaults() {
        let issuer_result = Issuer::builder().build();
        assert!(issuer_result.is_ok());
        let Some(issuer) = issuer_result.ok() else {
            return;
        };

        assert!(issuer.status.is_active(), "Default status should be Active");
        assert!(issuer.name.is_none());
        assert!(issuer.cnpj.is_none());
        assert!(issuer.lei.is_none());
        assert!(issuer.country_code.is_none());
        assert!(issuer.created_at <= Utc::now());
        assert!(
            matches!(issuer.securities(), Some(securities) if securities.is_empty()),
            "A newly built issuer is known to have no securities"
        );
    }

    #[test]
    fn test_reconstitute_from_snapshot_defaults_to_not_loaded() {
        let snapshot = IssuerSnapshot {
            id: IssuerId::new(),
            status: IssuerStatus::Active,
            created_at: Utc::now(),
            name: None,
            cnpj: None,
            lei: None,
            country_code: None,
            securities: Loading::NotLoaded,
        };
        let issuer = Issuer::reconstitute(snapshot);

        assert!(
            issuer.securities().is_none(),
            "Lazy reconstitution must not pretend the relation is empty"
        );
    }

    #[test]
    fn test_builder_with_all_valid_fields() {
        let custom_time = Utc::now();
        let name_result = IssuerName::new("Tech Global");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        let lei_result = Lei::new("5493000IBP32UQZ0KL24");
        assert!(lei_result.is_ok());
        let Some(lei) = lei_result.ok() else {
            return;
        };
        let country_result = CountryCode::from_str(US_COUNTRY_CODE);
        assert!(country_result.is_ok());
        let Some(country) = country_result.ok() else {
            return;
        };

        let issuer_result = Issuer::builder()
            .status(IssuerStatus::Retired)
            .created_at(custom_time)
            .name(name.clone())
            .lei(lei)
            .country_code(country)
            .build();
        assert!(issuer_result.is_ok());
        let Some(issuer) = issuer_result.ok() else {
            return;
        };

        assert!(issuer.status.is_retired());
        assert_eq!(issuer.created_at, custom_time);
        assert!(matches!(issuer.name.as_ref(), Some(value) if value == &name));
        assert!(
            matches!(issuer.country_code, Some(country) if country.as_str() == US_COUNTRY_CODE)
        );
    }

    #[test]
    fn test_builder_cnpj_validation_success_without_country() {
        let cnpj_result = Cnpj::new("12.345.678/0001-95");
        assert!(cnpj_result.is_ok());
        let Some(cnpj) = cnpj_result.ok() else {
            return;
        };

        let issuer_result = Issuer::builder().cnpj(cnpj).build();
        assert!(issuer_result.is_ok());
        let Some(issuer) = issuer_result.ok() else {
            return;
        };

        assert!(matches!(issuer.country_code, Some(country) if country.as_str() == "BR"));
    }

    #[test]
    fn test_builder_cnpj_validation_success_with_br() {
        let cnpj_result = Cnpj::new("12.345.678/0001-95");
        assert!(cnpj_result.is_ok());
        let Some(cnpj) = cnpj_result.ok() else {
            return;
        };
        let country_result = CountryCode::from_str("BR");
        assert!(country_result.is_ok());
        let Some(country_br) = country_result.ok() else {
            return;
        };

        let issuer_result = Issuer::builder()
            .cnpj(cnpj)
            .country_code(country_br)
            .build();
        assert!(issuer_result.is_ok());
        let Some(issuer) = issuer_result.ok() else {
            return;
        };

        assert!(matches!(issuer.country_code, Some(country) if country.as_str() == "BR"));
    }

    #[test]
    fn test_invalid_country_code_is_propagated_as_builder_error() {
        let country_code_result = CountryCode::from_str("ZZ");
        assert!(country_code_result.is_err());
        let Some(country_code_error) = country_code_result.err() else {
            return;
        };
        let builder_error: IssuerBuilderError = country_code_error.into();

        assert!(matches!(
            builder_error,
            IssuerBuilderError::CountryCodeError(_)
        ));
    }

    #[test]
    fn test_builder_cnpj_validation_fails_with_foreign_country() {
        let cnpj_result = Cnpj::new("12.345.678/0001-95");
        assert!(cnpj_result.is_ok());
        let Some(cnpj) = cnpj_result.ok() else {
            return;
        };
        let country_us_result = CountryCode::from_str(US_COUNTRY_CODE);
        assert!(country_us_result.is_ok());
        let Some(country_us) = country_us_result.ok() else {
            return;
        };

        let result = Issuer::builder()
            .cnpj(cnpj)
            .country_code(country_us)
            .build();

        assert!(
            matches!(result, Err(IssuerBuilderError::InvalidCountryForCnpj(code)) if code == "US"),
            "Should reject non-BR country codes when a CNPJ is present"
        );
    }

    #[test]
    fn single_field_patch_only_sets_that_field() {
        let name_result = IssuerName::new("Renamed Corp");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        let patch = IssuerPatch::builder().name(name.clone()).build();

        assert_eq!(patch.name, Some(name));
        assert!(patch.status.is_none());
        assert!(patch.cnpj.is_none());
        assert!(patch.lei.is_none());
        assert!(patch.country_code.is_none());
    }
}

#[cfg(test)]
mod tests_service {
    use super::*;

    fn issuer_with_cnpj() -> Option<Issuer> {
        let cnpj_result = Cnpj::new("12.345.678/0001-95");
        let cnpj = cnpj_result.ok()?;
        Issuer::builder().cnpj(cnpj).build().ok()
    }

    #[test]
    fn register_inserts_when_identifiers_are_free() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(false));
        repo.expect_exists_by_lei().returning(|_| Ok(false));
        repo.expect_insert().returning(|_| Ok(()));

        let Some(issuer) = issuer_with_cnpj() else {
            return;
        };
        assert!(register_issuer(&repo, &issuer).is_ok());
    }

    #[test]
    fn register_rejects_duplicate_cnpj_before_insert() {
        let mut repo = MockIssuerRepository::new();
        repo.expect_exists_by_cnpj().returning(|_| Ok(true));
        // insert must never be called on a duplicate.
        repo.expect_insert().never();

        let Some(issuer) = issuer_with_cnpj() else {
            return;
        };
        assert!(matches!(
            register_issuer(&repo, &issuer),
            Err(RegisterIssuerError::DuplicateCnpj)
        ));
    }
}
