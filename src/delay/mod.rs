//! Different types of delay for retryable operations.

use std::time::Duration;
use std::u64::MAX as U64_MAX;

#[cfg(feature = "random")]
mod random;

#[cfg(feature = "random")]
pub use random::{jitter, Range};

/// Each retry increases the delay since the last exponentially.
#[derive(Debug)]
pub struct Exponential {
    current: u64,
    factor: f64,
}

impl Exponential {
    /// Create a new [`Exponential`] using the given millisecond duration as the initial delay and
    /// an exponential backoff factor of `2.0`.
    pub fn from_millis(base: u64) -> Self {
        Exponential {
            current: base,
            factor: 2.0,
        }
    }

    /// Create a new [`Exponential`] using the given millisecond duration as the initial delay and
    /// the same duration as the exponential backoff factor. This was the behavior of
    /// [`Exponential::from_millis`] prior to version 2.0.
    pub fn from_millis_with_base_factor(base: u64) -> Self {
        Exponential {
            current: base,
            factor: base as f64,
        }
    }

    /// Create a new [`Exponential`] using the given millisecond duration as the initial delay and
    /// the given exponential backoff factor.
    pub fn from_millis_with_factor(base: u64, factor: f64) -> Self {
        Exponential {
            current: base,
            factor,
        }
    }
}

impl Iterator for Exponential {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let duration = Duration::from_millis(self.current);

        let next = (self.current as f64) * self.factor;
        self.current = if next > (U64_MAX as f64) {
            U64_MAX
        } else {
            next as u64
        };

        Some(duration)
    }
}

impl From<Duration> for Exponential {
    fn from(duration: Duration) -> Self {
        Self::from_millis(duration.as_millis() as u64)
    }
}

#[test]
fn exponential_with_factor() {
    let mut iter = Exponential::from_millis_with_factor(1000, 2.0);
    assert_eq!(iter.next(), Some(Duration::from_millis(1000)));
    assert_eq!(iter.next(), Some(Duration::from_millis(2000)));
    assert_eq!(iter.next(), Some(Duration::from_millis(4000)));
    assert_eq!(iter.next(), Some(Duration::from_millis(8000)));
    assert_eq!(iter.next(), Some(Duration::from_millis(16000)));
    assert_eq!(iter.next(), Some(Duration::from_millis(32000)));
}

#[test]
fn exponential_overflow() {
    let mut iter = Exponential::from_millis(U64_MAX);
    assert_eq!(iter.next(), Some(Duration::from_millis(U64_MAX)));
    assert_eq!(iter.next(), Some(Duration::from_millis(U64_MAX)));
}

/// Each retry uses a delay which is the sum of the two previous delays.
///
/// Depending on the problem at hand, a fibonacci delay strategy might perform better and lead to
/// better throughput than the [`Exponential`] strategy.
///
/// See ["A Performance Comparison of Different Backoff Algorithms under Different Rebroadcast
/// Probabilities for MANETs"](https://www.researchgate.net/publication/255672213_A_Performance_Comparison_of_Different_Backoff_Algorithms_under_Different_Rebroadcast_Probabilities_for_MANET's)
/// for more details.
#[derive(Debug)]
pub struct Fibonacci {
    curr: u64,
    next: u64,
}

impl Fibonacci {
    /// Create a new [`Fibonacci`] using the given duration in milliseconds.
    pub fn from_millis(millis: u64) -> Fibonacci {
        Fibonacci {
            curr: millis,
            next: millis,
        }
    }
}

impl Iterator for Fibonacci {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let duration = Duration::from_millis(self.curr);

        if let Some(next_next) = self.curr.checked_add(self.next) {
            self.curr = self.next;
            self.next = next_next;
        } else {
            self.curr = self.next;
            self.next = U64_MAX;
        }

        Some(duration)
    }
}

impl From<Duration> for Fibonacci {
    fn from(duration: Duration) -> Self {
        Self::from_millis(duration.as_millis() as u64)
    }
}

#[test]
fn fibonacci() {
    let mut iter = Fibonacci::from_millis(10);
    assert_eq!(iter.next(), Some(Duration::from_millis(10)));
    assert_eq!(iter.next(), Some(Duration::from_millis(10)));
    assert_eq!(iter.next(), Some(Duration::from_millis(20)));
    assert_eq!(iter.next(), Some(Duration::from_millis(30)));
    assert_eq!(iter.next(), Some(Duration::from_millis(50)));
    assert_eq!(iter.next(), Some(Duration::from_millis(80)));
}

#[test]
fn fibonacci_saturated() {
    let mut iter = Fibonacci::from_millis(U64_MAX);
    assert_eq!(iter.next(), Some(Duration::from_millis(U64_MAX)));
    assert_eq!(iter.next(), Some(Duration::from_millis(U64_MAX)));
}

/// Each retry uses a fixed delay.
#[derive(Debug)]
pub struct Fixed {
    duration: Duration,
}

impl Fixed {
    /// Create a new [`Fixed`] using the given duration in milliseconds.
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

impl From<Duration> for Fixed {
    fn from(delay: Duration) -> Self {
        Self {
            duration: delay.into(),
        }
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
