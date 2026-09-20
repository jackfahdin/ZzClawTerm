use std::collections::VecDeque;
use std::time::{Duration, Instant};

const SPEED_WINDOW: Duration = Duration::from_secs(3);

/// Recent-byte sampling shared by transfer protocols, independent of wire formats.
#[derive(Debug, Clone, Default)]
pub struct TransferSpeed {
    samples: VecDeque<(Instant, u64)>,
    bytes_per_second: f64,
}

impl TransferSpeed {
    pub fn record(&mut self, bytes_transferred: u64, now: Instant) {
        if let Some(&(previous_time, previous_bytes)) = self.samples.back() {
            if now < previous_time {
                return;
            }
            if bytes_transferred < previous_bytes {
                self.reset();
            } else if bytes_transferred == previous_bytes {
                if now.duration_since(previous_time) > SPEED_WINDOW {
                    self.bytes_per_second = 0.;
                }
                return;
            } else if now == previous_time {
                // A coalesced UI batch may deliver multiple updates at one timestamp.
                self.samples.pop_back();
            }
        }
        while self
            .samples
            .front()
            .is_some_and(|(time, _)| now.duration_since(*time) > SPEED_WINDOW)
        {
            self.samples.pop_front();
        }
        self.samples.push_back((now, bytes_transferred));
        let &(first_time, first_bytes) = self.samples.front().expect("sample just inserted");
        let elapsed = now.duration_since(first_time).as_secs_f64();
        self.bytes_per_second = if elapsed > 0. {
            ((bytes_transferred - first_bytes) as f64 / elapsed).round()
        } else {
            0.
        };
    }

    pub fn bytes_per_second(&self) -> f64 {
        self.bytes_per_second
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.bytes_per_second = 0.;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::TransferSpeed;

    #[test]
    fn calculates_speed_from_recent_bytes_not_lifetime_average() {
        let start = Instant::now();
        let mut speed = TransferSpeed::default();
        speed.record(0, start);
        speed.record(1024, start + Duration::from_secs(1));
        assert_eq!(speed.bytes_per_second(), 1024.);
        speed.record(2048, start + Duration::from_secs(2));
        speed.record(10_240, start + Duration::from_secs(4));
        assert_eq!(speed.bytes_per_second(), 3072.);
    }

    #[test]
    fn first_sample_and_restarted_byte_counter_have_zero_speed() {
        let start = Instant::now();
        let mut speed = TransferSpeed::default();
        speed.record(1024, start);
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(2048, start + Duration::from_secs(1));
        speed.record(10, start + Duration::from_secs(2));
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(522, start + Duration::from_secs(3));
        assert_eq!(speed.bytes_per_second(), 512.);
    }

    #[test]
    fn duplicate_updates_keep_rate_until_stale_and_long_gaps_restart_sampling() {
        let start = Instant::now();
        let mut speed = TransferSpeed::default();
        speed.record(0, start);
        speed.record(1024, start + Duration::from_secs(1));
        speed.record(1024, start + Duration::from_secs(2));
        assert_eq!(speed.bytes_per_second(), 1024.);
        speed.record(1024, start + Duration::from_secs(5));
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(2048, start + Duration::from_secs(6));
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(4096, start + Duration::from_secs(7));
        assert_eq!(speed.bytes_per_second(), 2048.);
    }

    #[test]
    fn reset_discards_pre_pause_samples_and_coalesced_updates_remain_finite() {
        let start = Instant::now();
        let mut speed = TransferSpeed::default();
        speed.record(0, start);
        speed.record(1024, start + Duration::from_secs(1));
        speed.record(2048, start + Duration::from_secs(1));
        assert_eq!(speed.bytes_per_second(), 2048.);
        speed.reset();
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(2048, start + Duration::from_secs(2));
        speed.record(3072, start + Duration::from_secs(2));
        assert_eq!(speed.bytes_per_second(), 0.);
        speed.record(4096, start + Duration::from_secs(3));
        assert_eq!(speed.bytes_per_second(), 1024.);
    }
}
