//! Synchronization primitives for the SQLite connection layer.
//!
//! Normal builds re-export primitives from `std::sync`. Builds with `--cfg loom` re-export the
//! corresponding instrumented primitives from `loom::sync`.
//!
//! The shim keeps synchronization code independent of the selected implementation. Loom exercises
//! reader-pool checkout and check-in interleaving, including lost-wakeup and deadlock detection.
//! It does not model mutex poisoning; poison recovery is covered by a standard-thread test.

#[cfg(loom)]
pub(crate) use loom::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::{Arc, Condvar, Mutex, MutexGuard};
