//! Sliding-window rate limiter: at most `budget` acquisitions in any `window`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;
use types::api::RateClass;
use types::config::ApiConfig;

/// Budgets per [`RateClass`], each an exact sliding-window log. A request that counts
/// toward several buckets is granted in all of them at the same instant, so no bucket
/// ever sees more than its budget in any window.
pub(crate) struct Limiter {
    window: Duration,
    budgets: BTreeMap<RateClass, usize>,
    granted: Mutex<BTreeMap<RateClass, VecDeque<Instant>>>,
}

impl Limiter {
    pub(crate) fn new(
        window: Duration,
        budgets: impl IntoIterator<Item = (RateClass, u32)>,
    ) -> Self {
        let budgets: BTreeMap<_, _> = budgets
            .into_iter()
            .map(|(c, b)| (c, b.max(1) as usize))
            .collect();
        let granted = Mutex::new(budgets.keys().map(|c| (*c, VecDeque::new())).collect());
        Limiter {
            window,
            budgets,
            granted,
        }
    }

    pub(crate) fn from_config(cfg: &ApiConfig) -> Self {
        Limiter::new(
            Duration::from_secs(10),
            [
                (RateClass::Data, cfg.data_per_10s),
                (RateClass::Trades, cfg.trades_per_10s),
                (RateClass::Positions, cfg.positions_per_10s),
                (RateClass::Gamma, cfg.gamma_per_10s),
                (RateClass::Rpc, cfg.rpc_per_10s),
            ],
        )
    }

    /// Waits until one request of `class` (and every bucket it also counts toward) may be sent.
    pub(crate) async fn acquire(&self, class: RateClass) {
        loop {
            let wait_until = {
                let mut granted = self.granted.lock().expect("limiter mutex poisoned");
                let now = Instant::now();
                let mut wait_until: Option<Instant> = None;
                for c in buckets(class) {
                    let (Some(log), Some(budget)) = (granted.get_mut(c), self.budgets.get(c))
                    else {
                        continue;
                    };
                    while log
                        .front()
                        .is_some_and(|t| now.duration_since(*t) >= self.window)
                    {
                        log.pop_front();
                    }
                    if log.len() >= *budget {
                        let free_at = log[log.len() - budget] + self.window;
                        wait_until = Some(wait_until.map_or(free_at, |w| w.max(free_at)));
                    }
                }
                match wait_until {
                    Some(w) => w,
                    None => {
                        for c in buckets(class) {
                            if let Some(log) = granted.get_mut(c) {
                                log.push_back(now);
                            }
                        }
                        return;
                    }
                }
            };
            tokio::time::sleep_until(wait_until).await;
        }
    }
}

/// `/v2/trades`, `/v2/positions` and `/v2/activity` also count toward the general Data bucket.
fn buckets(class: RateClass) -> &'static [RateClass] {
    match class {
        RateClass::Trades => &[RateClass::Trades, RateClass::Data],
        RateClass::Positions => &[RateClass::Positions, RateClass::Data],
        RateClass::Data => &[RateClass::Data],
        RateClass::Gamma => &[RateClass::Gamma],
        RateClass::Rpc => &[RateClass::Rpc],
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn max_in_window(times: &[Instant], window: Duration) -> usize {
        times
            .iter()
            .map(|start| {
                times
                    .iter()
                    .filter(|t| **t >= *start && t.duration_since(*start) < window)
                    .count()
            })
            .max()
            .unwrap_or(0)
    }

    #[tokio::test(start_paused = true)]
    async fn never_exceeds_budget_in_any_window() {
        let lim = Arc::new(Limiter::new(
            Duration::from_secs(10),
            [(RateClass::Positions, 5), (RateClass::Data, 7)],
        ));
        let t0 = Instant::now();
        let mut tasks = Vec::new();
        for i in 0..23 {
            let lim = lim.clone();
            let class = if i % 2 == 0 {
                RateClass::Positions
            } else {
                RateClass::Data
            };
            tasks.push(tokio::spawn(async move {
                lim.acquire(class).await;
                (class, Instant::now())
            }));
        }
        let mut pos = Vec::new();
        let mut all = Vec::new();
        for t in tasks {
            let (class, at) = t.await.unwrap();
            if class == RateClass::Positions {
                pos.push(at);
            }
            all.push(at);
        }
        let w = Duration::from_secs(10);
        assert!(max_in_window(&pos, w) <= 5);
        assert!(
            max_in_window(&all, w) <= 7,
            "positions must also count toward data"
        );
        // 23 requests through a 7/10 s bucket need at least three full windows.
        assert!(Instant::now().duration_since(t0) >= Duration::from_secs(30));
        assert!(Instant::now().duration_since(t0) < Duration::from_secs(41));
    }

    #[tokio::test(start_paused = true)]
    async fn independent_classes_do_not_block_each_other() {
        let lim = Limiter::new(
            Duration::from_secs(10),
            [(RateClass::Gamma, 2), (RateClass::Rpc, 2)],
        );
        let t0 = Instant::now();
        for _ in 0..2 {
            lim.acquire(RateClass::Gamma).await;
            lim.acquire(RateClass::Rpc).await;
        }
        assert_eq!(Instant::now(), t0);
        lim.acquire(RateClass::Rpc).await;
        assert_eq!(Instant::now().duration_since(t0), Duration::from_secs(10));
    }
}
