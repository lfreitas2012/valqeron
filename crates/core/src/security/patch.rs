use crate::common::{Empty, NonEmpty};
use crate::security::{DepositaryReceiptRatio, SecurityName, SecurityStatus};
use std::marker::PhantomData;
use valqeron_identifiers::{Cfi, Isin};

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

#[cfg(test)]
mod tests {
    use super::*;

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
