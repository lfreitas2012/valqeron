use crate::common::Loading;
use crate::issuer::error::{IssuerBuilderError, IssuerNameError, IssuerStatusError};
use crate::security::Security;
use chrono::{DateTime, Utc};
use std::str::FromStr;
use uuid::Uuid;
use valqeron_identifiers::{Cnpj, CountryCode, Lei};

pub mod error;
pub mod patch;
pub mod repository;
pub mod service;

const ISSUER_NAME_MAX_LEN: usize = 200;
const BRAZIL_COUNTRY_CODE: &str = "BR";

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct IssuerName(String);

impl IssuerName {
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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct IssuerId(Uuid);

impl IssuerId {
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

impl IssuerStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, IssuerStatus::Active)
    }

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

#[derive(Debug)]
pub struct Issuer {
    id: IssuerId,
    status: IssuerStatus,
    created_at: DateTime<Utc>,

    name: Option<IssuerName>,
    cnpj: Option<Cnpj>,
    lei: Option<Lei>,
    country_code: Option<CountryCode>,

    // Securities issued by this issuer. Read-side enrichment only: it is populated by eager reads
    // ([`crate::LoadMode::Eager`]) and never persisted through the issuer repository; securities
    // are written through their own repository.
    securities: Loading<Vec<Security>>,
}

// Raw state of an [`Issuer`] as persisted. Used by storage adapters to rehydrate the entity without
// re-running builder validation (grouped in a struct because the field count outgrew a positional
// constructor).
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
    pub fn builder() -> IssuerBuilder {
        IssuerBuilder::new()
    }

    pub fn id(&self) -> &IssuerId {
        &self.id
    }
    pub fn status(&self) -> IssuerStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn name(&self) -> Option<&IssuerName> {
        self.name.as_ref()
    }
    pub fn cnpj(&self) -> Option<&Cnpj> {
        self.cnpj.as_ref()
    }
    pub fn lei(&self) -> Option<&Lei> {
        self.lei.as_ref()
    }
    pub fn country_code(&self) -> Option<&CountryCode> {
        self.country_code.as_ref()
    }

    // Securities issued by this issuer, when they were loaded.
    //
    // `None` means the relation was not fetched (lazy read); load it through the security
    // repository when needed. `Some(&[])` means the issuer is known to have no securities.
    pub fn securities(&self) -> Option<&[Security]> {
        self.securities.as_loaded().map(Vec::as_slice)
    }

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: IssuerId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn status(mut self, status: IssuerStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    pub fn name(mut self, name: IssuerName) -> Self {
        self.name = Some(name);
        self
    }

    pub fn cnpj(mut self, cnpj: Cnpj) -> Self {
        self.cnpj = Some(cnpj);
        self
    }

    pub fn lei(mut self, lei: Lei) -> Self {
        self.lei = Some(lei);
        self
    }

    pub fn country_code(mut self, country_code: CountryCode) -> Self {
        self.country_code = Some(country_code);
        self
    }

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
}
