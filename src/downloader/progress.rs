use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct ProgressUpdate {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: f64,
    pub percentage: Option<f64>,
}

impl ProgressUpdate {
    pub fn speed_human(&self) -> String {
        human_bytes(self.speed_bytes_per_sec) + "/s"
    }

    pub fn downloaded_human(&self) -> String {
        human_bytes(self.bytes_downloaded as f64)
    }

    pub fn total_human(&self) -> Option<String> {
        self.total_bytes.map(|t| human_bytes(t as f64))
    }
}

fn human_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{value:.0} {}", UNITS[unit_idx])
    } else {
        format!("{value:.2} {}", UNITS[unit_idx])
    }
}

pub(crate) struct SpeedTracker {
    start: Instant,
    last_sample_at: Instant,
    last_sample_bytes: u64,
    ema_speed: f64,
    // Smoothing factor: higher = more reactive, lower = smoother.
    alpha: f64,
}

impl SpeedTracker {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last_sample_at: now,
            last_sample_bytes: 0,
            ema_speed: 0.0,
            alpha: 0.3,
        }
    }

    /// Call this each time new bytes have arrived, with the *cumulative*
    /// total downloaded so far. Returns the current smoothed speed.
    pub fn sample(&mut self, cumulative_bytes: u64) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample_at);

        // Avoid division by a near-zero duration on very fast/local links;
        // just keep the previous estimate until enough time has passed.
        if elapsed < Duration::from_millis(50) && self.ema_speed > 0.0 {
            return self.ema_speed;
        }

        let delta_bytes = cumulative_bytes.saturating_sub(self.last_sample_bytes);
        let instant_speed = delta_bytes as f64 / elapsed.as_secs_f64().max(0.001);

        self.ema_speed = if self.ema_speed == 0.0 {
            instant_speed
        } else {
            self.alpha * instant_speed + (1.0 - self.alpha) * self.ema_speed
        };

        self.last_sample_at = now;
        self.last_sample_bytes = cumulative_bytes;
        self.ema_speed
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::human_bytes;

    #[test]
    fn human_bytes_formatting() {
        assert_eq!(human_bytes(500.0), "500 B");
        assert_eq!(human_bytes(2048.0), "2.00 KiB");
        assert_eq!(human_bytes(5_242_880.0), "5.00 MiB");
    }
}