//! Synchronization primitive shim.
//!
//! Normal builds use `std::sync`. Under `--cfg loom` (used only by the exhaustive interleaving
//! tests in the [`pool`](crate::sqlite::connection::pool) module) the very same
//! `Arc`/`Mutex`/`Condvar` are swapped for `loom::sync`'s instrumented equivalents, letting loom
//! explore every thread interleaving around `lock()`/`Drop`/`wait()`/`notify_one()`. The
//! concurrency-carrying types in the connection layer (the reader pool, the writer mutex) go through
//! this shim so no `#[cfg]` litters their bodies.
//!
//! Note: loom does not model mutex *poisoning* (a panic aborts the model), so the poison-recovery
//! path in [`lock_writer`](crate::sqlite::connection::pool::lock_writer) is validated by the plain
//! `std::thread` test `poisoned_writer_with_open_transaction_is_healed_on_next_write`, not by loom.
//! Loom's remit here is the reader-pool `Condvar` checkout/checkin interleaving (no lost wakeup, no
//! deadlock).

#[cfg(loom)]
pub(crate) use loom::sync::{Arc, Condvar, Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::{Arc, Condvar, Mutex, MutexGuard};
