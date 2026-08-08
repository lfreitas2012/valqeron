use crate::security::error::{
    DrRatioError, SecurityBuilderError, SecurityKindError, SecurityNameError, SecurityStatusError,
};
use chrono::{DateTime, Utc};
use std::num::NonZeroU32;
use std::str::FromStr;
use valqeron_identifiers::{Cfi, Isin};

pub mod error;
pub mod patch;
pub mod repository;
pub mod service;

const SECURITY_NAME_MAX_LEN: usize = 200;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SecurityName(String);

impl SecurityName {
    pub fn new(value: impl Into<String>) -> Result<Self, SecurityNameError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(SecurityNameError::Empty);
        }
        if trimmed.chars().count() > SECURITY_NAME_MAX_LEN {
            return Err(SecurityNameError::TooLong {
                max: SECURITY_NAME_MAX_LEN,
            });
        }

        Ok(Self(trimmed.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityKind {
    /// Ordinary/common shares (B3 "ON", e.g., VALE3).
    CommonShare,
    /// Preference shares (B3 "PN"; US preferred stock).
    PreferredShare,
    /// Bundled certificates such as B3 units (e.g., SANB11).
    Unit,
    /// Depositary receipts (ADR/BDR/GDR) wrapping underlying security.
    DepositaryReceipt,
}

impl SecurityKind {
    pub fn is_depositary_receipt(&self) -> bool {
        matches!(self, SecurityKind::DepositaryReceipt)
    }
}

impl FromStr for SecurityKind {
    type Err = SecurityKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "COMMON_SHARE" => Ok(SecurityKind::CommonShare),
            "PREFERRED_SHARE" => Ok(SecurityKind::PreferredShare),
            "UNIT" => Ok(SecurityKind::Unit),
            "DEPOSITARY_RECEIPT" => Ok(SecurityKind::DepositaryReceipt),
            _ => Err(SecurityKindError::InvalidKind),
        }
    }
}

impl From<SecurityKind> for String {
    fn from(val: SecurityKind) -> Self {
        match val {
            SecurityKind::CommonShare => "COMMON_SHARE".into(),
            SecurityKind::PreferredShare => "PREFERRED_SHARE".into(),
            SecurityKind::Unit => "UNIT".into(),
            SecurityKind::DepositaryReceipt => "DEPOSITARY_RECEIPT".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SecurityStatus {
    #[default]
    Active,
    Retired,
}

impl SecurityStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, SecurityStatus::Active)
    }

    pub fn is_retired(&self) -> bool {
        matches!(self, SecurityStatus::Retired)
    }
}

impl FromStr for SecurityStatus {
    type Err = SecurityStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Ok(SecurityStatus::Active),
            "RETIRED" => Ok(SecurityStatus::Retired),
            _ => Err(SecurityStatusError::InvalidStatus),
        }
    }
}

impl From<SecurityStatus> for String {
    fn from(val: SecurityStatus) -> Self {
        match val {
            SecurityStatus::Active => "ACTIVE".into(),
            SecurityStatus::Retired => "RETIRED".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DepositaryReceiptRatio {
    receipts: NonZeroU32,
    underlying: NonZeroU32,
}

impl DepositaryReceiptRatio {
    pub fn new(receipts: u32, underlying: u32) -> Result<Self, DrRatioError> {
        let receipts = NonZeroU32::new(receipts).ok_or(DrRatioError::ZeroReceipts)?;
        let underlying = NonZeroU32::new(underlying).ok_or(DrRatioError::ZeroUnderlying)?;
        Ok(Self {
            receipts,
            underlying,
        })
    }

    #[must_use]
    pub const fn receipts(&self) -> NonZeroU32 {
        self.receipts
    }

    #[must_use]
    pub const fn underlying(&self) -> NonZeroU32 {
        self.underlying
    }
}

#[derive(Debug)]
pub struct Security {
    isin: Isin,
    kind: SecurityKind,
    status: SecurityStatus,
    created_at: DateTime<Utc>,

    name: Option<SecurityName>,
    cfi: Option<Cfi>,
    underlying_security_id: Option<Isin>,
    dr_ratio: Option<DepositaryReceiptRatio>,
}

#[derive(Debug)]
pub struct SecuritySnapshot {
    pub isin: Isin,
    pub kind: SecurityKind,
    pub status: SecurityStatus,
    pub created_at: DateTime<Utc>,
    pub name: Option<SecurityName>,
    pub cfi: Option<Cfi>,
    pub underlying_security_id: Option<Isin>,
    pub dr_ratio: Option<DepositaryReceiptRatio>,
}

impl Security {
    pub fn builder(isin: Isin, kind: SecurityKind) -> SecurityBuilder {
        SecurityBuilder::new(isin, kind)
    }

    pub fn isin(&self) -> Isin {
        self.isin
    }

    pub fn kind(&self) -> SecurityKind {
        self.kind
    }
    pub fn status(&self) -> SecurityStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn name(&self) -> Option<&SecurityName> {
        self.name.as_ref()
    }
    pub fn cfi(&self) -> Option<&Cfi> {
        self.cfi.as_ref()
    }
    pub fn underlying_security_isin(&self) -> Option<&Isin> {
        self.underlying_security_id.as_ref()
    }
    pub fn dr_ratio(&self) -> Option<&DepositaryReceiptRatio> {
        self.dr_ratio.as_ref()
    }

    pub fn reconstitute(snapshot: SecuritySnapshot) -> Self {
        Self {
            isin: snapshot.isin,
            kind: snapshot.kind,
            status: snapshot.status,
            created_at: snapshot.created_at,
            name: snapshot.name,
            cfi: snapshot.cfi,
            underlying_security_id: snapshot.underlying_security_id,
            dr_ratio: snapshot.dr_ratio,
        }
    }
}

pub struct SecurityBuilder {
    isin: Isin,
    kind: SecurityKind,
    status: Option<SecurityStatus>,
    created_at: Option<DateTime<Utc>>,
    name: Option<SecurityName>,
    cfi: Option<Cfi>,
    underlying_security_id: Option<Isin>,
    dr_ratio: Option<DepositaryReceiptRatio>,
}

impl SecurityBuilder {
    pub fn new(isin: Isin, kind: SecurityKind) -> Self {
        Self {
            kind,
            isin,
            status: None,
            created_at: None,
            name: None,
            cfi: None,
            underlying_security_id: None,
            dr_ratio: None,
        }
    }

    pub fn status(mut self, status: SecurityStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    pub fn name(mut self, name: SecurityName) -> Self {
        self.name = Some(name);
        self
    }

    pub fn cfi(mut self, cfi: Cfi) -> Self {
        self.cfi = Some(cfi);
        self
    }

    pub fn underlying_security_id(mut self, underlying_security_id: Isin) -> Self {
        self.underlying_security_id = Some(underlying_security_id);
        self
    }

    pub fn dr_ratio(mut self, dr_ratio: DepositaryReceiptRatio) -> Self {
        self.dr_ratio = Some(dr_ratio);
        self
    }

    pub fn build(self) -> Result<Security, SecurityBuilderError> {
        let status = self.status.unwrap_or_default();
        let created_at = self.created_at.unwrap_or_else(Utc::now);

        // Cross-field validation: depositary receipt fields require the
        // DepositaryReceipt kind.
        if !self.kind.is_depositary_receipt() {
            if self.underlying_security_id.is_some() {
                return Err(SecurityBuilderError::UnderlyingRequiresDepositaryReceipt(
                    self.kind.into(),
                ));
            }
            if self.dr_ratio.is_some() {
                return Err(SecurityBuilderError::DrRatioRequiresDepositaryReceipt(
                    self.kind.into(),
                ));
            }
        }

        // Cross-field validation: a depositary receipt cannot wrap itself.
        if let Some(underlying) = &self.underlying_security_id
            && underlying == &self.isin
        {
            return Err(SecurityBuilderError::SelfUnderlying);
        }

        Ok(Security {
            isin: self.isin,
            kind: self.kind,
            status,
            created_at,
            name: self.name,
            cfi: self.cfi,
            underlying_security_id: self.underlying_security_id,
            dr_ratio: self.dr_ratio,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const VALE_ON_ISIN: &str = "BRVALEACNOR0";
    const VALE_ADR_ISIN: &str = "US91912E1055";
    const APPLE_ISIN: &str = "US0378331005";

    #[test]
    fn security_name_trims_and_validates() {
        let name_result = SecurityName::new(" Vale ON ");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        assert_eq!(name.as_str(), "Vale ON");
    }

    #[test]
    fn security_name_empty_fails() {
        assert!(matches!(
            SecurityName::new("   "),
            Err(SecurityNameError::Empty)
        ));
    }

    #[test]
    fn security_name_too_long_fails() {
        let long_string = "A".repeat(SECURITY_NAME_MAX_LEN.saturating_add(1));
        assert!(matches!(
            SecurityName::new(long_string),
            Err(SecurityNameError::TooLong { max: 200 })
        ));
    }

    #[test]
    fn security_kind_round_trips() {
        let kinds = [
            (SecurityKind::CommonShare, "COMMON_SHARE"),
            (SecurityKind::PreferredShare, "PREFERRED_SHARE"),
            (SecurityKind::Unit, "UNIT"),
            (SecurityKind::DepositaryReceipt, "DEPOSITARY_RECEIPT"),
        ];
        for (kind, canonical) in kinds {
            let as_string: String = kind.into();
            assert_eq!(as_string, canonical);
            assert!(matches!(
                SecurityKind::from_str(canonical),
                Ok(parsed) if parsed == kind
            ));
        }

        assert!(matches!(
            SecurityKind::from_str("common_share"),
            Ok(SecurityKind::CommonShare)
        ));
        assert!(matches!(
            SecurityKind::from_str("BOND"),
            Err(SecurityKindError::InvalidKind)
        ));
    }

    #[test]
    fn security_status_round_trips() {
        assert!(SecurityStatus::default().is_active());

        let active_str: String = SecurityStatus::Active.into();
        assert_eq!(active_str, "ACTIVE");

        assert!(matches!(
            SecurityStatus::from_str("retired"),
            Ok(SecurityStatus::Retired)
        ));
        assert!(matches!(
            SecurityStatus::from_str("UNKNOWN"),
            Err(SecurityStatusError::InvalidStatus)
        ));
    }

    #[test]
    fn dr_ratio_rejects_zero_on_either_side() {
        assert!(matches!(
            DepositaryReceiptRatio::new(0, 1),
            Err(DrRatioError::ZeroReceipts)
        ));
        assert!(matches!(
            DepositaryReceiptRatio::new(1, 0),
            Err(DrRatioError::ZeroUnderlying)
        ));

        let ratio_result = DepositaryReceiptRatio::new(1, 2);
        assert!(ratio_result.is_ok());
        let Some(ratio) = ratio_result.ok() else {
            return;
        };
        assert_eq!(ratio.receipts().get(), 1);
        assert_eq!(ratio.underlying().get(), 2);
    }

    #[test]
    fn builder_resolves_defaults() {
        let apple = Isin::new("US0378331005").unwrap();
        let security_result = Security::builder(apple, SecurityKind::CommonShare).build();
        assert!(security_result.is_ok());
        let Some(security) = security_result.ok() else {
            return;
        };

        assert!(security.status().is_active());
        assert!(matches!(security.kind(), SecurityKind::CommonShare));
        assert!(security.name().is_none());
        assert!(security.cfi().is_none());
        assert!(security.underlying_security_isin().is_none());
        assert!(security.dr_ratio().is_none());
        assert!(security.created_at() <= Utc::now());
    }

    #[test]
    fn builder_accepts_depositary_receipt_with_underlying_and_ratio() {
        let underlying_security = Isin::new(VALE_ON_ISIN);
        assert!(underlying_security.is_ok());
        let Some(underlying_id) = underlying_security.ok() else {
            return;
        };

        let isin_result = Isin::new(VALE_ADR_ISIN);
        assert!(isin_result.is_ok());
        let Some(isin) = isin_result.ok() else {
            return;
        };
        let ratio_result = DepositaryReceiptRatio::new(1, 1);
        assert!(ratio_result.is_ok());
        let Some(ratio) = ratio_result.ok() else {
            return;
        };

        let adr_result = Security::builder(isin, SecurityKind::DepositaryReceipt)
            .underlying_security_id(underlying_id)
            .dr_ratio(ratio)
            .build();
        assert!(adr_result.is_ok());
        let Some(adr) = adr_result.ok() else {
            return;
        };

        assert!(adr.kind().is_depositary_receipt());
        assert!(matches!(
            adr.underlying_security_isin(),
            Some(id) if id == &underlying_id
        ));
        assert!(matches!(adr.dr_ratio(), Some(r) if r.receipts().get() == 1));
    }

    #[test]
    fn builder_rejects_dr_ratio_on_non_depositary_receipt() {
        let security = Isin::new(VALE_ON_ISIN);
        assert!(security.is_ok());
        let Some(isin) = security.ok() else {
            return;
        };

        let ratio_result = DepositaryReceiptRatio::new(1, 1);
        assert!(ratio_result.is_ok());
        let Some(ratio) = ratio_result.ok() else {
            return;
        };

        let result = Security::builder(isin, SecurityKind::PreferredShare)
            .dr_ratio(ratio)
            .build();

        assert!(matches!(
            result,
            Err(SecurityBuilderError::DrRatioRequiresDepositaryReceipt(kind))
                if kind == "PREFERRED_SHARE"
        ));
    }

    #[test]
    fn builder_rejects_self_referencing_underlying() {
        let security = Isin::new(VALE_ON_ISIN);
        assert!(security.is_ok());
        let Some(isin) = security.ok() else {
            return;
        };

        let result = Security::builder(isin, SecurityKind::DepositaryReceipt)
            .underlying_security_id(isin)
            .build();

        assert!(matches!(result, Err(SecurityBuilderError::SelfUnderlying)));
    }
}
