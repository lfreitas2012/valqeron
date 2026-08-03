#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub data: T,
    pub version: u32,
}
