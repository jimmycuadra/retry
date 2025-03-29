use std::{
    ops::{Range as StdRange, RangeInclusive},
    time::Duration,
};

use rand::{
    distr::{uniform::Error as UniformError, Distribution, Uniform},
    random,
    rngs::ThreadRng,
};

/// Each retry uses a duration randomly chosen from a range. (When the `random` Cargo feature is
/// enabled.)
#[derive(Debug)]
pub struct Range {
    distribution: Uniform<u64>,
    rng: ThreadRng,
}

impl Range {
    /// Create a new [`Range`] between the given millisecond durations, excluding the maximum value.
    ///
    /// # Panics
    ///
    /// Panics if the minimum is greater than or equal to the maximum.
    pub fn from_millis_exclusive(minimum: u64, maximum: u64) -> Self {
        Range {
            distribution: Uniform::new(minimum, maximum)
                .expect("minimum must be less than maximum"),
            rng: rand::rng(),
        }
    }

    /// Attempt to create a new [`Range`] between the given millisecond durations, excluding the
    /// maximum value.
    ///
    /// # Errors
    ///
    /// Returns an error if the minimum is greater than or equal to the maximum.
    pub fn try_from_millis_exclusive(minimum: u64, maximum: u64) -> Result<Self, UniformError> {
        Ok(Range {
            distribution: Uniform::new(minimum, maximum)?,
            rng: rand::rng(),
        })
    }

    /// Create a new [`Range`] between the given millisecond durations, including the maximum value.
    ///
    /// # Panics
    ///
    /// Panics if the minimum is greater than or equal to the maximum.
    pub fn from_millis_inclusive(minimum: u64, maximum: u64) -> Self {
        Range {
            distribution: Uniform::new_inclusive(minimum, maximum)
                .expect("minimum must be less than maximum"),
            rng: rand::rng(),
        }
    }

    /// Attempt to create a new [`Range`] between the given millisecond durations, including the
    /// maximum value.
    ///
    /// # Errors
    ///
    /// Returns an error if the minimum is greater than or equal to the maximum.
    pub fn try_from_millis_inclusive(minimum: u64, maximum: u64) -> Result<Self, UniformError> {
        Ok(Range {
            distribution: Uniform::new_inclusive(minimum, maximum)?,
            rng: rand::rng(),
        })
    }
}

impl Iterator for Range {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(Duration::from_millis(
            self.distribution.sample(&mut self.rng),
        ))
    }
}

impl From<StdRange<Duration>> for Range {
    fn from(range: StdRange<Duration>) -> Self {
        Self::from_millis_exclusive(range.start.as_millis() as u64, range.end.as_millis() as u64)
    }
}

impl From<RangeInclusive<Duration>> for Range {
    fn from(range: RangeInclusive<Duration>) -> Self {
        Self::from_millis_inclusive(
            range.start().as_millis() as u64,
            range.end().as_millis() as u64,
        )
    }
}

/// Apply full random jitter to a duration. (When the `random` Cargo feature is enabled.)
pub fn jitter(duration: Duration) -> Duration {
    let jitter = random::<f64>();
    let secs = ((duration.as_secs() as f64) * jitter).ceil() as u64;
    let nanos = ((f64::from(duration.subsec_nanos())) * jitter).ceil() as u32;
    Duration::new(secs, nanos)
}

#[test]
fn range_uniform() {
    let mut range = Range::from_millis_exclusive(0, 1);
    assert_eq!(Duration::from_millis(0), range.next().unwrap());
    assert_eq!(Duration::from_millis(0), range.next().unwrap());
    assert_eq!(Duration::from_millis(0), range.next().unwrap());
}

#[test]
#[should_panic]
fn range_exclusive_uniform_wrong_input() {
    Range::from_millis_exclusive(1, 0);
}

#[test]
#[should_panic]
fn range_inclusive_uniform_wrong_input() {
    Range::from_millis_inclusive(1, 0);
}

#[test]
fn try_range_uniform() {
    assert!(Range::try_from_millis_exclusive(0, 1).is_ok());
    assert!(Range::try_from_millis_inclusive(0, 1).is_ok());

    assert!(Range::try_from_millis_exclusive(1, 0).is_err());
    assert!(Range::try_from_millis_inclusive(1, 0).is_err());
}

#[test]
fn test_jitter() {
    assert_eq!(Duration::from_millis(0), jitter(Duration::from_millis(0)));
    assert!(Duration::from_millis(0) < jitter(Duration::from_millis(2)));
}
