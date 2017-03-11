//! Different types of delay for retryable operations.

use std::fmt::{Debug, Formatter, Error as FmtError};
use std::time::Duration;

use rand::distributions::{IndependentSample, Range as RandRange};
use rand::{ThreadRng, thread_rng};

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

impl Iterator for Exponential {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let duration = Duration::from_millis(self.current);

        self.current = self.current * self.base;

        Some(duration)
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

impl Iterator for Fixed {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(self.duration)
    }
}

/// Each retry happens immediately without any delay.
#[derive(Debug)]
pub struct NoDelay;

impl Iterator for NoDelay {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(Duration::default())
    }
}

/// Each retry uses a duration randomly chosen from a range.
pub struct Range {
    range: RandRange<u64>,
    rng: ThreadRng,
}

impl Range {
    /// Create a new `Range` between the given millisecond durations.
    pub fn from_millis(minimum: u64, maximum: u64) -> Self {
        Range {
            range: RandRange::new(minimum, maximum),
            rng: thread_rng(),
        }
    }
}

impl Iterator for Range {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(Duration::from_millis(self.range.ind_sample(&mut self.rng)))
    }
}

impl Debug for Range {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "Range {{ range: RandRange<u64>, rng: ThreadRng }}")
    }
}
