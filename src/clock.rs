use std::time::Duration;

/// A monotonic simulation clock. It advances only when the caller requests it.
#[derive(Debug, Default)]
pub struct ControlledClock {
    now: Duration,
}

impl ControlledClock {
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Advances virtual time without sleeping.
    ///
    /// # Panics
    /// Panics if the new time exceeds the range of `Duration`.
    pub fn advance(&mut self, elapsed: Duration) {
        self.now = self
            .now
            .checked_add(elapsed)
            .expect("simulation time overflow");
    }
}
