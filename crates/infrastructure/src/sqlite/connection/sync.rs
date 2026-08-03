#[cfg(loom)]
pub(crate) use loom::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::{Arc, Condvar, Mutex, MutexGuard};
