use std::str::FromStr;

use serde::{Deserialize, Serialize};

use valqeron_core::{
    Cnpj, CountryCode, Issuer, IssuerBuilder, IssuerName, IssuerStatus, Lei, Versioned,
};

use crate::error::AppResult;

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
