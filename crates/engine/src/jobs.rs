//! Minimal periodic-job runner for the engine's background work.
//!
//! Each job is one spawned task that owns its own timer; the job body is
//! awaited inline on that task, so overlapping runs are impossible by
//! construction (no busy-flag bookkeeping). Missed ticks are skipped
//! (`MissedTickBehavior::Skip`): a body that overruns its period never causes
//! a catch-up burst. First ticks land one full period after spawn — the
//! startup banner already covers "the engine is alive".
//!
//! Shutdown is a two-step protocol driven by [`JobSet::drain`]: a `watch`
//! flip stops every ticker, and a bounded `JoinSet` drain waits for bodies
//! still in flight. Jobs that outlive the deadline are reported, not awaited
//! forever — the runtime shutdown backstop handles truly stuck work.

use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

/// Description of one periodic background job.
pub(crate) struct PeriodicJob {
    /// Name used in logs.
    pub name: &'static str,
    /// Base tick period.
    pub period: Duration,
    /// Apply ±10% jitter to the period so periodic jobs do not synchronize
    /// with other periodic load.
    pub jitter: bool,
}

/// Owns the spawned job tasks and their shared shutdown signal.
pub(crate) struct JobSet {
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: JoinSet<()>,
}

impl JobSet {
    pub fn new() -> Self {
        let (shutdown, _) = tokio::sync::watch::channel(false);
        Self {
            shutdown,
            tasks: JoinSet::new(),
        }
    }

    /// Spawn a periodic job. `run` produces one future per tick; the future
    /// is awaited before the next tick is considered, so runs never overlap.
    pub fn spawn<F, Fut>(&mut self, job: PeriodicJob, mut run: F)
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let mut shutdown = self.shutdown.subscribe();
        self.tasks.spawn(async move {
            let period = if job.jitter {
                jittered(job.period)
            } else {
                job.period
            };
            let first_tick = tokio::time::Instant::now()
                .checked_add(period)
                .unwrap_or_else(tokio::time::Instant::now);
            let mut ticker = tokio::time::interval_at(first_tick, period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        tracing::debug!(job = job.name, "job stopped");
                        break;
                    }
                    _ = ticker.tick() => run().await,
                }
            }
        });
    }

    /// Stop every ticker and wait (bounded) for in-flight job bodies to
    /// finish. Returns `true` when all jobs exited within the deadline.
    pub async fn drain(mut self, deadline: Duration) -> bool {
        let _ = self.shutdown.send(true);
        let all_done = async { while self.tasks.join_next().await.is_some() {} };
        tokio::time::timeout(deadline, all_done).await.is_ok()
    }
}

/// Scale a period into 90%..=110% using clock sub-second noise — enough to
/// desynchronize periodic jobs without pulling in an RNG dependency.
fn jittered(base: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let percent = u64::from(90u32.saturating_add(nanos.checked_rem(21).unwrap_or(0)));
    let millis = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let scaled = millis
        .saturating_mul(percent)
        .checked_div(100)
        .unwrap_or(millis);
    if scaled == 0 {
        base
    } else {
        Duration::from_millis(scaled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn jitter_stays_within_ten_percent() {
        let base = Duration::from_secs(3600);
        for _ in 0..50 {
            let j = jittered(base);
            assert!(
                j >= Duration::from_secs(3240) && j <= Duration::from_secs(3960),
                "jittered value out of ±10% envelope: {j:?}"
            );
        }
    }

    #[test]
    fn jitter_never_zeroes_a_tiny_period() {
        assert!(jittered(Duration::from_millis(1)) > Duration::ZERO);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn job_ticks_repeatedly_and_drains_on_shutdown() {
        let mut jobs = JobSet::new();
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        jobs.spawn(
            PeriodicJob {
                name: "test.tick",
                period: Duration::from_millis(10),
                jitter: false,
            },
            move || {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            },
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while ticks.load(Ordering::SeqCst) < 3 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "job never reached three ticks"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        assert!(
            jobs.drain(Duration::from_secs(1)).await,
            "jobs must drain within the deadline"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overlong_body_never_overlaps_and_drain_waits_for_it() {
        let mut jobs = JobSet::new();
        let running = Arc::new(AtomicUsize::new(0));
        let overlaps = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));

        let (running_c, overlaps_c, started_c) = (
            Arc::clone(&running),
            Arc::clone(&overlaps),
            Arc::clone(&started),
        );
        jobs.spawn(
            PeriodicJob {
                name: "test.slow",
                period: Duration::from_millis(5),
                jitter: false,
            },
            move || {
                let running = Arc::clone(&running_c);
                let overlaps = Arc::clone(&overlaps_c);
                let started = Arc::clone(&started_c);
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    if running.fetch_add(1, Ordering::SeqCst) > 0 {
                        overlaps.fetch_add(1, Ordering::SeqCst);
                    }
                    // Body deliberately longer than the period.
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    running.fetch_sub(1, Ordering::SeqCst);
                }
            },
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while started.load(Ordering::SeqCst) < 2 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "slow job never reached two runs"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        assert!(jobs.drain(Duration::from_secs(1)).await);
        assert_eq!(
            overlaps.load(Ordering::SeqCst),
            0,
            "job bodies must never overlap"
        );
        assert_eq!(
            running.load(Ordering::SeqCst),
            0,
            "drain waits for the body"
        );
    }
}
