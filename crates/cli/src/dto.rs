//! Data-transfer objects bridging JSON I/O and `valqeron-core` domain types.
//!
//! Identifier value types are parsed from / rendered to strings here rather than
//! relying on the identifier crate's optional serde support, keeping the wire
//! format explicit and independent of upstream feature flags.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use valqeron_core::{
    Cnpj, CountryCode, Issuer, IssuerBuilder, IssuerName, IssuerStatus, Lei, Versioned,
};

use crate::error::AppResult;

/// A partial issuer document accepted as input for `issuer register`.
///
/// All fields are optional; missing fields fall back to engine defaults (a
/// fresh id, `ACTIVE` status, current timestamp). Values provided via CLI flags
/// are merged on top of a document read from `--input`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerInput {
    pub name: Option<String>,
    pub status: Option<String>,
    pub cnpj: Option<String>,
    pub lei: Option<String>,
    pub country_code: Option<String>,
}

impl IssuerInput {
    /// Overlay CLI-flag values on top of self; flags take precedence over any
    /// value already present (e.g. from a `--input` document).
    pub fn merge_flags(
        mut self,
        name: Option<String>,
        status: Option<String>,
        cnpj: Option<String>,
        lei: Option<String>,
        country_code: Option<String>,
    ) -> Self {
        if name.is_some() {
            self.name = name;
        }
        if status.is_some() {
            self.status = status;
        }
        if cnpj.is_some() {
            self.cnpj = cnpj;
        }
        if lei.is_some() {
            self.lei = lei;
        }
        if country_code.is_some() {
            self.country_code = country_code;
        }
        self
    }

    /// Validate and assemble a domain [`Issuer`]. Each field is converted
    /// through its typed constructor, so any failure yields a precise
    /// `AppError` with a structured problem.
    pub fn into_issuer(self) -> AppResult<Issuer> {
        let mut builder: IssuerBuilder = Issuer::builder();

        if let Some(name) = self.name.as_deref() {
            builder = builder.name(IssuerName::new(name)?);
        }
        if let Some(status) = self.status.as_deref() {
            builder = builder.status(IssuerStatus::from_str(status)?);
        }
        if let Some(cnpj) = self.cnpj.as_deref() {
            builder = builder.cnpj(Cnpj::parse(cnpj)?);
        }
        if let Some(lei) = self.lei.as_deref() {
            builder = builder.lei(Lei::parse(lei)?);
        }
        if let Some(cc) = self.country_code.as_deref() {
            builder = builder.country_code(CountryCode::parse(cc)?);
        }

        Ok(builder.build()?)
    }
}

/// A serializable, read-model view of a stored issuer plus its version.
#[derive(Debug, Clone, Serialize)]
pub struct IssuerView {
    pub id: String,
    pub status: String,
    pub created_at: String,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cnpj: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lei: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
}

impl IssuerView {
    /// Build a view from an issuer at a known version.
    pub fn new(issuer: &Issuer, version: u32) -> Self {
        Self {
            id: issuer.id().value(),
            status: String::from(issuer.status()),
            created_at: issuer.created_at().to_rfc3339(),
            version,
            name: issuer.name().map(|n| n.as_str().to_string()),
            cnpj: issuer.cnpj().map(|c| c.as_str().to_string()),
            lei: issuer.lei().map(|l| l.as_str().to_string()),
            country_code: issuer.country_code().map(|c| c.as_str().to_string()),
        }
    }
}

impl From<&Versioned<Issuer>> for IssuerView {
    fn from(v: &Versioned<Issuer>) -> Self {
        IssuerView::new(&v.data, v.version)
    }
}
