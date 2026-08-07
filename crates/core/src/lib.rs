mod common;
mod issuer;
mod storage;

pub use common::Versioned;

pub use storage::{PersistenceManager, Repositories, StorageEngine, StorageError, StorageFault};

pub use valqeron_identifiers::{
    Cfi, CfiError, Cnpj, CnpjError, CountryCode, CountryCodeError, Isin, IsinError, Lei, LeiError,
};

pub use issuer::patch::IssuerPatch;

#[doc(hidden)]
pub use issuer::patch::{Empty, IssuerPatchBuilder, NonEmpty};

pub use issuer::repository::{IssuerRepository, RepositoryResult, WriteOutcome};

pub use issuer::service::register_issuer;

pub use issuer::{
    Issuer, IssuerBuilder, IssuerId, IssuerName, IssuerStatus,
    error::{IssuerBuilderError, IssuerNameError, IssuerStatusError, RegisterIssuerError},
};
