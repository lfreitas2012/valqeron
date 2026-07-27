use std::marker::PhantomData;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier<T> {
    value: String,
    _marker: PhantomData<T>,
}

impl<T> Identifier<T> {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            _marker: PhantomData,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone)]
pub struct CnpjMarker;
#[derive(Debug, Clone)]
pub struct LeiMarker;
#[derive(Debug, Clone)]
pub struct IsinMarker;

pub type CnpjIdentifier = Identifier<CnpjMarker>;
pub type LeiIdentifier = Identifier<LeiMarker>;
pub type IsinIdentifier = Identifier<IsinMarker>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub data: T,
    pub version: u32,
}
