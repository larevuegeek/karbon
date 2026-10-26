use std::time::Duration;

use rand::RngExt;

/// Waits 50 to 150 ms, jittered. Await it before answering an authentication failure
/// (unknown user, wrong password, bad code) so that response times do not tell which
/// check failed; the jitter keeps the delay from being a recognisable constant.
pub async fn anti_timing_delay() {
    let ms: u64 = rand::rng().random_range(50..=150);

    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waits_between_50_and_150_ms() {
        let start = std::time::Instant::now();
        anti_timing_delay().await;
        let elapsed = start.elapsed().as_millis();

        assert!((50..400).contains(&elapsed), "{elapsed} ms");
    }
}
