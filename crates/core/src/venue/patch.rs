use crate::common::{Empty, NonEmpty};
use crate::venue::{VenueName, VenueStatus};
use std::marker::PhantomData;

/// Partial update for a venue. The MIC is the venue's identity and is not
/// patchable (retiring one MIC and registering another is a new venue), and
/// the operating MIC and country are derived from the ISO 10383 registry, so
/// only the display name and lifecycle status remain mutable.
#[derive(Debug, Clone)]
pub struct VenuePatch {
    pub(crate) name: Option<VenueName>,
    pub(crate) status: Option<VenueStatus>,
}

impl VenuePatch {
    #[must_use]
    pub const fn builder() -> VenuePatchBuilder<Empty> {
        VenuePatchBuilder::new()
    }

    const fn empty() -> Self {
        Self {
            name: None,
            status: None,
        }
    }

    #[must_use]
    pub const fn name(&self) -> Option<&VenueName> {
        self.name.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> Option<VenueStatus> {
        self.status
    }
}

pub struct VenuePatchBuilder<State> {
    inner: VenuePatch,
    _state: PhantomData<State>,
}

impl VenuePatchBuilder<Empty> {
    const fn new() -> Self {
        Self {
            inner: VenuePatch::empty(),
            _state: PhantomData,
        }
    }
}

impl<State> VenuePatchBuilder<State> {
    #[must_use]
    pub fn name(self, name: VenueName) -> VenuePatchBuilder<NonEmpty> {
        VenuePatchBuilder {
            inner: VenuePatch {
                name: Some(name),
                ..self.inner
            },
            _state: PhantomData,
        }
    }

    #[must_use]
    pub fn status(self, status: VenueStatus) -> VenuePatchBuilder<NonEmpty> {
        VenuePatchBuilder {
            inner: VenuePatch {
                status: Some(status),
                ..self.inner
            },
            _state: PhantomData,
        }
    }
}

impl VenuePatchBuilder<NonEmpty> {
    #[must_use]
    pub fn build(self) -> VenuePatch {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_field_patch_only_sets_that_field() {
        let status_patch = VenuePatch::builder().status(VenueStatus::Retired).build();

        assert!(matches!(status_patch.status(), Some(VenueStatus::Retired)));
        assert!(status_patch.name().is_none());
    }
}
