use crate::{
    StorageFault,
    common::{Empty, NonEmpty, RepositoryResult, Versioned, WriteOutcome},
    domain::{issuer::IssuerId, issuer::IssuerRepository},
};

use chrono::{DateTime, Utc};
use std::{marker::PhantomData, num::NonZeroU32, rc::Rc, str::FromStr, sync::Arc};
use uuid::Uuid;
use valqeron_identifiers::{Cfi, Isin};

const SECURITY_NAME_MAX_LEN: usize = 200;

// ================ SECURITY DOMAIN ================

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SecurityName(String);

#[derive(thiserror::Error, Debug)]
pub enum SecurityNameError {
    #[error("security name cannot be empty")]
    Empty,

    #[error("security name exceeds maximum length of {max} characters")]
    TooLong { max: usize },
}

impl SecurityName {
    /// # Errors
    ///
    /// Return `SecurityNameError`.
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

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SecurityId(Uuid);

impl SecurityId {
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

impl Default for SecurityId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityKind {
    CommonShare,
    PreferredShare,
    Unit,
    DepositaryReceipt,
}

#[derive(thiserror::Error, Debug)]
pub enum SecurityKindError {
    #[error(
        "Invalid kind. Must be one of: {kinds:?}",
        kinds = vec!["COMMON_SHARE", "PREFERRED_SHARE", "UNIT", "DEPOSITARY_RECEIPT"]
    )]
    InvalidKind,
}

impl SecurityKind {
    #[must_use]
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

#[derive(thiserror::Error, Debug)]
pub enum SecurityStatusError {
    #[error("Invalid status. Must be one of: {statuses:?}", statuses = vec!["ACTIVE", "RETIRED"])]
    InvalidStatus,
}

impl SecurityStatus {
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, SecurityStatus::Active)
    }

    #[must_use]
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

#[derive(thiserror::Error, Debug)]
pub enum DrRatioError {
    #[error("depositary receipt ratio requires a non-zero receipts side")]
    ZeroReceipts,

    #[error("depositary receipt ratio requires a non-zero underlying side")]
    ZeroUnderlying,
}

impl DepositaryReceiptRatio {
    /// # Errors
    ///
    /// Return `DrRatioError`.
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

#[derive(Debug, Clone)]
pub struct Security {
    id: SecurityId,
    issuer_id: IssuerId,
    kind: SecurityKind,
    status: SecurityStatus,
    created_at: DateTime<Utc>,

    name: Option<SecurityName>,
    isin: Option<Isin>,
    cfi: Option<Cfi>,
    underlying_security_id: Option<SecurityId>,
    dr_ratio: Option<DepositaryReceiptRatio>,
}

#[derive(Debug)]
pub struct SecuritySnapshot {
    pub id: SecurityId,
    pub issuer_id: IssuerId,
    pub kind: SecurityKind,
    pub status: SecurityStatus,
    pub created_at: DateTime<Utc>,
    pub name: Option<SecurityName>,
    pub isin: Option<Isin>,
    pub cfi: Option<Cfi>,
    pub underlying_security_id: Option<SecurityId>,
    pub dr_ratio: Option<DepositaryReceiptRatio>,
}

impl Security {
    #[must_use]
    pub fn builder(issuer_id: IssuerId, kind: SecurityKind) -> SecurityBuilder {
        SecurityBuilder::new(issuer_id, kind)
    }
    #[must_use]
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    #[must_use]
    pub fn issuer_id(&self) -> &IssuerId {
        &self.issuer_id
    }
    #[must_use]
    pub fn kind(&self) -> SecurityKind {
        self.kind
    }
    #[must_use]
    pub fn status(&self) -> SecurityStatus {
        self.status
    }
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub fn name(&self) -> Option<&SecurityName> {
        self.name.as_ref()
    }
    #[must_use]
    pub fn isin(&self) -> Option<&Isin> {
        self.isin.as_ref()
    }
    #[must_use]
    pub fn cfi(&self) -> Option<&Cfi> {
        self.cfi.as_ref()
    }
    #[must_use]
    pub fn underlying_security_id(&self) -> Option<&SecurityId> {
        self.underlying_security_id.as_ref()
    }
    #[must_use]
    pub fn dr_ratio(&self) -> Option<&DepositaryReceiptRatio> {
        self.dr_ratio.as_ref()
    }

    #[must_use]
    pub fn reconstitute(snapshot: SecuritySnapshot) -> Self {
        Self {
            id: snapshot.id,
            issuer_id: snapshot.issuer_id,
            kind: snapshot.kind,
            status: snapshot.status,
            created_at: snapshot.created_at,
            name: snapshot.name,
            isin: snapshot.isin,
            cfi: snapshot.cfi,
            underlying_security_id: snapshot.underlying_security_id,
            dr_ratio: snapshot.dr_ratio,
        }
    }
}

pub struct SecurityBuilder {
    issuer_id: IssuerId,
    kind: SecurityKind,
    id: Option<SecurityId>,
    status: Option<SecurityStatus>,
    created_at: Option<DateTime<Utc>>,
    name: Option<SecurityName>,
    isin: Option<Isin>,
    cfi: Option<Cfi>,
    underlying_security_id: Option<SecurityId>,
    dr_ratio: Option<DepositaryReceiptRatio>,
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityBuilderError {
    #[error("An underlying security may only be set on a DEPOSITARY_RECEIPT security. Found: {0}")]
    UnderlyingRequiresDepositaryReceipt(String),

    #[error(
        "A depositary receipt ratio may only be set on a DEPOSITARY_RECEIPT security. Found: {0}"
    )]
    DrRatioRequiresDepositaryReceipt(String),

    #[error("a security cannot reference itself as its underlying")]
    SelfUnderlying,
}

impl SecurityBuilder {
    #[must_use]
    pub fn new(issuer_id: IssuerId, kind: SecurityKind) -> Self {
        Self {
            issuer_id,
            kind,
            id: None,
            status: None,
            created_at: None,
            name: None,
            isin: None,
            cfi: None,
            underlying_security_id: None,
            dr_ratio: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: SecurityId) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn status(mut self, status: SecurityStatus) -> Self {
        self.status = Some(status);
        self
    }

    #[must_use]
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    #[must_use]
    pub fn name(mut self, name: SecurityName) -> Self {
        self.name = Some(name);
        self
    }

    #[must_use]
    pub fn isin(mut self, isin: Isin) -> Self {
        self.isin = Some(isin);
        self
    }

    #[must_use]
    pub fn cfi(mut self, cfi: Cfi) -> Self {
        self.cfi = Some(cfi);
        self
    }

    #[must_use]
    pub fn underlying_security_id(mut self, underlying_security_id: SecurityId) -> Self {
        self.underlying_security_id = Some(underlying_security_id);
        self
    }

    #[must_use]
    pub fn dr_ratio(mut self, dr_ratio: DepositaryReceiptRatio) -> Self {
        self.dr_ratio = Some(dr_ratio);
        self
    }

    /// # Errors
    ///
    /// Returns `SecurityBuilderError`.
    pub fn build(self) -> Result<Security, SecurityBuilderError> {
        let id = self.id.unwrap_or_default();
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
            && underlying == &id
        {
            return Err(SecurityBuilderError::SelfUnderlying);
        }

        Ok(Security {
            id,
            issuer_id: self.issuer_id,
            kind: self.kind,
            status,
            created_at,
            name: self.name,
            isin: self.isin,
            cfi: self.cfi,
            underlying_security_id: self.underlying_security_id,
            dr_ratio: self.dr_ratio,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SecurityPatch {
    pub(crate) name: Option<SecurityName>,
    pub(crate) status: Option<SecurityStatus>,
    pub(crate) isin: Option<Isin>,
    pub(crate) cfi: Option<Cfi>,
    pub(crate) dr_ratio: Option<DepositaryReceiptRatio>,
}

impl SecurityPatch {
    #[must_use]
    pub const fn builder() -> SecurityPatchBuilder<Empty> {
        SecurityPatchBuilder::new()
    }

    const fn empty() -> Self {
        Self {
            name: None,
            status: None,
            isin: None,
            cfi: None,
            dr_ratio: None,
        }
    }

    #[must_use]
    pub const fn name(&self) -> Option<&SecurityName> {
        self.name.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> Option<SecurityStatus> {
        self.status
    }

    #[must_use]
    pub const fn isin(&self) -> Option<&Isin> {
        self.isin.as_ref()
    }

    #[must_use]
    pub const fn cfi(&self) -> Option<&Cfi> {
        self.cfi.as_ref()
    }

    #[must_use]
    pub const fn dr_ratio(&self) -> Option<DepositaryReceiptRatio> {
        self.dr_ratio
    }
}

pub struct SecurityPatchBuilder<State> {
    inner: SecurityPatch,
    _state: PhantomData<State>,
}

impl SecurityPatchBuilder<Empty> {
    const fn new() -> Self {
        Self {
            inner: SecurityPatch::empty(),
            _state: PhantomData,
        }
    }
}

impl<State> SecurityPatchBuilder<State> {
    #[must_use]
    pub fn name(self, name: SecurityName) -> SecurityPatchBuilder<NonEmpty> {
        SecurityPatchBuilder {
            inner: SecurityPatch {
                name: Some(name),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn status(self, status: SecurityStatus) -> SecurityPatchBuilder<NonEmpty> {
        SecurityPatchBuilder {
            inner: SecurityPatch {
                status: Some(status),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn isin(self, isin: Isin) -> SecurityPatchBuilder<NonEmpty> {
        SecurityPatchBuilder {
            inner: SecurityPatch {
                isin: Some(isin),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn cfi(self, cfi: Cfi) -> SecurityPatchBuilder<NonEmpty> {
        SecurityPatchBuilder {
            inner: SecurityPatch {
                cfi: Some(cfi),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn dr_ratio(self, dr_ratio: DepositaryReceiptRatio) -> SecurityPatchBuilder<NonEmpty> {
        SecurityPatchBuilder {
            inner: SecurityPatch {
                dr_ratio: Some(dr_ratio),
                ..self.inner
            },
            _state: PhantomData,
        }
    }
}

impl SecurityPatchBuilder<NonEmpty> {
    #[must_use]
    pub fn build(self) -> SecurityPatch {
        self.inner
    }
}

// ================ SECURITY REPOSITORY ================
#[cfg_attr(test, mockall::automock)]
pub trait SecurityRepository {
    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn find_by_id(&self, id: &SecurityId) -> RepositoryResult<Option<Versioned<Security>>>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn find_by_isin(&self, isin: &Isin) -> RepositoryResult<Option<Versioned<Security>>>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn list_all(&self) -> RepositoryResult<Vec<Versioned<Security>>>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn list_by_issuer(&self, issuer_id: &IssuerId) -> RepositoryResult<Vec<Versioned<Security>>>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn list_paged(
        &self,
        after: Option<SecurityId>,
        limit: u32,
    ) -> RepositoryResult<Vec<Versioned<Security>>>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn exists(&self, id: &SecurityId) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn exists_by_isin(&self, isin: &Isin) -> RepositoryResult<bool>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn insert(&self, security: &Security) -> RepositoryResult<()>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn apply_patch(
        &self,
        id: &SecurityId,
        expected_version: u32,
        patch: SecurityPatch,
    ) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn update(&self, security: &Security, expected_version: u32) -> RepositoryResult<WriteOutcome>;

    /// # Errors
    ///
    /// Return `StorageFault` if a storage error occurs.
    fn delete(&self, id: &SecurityId, expected_version: u32) -> RepositoryResult<WriteOutcome>;
}

macro_rules! delegate_security_repository {
    ($ty:ty) => {
        impl<R: SecurityRepository + ?Sized> SecurityRepository for $ty {
            fn find_by_id(&self, id: &SecurityId) -> RepositoryResult<Option<Versioned<Security>>> {
                (**self).find_by_id(id)
            }
            fn find_by_isin(&self, isin: &Isin) -> RepositoryResult<Option<Versioned<Security>>> {
                (**self).find_by_isin(isin)
            }
            fn list_all(&self) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_all()
            }
            fn list_by_issuer(
                &self,
                issuer_id: &IssuerId,
            ) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_by_issuer(issuer_id)
            }
            fn list_paged(
                &self,
                after: Option<SecurityId>,
                limit: u32,
            ) -> RepositoryResult<Vec<Versioned<Security>>> {
                (**self).list_paged(after, limit)
            }
            fn exists(&self, id: &SecurityId) -> RepositoryResult<bool> {
                (**self).exists(id)
            }
            fn exists_by_isin(&self, isin: &Isin) -> RepositoryResult<bool> {
                (**self).exists_by_isin(isin)
            }
            fn insert(&self, security: &Security) -> RepositoryResult<()> {
                (**self).insert(security)
            }
            fn apply_patch(
                &self,
                id: &SecurityId,
                expected_version: u32,
                patch: SecurityPatch,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).apply_patch(id, expected_version, patch)
            }
            fn update(
                &self,
                security: &Security,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).update(security, expected_version)
            }
            fn delete(
                &self,
                id: &SecurityId,
                expected_version: u32,
            ) -> RepositoryResult<WriteOutcome> {
                (**self).delete(id, expected_version)
            }
        }
    };
}

delegate_security_repository!(Box<R>);
delegate_security_repository!(Rc<R>);
delegate_security_repository!(Arc<R>);

// ================ SECURITY SERVICE ================
#[derive(Debug, thiserror::Error)]
pub enum RegisterSecurityError {
    #[error("the referenced issuer does not exist")]
    UnknownIssuer,

    #[error("a security with this ISIN already exists")]
    DuplicateIsin,

    #[error("the referenced underlying security does not exist")]
    UnknownUnderlyingSecurity,

    #[error(transparent)]
    Storage(#[from] StorageFault),
}

/// # Errors
///
/// Return `RegisterSecurityError` if unknown isser, duplicated isin, unknown uncerlying security or
/// storage fault.
pub fn register_security<S, I>(
    securities: &S,
    issuers: &I,
    security: &Security,
) -> Result<(), RegisterSecurityError>
where
    S: SecurityRepository + ?Sized,
    I: IssuerRepository + ?Sized,
{
    if !issuers.exists(security.issuer_id())? {
        return Err(RegisterSecurityError::UnknownIssuer);
    }

    if let Some(isin) = security.isin()
        && securities.exists_by_isin(isin)?
    {
        return Err(RegisterSecurityError::DuplicateIsin);
    }

    if let Some(underlying) = security.underlying_security_id()
        && !securities.exists(underlying)?
    {
        return Err(RegisterSecurityError::UnknownUnderlyingSecurity);
    }

    securities.insert(security)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vale S.A. common shares (B3: VALE3).
    const VALE_ON_ISIN: &str = "BRVALEACNOR0";
    // Vale S.A. sponsored ADR (NYSE: VALE).
    const VALE_ADR_ISIN: &str = "US91912E1055";

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
    fn security_id_creation_and_conversions() {
        let original_uuid = Uuid::now_v7();
        let id = SecurityId::from_uuid(original_uuid);

        assert_eq!(id.as_uuid(), &original_uuid);
        assert_eq!(id.value(), original_uuid.to_string());
        assert_eq!(id.as_bytes(), original_uuid.as_bytes());
        assert_ne!(SecurityId::new(), SecurityId::new());
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
        let security_result = Security::builder(IssuerId::new(), SecurityKind::CommonShare).build();
        assert!(security_result.is_ok());
        let Some(security) = security_result.ok() else {
            return;
        };

        assert!(security.status().is_active());
        assert!(matches!(security.kind(), SecurityKind::CommonShare));
        assert!(security.name().is_none());
        assert!(security.isin().is_none());
        assert!(security.cfi().is_none());
        assert!(security.underlying_security_id().is_none());
        assert!(security.dr_ratio().is_none());
        assert!(security.created_at() <= Utc::now());
    }

    #[test]
    fn builder_accepts_common_share_with_isin() {
        let isin_result = Isin::new(VALE_ON_ISIN);
        assert!(isin_result.is_ok());
        let Some(isin) = isin_result.ok() else {
            return;
        };

        let security_result = Security::builder(IssuerId::new(), SecurityKind::CommonShare)
            .isin(isin)
            .build();
        assert!(security_result.is_ok());
        let Some(security) = security_result.ok() else {
            return;
        };

        assert!(matches!(security.isin(), Some(i) if i.as_str() == VALE_ON_ISIN));
    }

    #[test]
    fn builder_accepts_depositary_receipt_with_underlying_and_ratio() {
        let underlying_id = SecurityId::new();
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

        let adr_result = Security::builder(IssuerId::new(), SecurityKind::DepositaryReceipt)
            .isin(isin)
            .underlying_security_id(underlying_id)
            .dr_ratio(ratio)
            .build();
        assert!(adr_result.is_ok());
        let Some(adr) = adr_result.ok() else {
            return;
        };

        assert!(adr.kind().is_depositary_receipt());
        assert!(matches!(
            adr.underlying_security_id(),
            Some(id) if id == &underlying_id
        ));
        assert!(matches!(adr.dr_ratio(), Some(r) if r.receipts().get() == 1));
    }

    #[test]
    fn builder_rejects_underlying_on_non_depositary_receipt() {
        let result = Security::builder(IssuerId::new(), SecurityKind::CommonShare)
            .underlying_security_id(SecurityId::new())
            .build();

        assert!(
            matches!(
                result,
                Err(SecurityBuilderError::UnderlyingRequiresDepositaryReceipt(kind))
                    if kind == "COMMON_SHARE"
            ),
            "Only depositary receipts may reference an underlying security"
        );
    }

    #[test]
    fn builder_rejects_dr_ratio_on_non_depositary_receipt() {
        let ratio_result = DepositaryReceiptRatio::new(1, 1);
        assert!(ratio_result.is_ok());
        let Some(ratio) = ratio_result.ok() else {
            return;
        };

        let result = Security::builder(IssuerId::new(), SecurityKind::PreferredShare)
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
        let id = SecurityId::new();
        let result = Security::builder(IssuerId::new(), SecurityKind::DepositaryReceipt)
            .id(id)
            .underlying_security_id(id)
            .build();

        assert!(matches!(result, Err(SecurityBuilderError::SelfUnderlying)));
    }

    #[test]
    fn single_field_patch_only_sets_that_field() {
        let name_result = SecurityName::new("Vale ADR");
        assert!(name_result.is_ok());
        let Some(name) = name_result.ok() else {
            return;
        };
        let patch = SecurityPatch::builder().name(name.clone()).build();

        assert_eq!(patch.name, Some(name));
        assert!(patch.status.is_none());
        assert!(patch.isin.is_none());
        assert!(patch.cfi.is_none());
        assert!(patch.dr_ratio.is_none());
    }
}

#[cfg(test)]
mod tests_service {
    use super::*;
    use crate::domain::issuer::{IssuerId, MockIssuerRepository};
    use crate::domain::security::{SecurityId, SecurityKind};
    use valqeron_identifiers::Isin;

    const VALE_ON_ISIN: &str = "BRVALEACNOR0";

    fn common_share_with_isin() -> Option<Security> {
        let isin = Isin::new(VALE_ON_ISIN).ok()?;
        Security::builder(IssuerId::new(), SecurityKind::CommonShare)
            .isin(isin)
            .build()
            .ok()
    }

    fn adr_with_underlying(underlying: SecurityId) -> Option<Security> {
        Security::builder(IssuerId::new(), SecurityKind::DepositaryReceipt)
            .underlying_security_id(underlying)
            .build()
            .ok()
    }

    #[test]
    fn register_inserts_when_references_and_isin_are_valid() {
        let mut issuers = MockIssuerRepository::new();
        issuers.expect_exists().returning(|_| Ok(true));

        let mut securities = MockSecurityRepository::new();
        securities.expect_exists_by_isin().returning(|_| Ok(false));
        securities.expect_insert().returning(|_| Ok(()));

        let Some(security) = common_share_with_isin() else {
            return;
        };
        assert!(register_security(&securities, &issuers, &security).is_ok());
    }

    #[test]
    fn register_rejects_unknown_issuer_before_insert() {
        let mut issuers = MockIssuerRepository::new();
        issuers.expect_exists().returning(|_| Ok(false));

        let mut securities = MockSecurityRepository::new();
        // insert must never be called for an unknown issuer.
        securities.expect_insert().never();

        let Some(security) = common_share_with_isin() else {
            return;
        };
        assert!(matches!(
            register_security(&securities, &issuers, &security),
            Err(RegisterSecurityError::UnknownIssuer)
        ));
    }

    #[test]
    fn register_rejects_duplicate_isin_before_insert() {
        let mut issuers = MockIssuerRepository::new();
        issuers.expect_exists().returning(|_| Ok(true));

        let mut securities = MockSecurityRepository::new();
        securities.expect_exists_by_isin().returning(|_| Ok(true));
        // insert must never be called on a duplicate ISIN.
        securities.expect_insert().never();

        let Some(security) = common_share_with_isin() else {
            return;
        };
        assert!(matches!(
            register_security(&securities, &issuers, &security),
            Err(RegisterSecurityError::DuplicateIsin)
        ));
    }

    #[test]
    fn register_rejects_unknown_underlying_before_insert() {
        let mut issuers = MockIssuerRepository::new();
        issuers.expect_exists().returning(|_| Ok(true));

        let mut securities = MockSecurityRepository::new();
        securities.expect_exists().returning(|_| Ok(false));
        // insert must never be called when the underlying is missing.
        securities.expect_insert().never();

        let Some(adr) = adr_with_underlying(SecurityId::new()) else {
            return;
        };
        assert!(matches!(
            register_security(&securities, &issuers, &adr),
            Err(RegisterSecurityError::UnknownUnderlyingSecurity)
        ));
    }

    #[test]
    fn register_inserts_depositary_receipt_when_underlying_exists() {
        let mut issuers = MockIssuerRepository::new();
        issuers.expect_exists().returning(|_| Ok(true));

        let mut securities = MockSecurityRepository::new();
        securities.expect_exists().returning(|_| Ok(true));
        securities.expect_insert().returning(|_| Ok(()));

        let Some(adr) = adr_with_underlying(SecurityId::new()) else {
            return;
        };
        assert!(register_security(&securities, &issuers, &adr).is_ok());
    }
}
