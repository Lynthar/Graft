//! Per-site spacing of requests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// One gate per site, shared by every task that talks to sites.
#[derive(Default)]
pub struct RateGates {
    gates: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<Instant>>>>>,
}

impl RateGates {
    /// Wait until a request to `site_id` may be sent, then claim the slot.
    ///
    /// Call it right before every request to the site, including ones that may fail:
    /// a failed request still counts against the site's limit. Concurrent callers
    /// for the same site queue behind each other.
    pub async fn wait(&self, site_id: &str, requests_per_minute: u32) {
        let gate = self
            .gates
            .lock()
            .expect("gate map poisoned")
            .entry(site_id.to_string())
            .or_default()
            .clone();
        let mut last = gate.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep_until(previous + interval(requests_per_minute)).await;
        }
        *last = Some(Instant::now());
    }
}

pub fn interval(requests_per_minute: u32) -> Duration {
    Duration::from_secs(60) / requests_per_minute.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn requests_to_one_site_are_spaced_and_other_sites_are_not_held_up() {
        let gates = RateGates::default();
        let start = Instant::now();
        gates.wait("a", 600).await;
        gates.wait("b", 600).await;
        assert!(start.elapsed() < Duration::from_millis(50));
        gates.wait("a", 600).await;
        gates.wait("a", 600).await;
        assert!(start.elapsed() >= Duration::from_millis(200), "{:?}", start.elapsed());
    }
}
