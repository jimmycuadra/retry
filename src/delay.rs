//! Different types of delay for retryable operations.

use std::time::Duration;

use rand::distributions::{IndependentSample, Range as RandRange};
use rand::thread_rng;

use super::DetermineDelay;

/// Each retry increases the delay since the last exponentially.
#[derive(Debug)]
pub struct Exponential {
    base: u64,
    current: u64,
}

impl Exponential {
    /// Create a new `Exponential` using the given millisecond duration as the initial delay.
    pub fn from_millis(base: u64) -> Self {
        Exponential {
            base: base,
            current: base,
        }
    }
}

impl DetermineDelay for Exponential {
    fn next(&mut self) -> Duration {
        let duration = Duration::from_millis(self.current);

        self.current = self.current * self.base;

        duration
    }
}

/// Each retry uses a fixed delay.
#[derive(Debug)]
pub struct Fixed {
    duration: Duration,
}

impl Fixed {
    /// Create a new `Fixed` using the given duration in milliseconds.
    pub fn from_millis(millis: u64) -> Self {
        Fixed {
            duration: Duration::from_millis(millis),
        }
    }
}

impl DetermineDelay for Fixed {
    fn next(&mut self) -> Duration {
        self.duration
    }
}

/// Each retry happens immediately without any delay.
#[derive(Debug)]
pub struct NoDelay;

impl DetermineDelay for NoDelay {
    fn next(&mut self) -> Duration {
        Duration::default()
    }
}

/// Each retry uses a duration randomly chosen from a range.
#[derive(Debug)]
pub struct Range {
    minimum: u64,
    maximum: u64,
}

impl Range {
    /// Create a new `Range` between the given millisecond durations.
    pub fn from_millis(minimum: u64, maximum: u64) -> Self {
        Range {
            minimum: minimum,
            maximum: maximum,
        }
    }
}

impl DetermineDelay for Range {
    fn next(&mut self) -> Duration {
        let range = RandRange::new(self.minimum, self.maximum);

        let mut rng = thread_rng();

        Duration::from_millis(range.ind_sample(&mut rng))
    }
}
