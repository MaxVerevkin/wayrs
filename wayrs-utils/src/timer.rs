use std::time::{Duration, Instant};

/// A simple timer. Useful for keyboard-repeat.
#[derive(Debug)]
pub struct Timer {
    next_fire: Instant,
    interval: Duration,
}

impl Timer {
    /// Create a new timer with a given delay and interval.
    pub fn new(delay: Duration, interval: Duration) -> Self {
        Self {
            next_fire: Instant::now() + delay,
            interval,
        }
    }

    /// Update the internal state and check if timer fired.
    ///
    /// Returns `true` if timer has fired. `false` otherwise.
    ///
    /// Regularly call thin function in your event loop.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        if now >= self.next_fire {
            self.next_fire += self.interval;
            true
        } else {
            false
        }
    }

    /// The duration untill next fire.
    pub fn sleep(&self) -> Duration {
        self.next_fire.saturating_duration_since(Instant::now())
    }
}
