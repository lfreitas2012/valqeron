//! One mirror struct per service, each a thin `runtime.block_on(...)`
//! wrapper around the matching async service in `admin`/`issuer`/etc. Only
//! `Client` (the blocking facade) constructs these — `AsyncClient` calls the
//! async services directly and never sees this module.

use crate::admin::AdminService;
use crate::issuer::{
    DeleteIssuerRequest, IssuerService, PatchIssuerRequest, RegisterIssuerRequest,
};
use crate::{BackgroundTaskDetail, ClientError, EngineInfo, EngineStatus};
use tokio::runtime::Runtime;
use valqeron_core::common::{Versioned, WriteOutcome};
use valqeron_core::domain::issuer::{Issuer, IssuerId};

pub struct BlockingAdmin<'a> {
    runtime: &'a Runtime,
    inner: AdminService,
}

impl<'a> BlockingAdmin<'a> {
    pub(crate) fn new(runtime: &'a Runtime, inner: AdminService) -> Self {
        Self { runtime, inner }
    }

    pub fn health(&self) -> Result<EngineInfo, ClientError> {
        self.runtime.block_on(self.inner.health())
    }

    pub fn status(&self) -> Result<EngineStatus, ClientError> {
        self.runtime.block_on(self.inner.status())
    }

    pub fn list_background_tasks(&self) -> Result<Vec<BackgroundTaskDetail>, ClientError> {
        self.runtime.block_on(self.inner.list_background_tasks())
    }
}

pub struct BlockingIssuers<'a> {
    runtime: &'a Runtime,
    inner: IssuerService,
}

impl<'a> BlockingIssuers<'a> {
    pub(crate) fn new(runtime: &'a Runtime, inner: IssuerService) -> Self {
        Self { runtime, inner }
    }

    pub fn register(
        &self,
        request: RegisterIssuerRequest,
    ) -> Result<Versioned<Issuer>, ClientError> {
        self.runtime.block_on(self.inner.register(request))
    }

    pub fn get(&self, id: &IssuerId) -> Result<Option<Versioned<Issuer>>, ClientError> {
        self.runtime.block_on(self.inner.get(id))
    }

    pub fn list(
        &self,
        after: Option<&IssuerId>,
        limit: u32,
    ) -> Result<Vec<Versioned<Issuer>>, ClientError> {
        self.runtime.block_on(self.inner.list(after, limit))
    }

    pub fn patch(
        &self,
        id: &IssuerId,
        request: PatchIssuerRequest,
    ) -> Result<WriteOutcome, ClientError> {
        self.runtime.block_on(self.inner.patch(id, request))
    }

    pub fn delete(
        &self,
        id: &IssuerId,
        request: DeleteIssuerRequest,
    ) -> Result<WriteOutcome, ClientError> {
        self.runtime.block_on(self.inner.delete(id, request))
    }
}

// Adding a new service (e.g. SecurityService) means: a BlockingSecurities
// struct here mirroring its async methods 1:1, plus one accessor on `Client`
// in lib.rs. Nothing else changes.
