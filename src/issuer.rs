use crate::common::{CnpjIdentifier, LeiIdentifier};
use crate::issuer::error::{IssuerBuilderError, IssuerNameError, IssuerStatusError};
use chrono::{DateTime, Utc};
use ftracker_identifiers::CountryCode;
use std::str::FromStr;
use uuid::Uuid;

mod error;
mod patch;
mod repository;

const ISSUER_NAME_MAX_LEN: usize = 200;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssuerStatus {
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

impl Default for IssuerStatus {
    fn default() -> Self {
        IssuerStatus::Active
    }
}

pub struct Issuer {
    id: IssuerId,
    status: IssuerStatus,
    created_at: DateTime<Utc>,

    name: Option<IssuerName>,
    cnpj: Option<CnpjIdentifier>,
    lei: Option<LeiIdentifier>,
    country_code: Option<CountryCode>,
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
    pub fn cnpj(&self) -> Option<&CnpjIdentifier> {
        self.cnpj.as_ref()
    }
    pub fn lei(&self) -> Option<&LeiIdentifier> {
        self.lei.as_ref()
    }
    pub fn country_code(&self) -> Option<&CountryCode> {
        self.country_code.as_ref()
    }

    pub fn reconstitute(
        id: IssuerId,
        status: IssuerStatus,
        created_at: DateTime<Utc>,
        name: Option<IssuerName>,
        cnpj: Option<CnpjIdentifier>,
        lei: Option<LeiIdentifier>,
        country_code: Option<CountryCode>,
    ) -> Self {
        Self {
            id,
            status,
            created_at,
            name,
            cnpj,
            lei,
            country_code,
        }
    }
}

#[derive(Default)]
pub struct IssuerBuilder {
    id: Option<IssuerId>,
    status: Option<IssuerStatus>,
    created_at: Option<DateTime<Utc>>,
    name: Option<IssuerName>,
    cnpj: Option<CnpjIdentifier>,
    lei: Option<LeiIdentifier>,
    country_code: Option<CountryCode>,
}

const BRAZIL_COUNTRY_CODE: &'static str = "BR";

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

    pub fn cnpj(mut self, cnpj: CnpjIdentifier) -> Self {
        self.cnpj = Some(cnpj);
        self
    }

    pub fn lei(mut self, lei: LeiIdentifier) -> Self {
        self.lei = Some(lei);
        self
    }

    pub fn country_code(mut self, country_code: CountryCode) -> Self {
        self.country_code = Some(country_code);
        self
    }

    pub fn build(self) -> Result<Issuer, IssuerBuilderError> {
        let id = self.id.unwrap_or_else(IssuerId::new);
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
                    country_code = Some(CountryCode::from_str("BR").unwrap());
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_issuer_name_valid() {
        let name = IssuerName::new(" Acme Corp ").expect("Should create valid name");
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
        // Case insensitive parsing
        assert_eq!(
            IssuerStatus::from_str("ACTIVE").unwrap(),
            IssuerStatus::Active
        );
        assert_eq!(
            IssuerStatus::from_str("active").unwrap(),
            IssuerStatus::Active
        );
        assert_eq!(
            IssuerStatus::from_str("Retired").unwrap(),
            IssuerStatus::Retired
        );

        // Invalid parsing
        assert!(matches!(
            IssuerStatus::from_str("UNKNOWN"),
            Err(IssuerStatusError::InvalidStatus)
        ));
    }

    #[test]
    fn test_builder_resolves_defaults() {
        let issuer = Issuer::builder()
            .build()
            .expect("Should build successfully");

        assert!(issuer.status.is_active(), "Default status should be Active");
        // Optionals should be None
        assert!(issuer.name.is_none());
        assert!(issuer.cnpj.is_none());
        assert!(issuer.lei.is_none());
        assert!(issuer.country_code.is_none());
        // Timestamps and IDs should be populated
        assert!(issuer.created_at <= Utc::now());
    }

    #[test]
    fn test_builder_with_all_valid_fields() {
        let custom_time = Utc::now();
        let name = IssuerName::new("Tech Global").unwrap();
        let lei = LeiIdentifier::new("5493001R7TCG5YONX123");
        let country = CountryCode::from_str("US").unwrap();

        let issuer = Issuer::builder()
            .status(IssuerStatus::Retired)
            .created_at(custom_time)
            .name(name.clone())
            .lei(lei)
            .country_code(country)
            .build()
            .expect("Should build successfully");

        assert!(issuer.status.is_retired());
        assert_eq!(issuer.created_at, custom_time);
        assert_eq!(issuer.name.unwrap(), name);
        assert_eq!(issuer.country_code.unwrap().as_str(), "US");
    }

    #[test]
    fn test_builder_cnpj_validation_success_without_country() {
        let cnpj = CnpjIdentifier::new("12.345.678/0001-90");

        let issuer = Issuer::builder()
            .cnpj(cnpj)
            .build()
            .expect("Should build successfully when no country code is provided alongside CNPJ");

        assert_eq!(issuer.country_code.unwrap().as_str(), "BR");
    }

    #[test]
    fn test_builder_cnpj_validation_success_with_br() {
        let cnpj = CnpjIdentifier::new("12.345.678/0001-90");
        let country_br = CountryCode::from_str("BR").unwrap();

        let issuer = Issuer::builder()
            .cnpj(cnpj)
            .country_code(country_br)
            .build()
            .expect("Should build successfully when BR is explicitly provided");

        assert_eq!(issuer.country_code.unwrap().as_str(), "BR");
    }

    #[test]
    fn test_builder_cnpj_validation_fails_with_foreign_country() {
        let cnpj = CnpjIdentifier::new("12.345.678/0001-90");
        let country_us = CountryCode::from_str("US").unwrap();

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
