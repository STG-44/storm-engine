use std::time::Instant;

/// Pause excludes wall time, so resuming never jumps the cloth/sea animation.
pub struct Timeline {
    last: Instant,
    seconds: f64,
    pub paused: bool,
}

impl Timeline {
    pub fn new(seconds: f32, paused: bool) -> Self {
        Self {
            last: Instant::now(),
            seconds: f64::from(seconds),
            paused,
        }
    }

    fn advance(&mut self, now: Instant) -> f32 {
        if !self.paused {
            self.seconds += now.saturating_duration_since(self.last).as_secs_f64();
        }
        self.last = now;
        self.seconds as f32
    }

    pub fn seconds(&mut self) -> f32 {
        self.advance(Instant::now())
    }

    pub fn toggle(&mut self) {
        self.seconds();
        self.paused = !self.paused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pause_preserves_time_and_resume_excludes_the_pause() {
        let mut clock = Timeline::new(4.0, false);
        let start = clock.last;
        assert_eq!(clock.advance(start + Duration::from_secs(2)), 6.0);
        clock.paused = true;
        assert_eq!(clock.advance(start + Duration::from_secs(100)), 6.0);
        clock.paused = false;
        assert_eq!(clock.advance(start + Duration::from_secs(101)), 7.0);
    }
}
