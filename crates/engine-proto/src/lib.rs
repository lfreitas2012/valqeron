pub mod v1 {
    tonic::include_proto!("valqeron.v1");
}

pub use prost_types;

pub const PROTOCOL_VERSION: u32 = 1;
