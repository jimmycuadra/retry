//! Crate `retry` provides utilities for retrying operations that can fail.
//!
//! # Usage
//!
//! Retry an operation using the `retry` function. `retry` accepts an iterator over `Duration`s and
//! a closure that returns a `Result`. The iterator is used to determine how long to wait after
//! each unsuccessful try and how many times to try before giving up and returning `Result::Err`.
//!
//! Any type that implements `Iterator<Duration>` can be used to determine retry behavior, though a
//! few useful implementations are provided in the `delay` module, including a fixed delay and
//! exponential back-off.
//!
//! ```
//! # use retry::retry;
//! # use retry::delay::Fixed;
//! let mut collection = vec![1, 2, 3].into_iter();
//!
//! let result = retry(Fixed::from_millis(100), || {
//!     match collection.next() {
//!         Some(n) if n == 3 => Ok("n is 3!"),
//!         Some(_) => Err("n must be 3!"),
//!         None => Err("n was never 3!"),
//!     }
//! });
//!
//! assert!(result.is_ok());
//! ```
//!
//! The `Iterator` API can be used to limit or modify the delay strategy. For example, to limit the
//! number of retries to 1:
//!
//! ```
//! # use retry::retry;
//! # use retry::delay::Fixed;
//! let mut collection = vec![1, 2, 3].into_iter();
//!
//! let result = retry(Fixed::from_millis(100).take(1), || {
//!     match collection.next() {
//!         Some(n) if n == 3 => Ok("n is 3!"),
//!         Some(_) => Err("n must be 3!"),
//!         None => Err("n was never 3!"),
//!     }
//! });
//!
//! assert!(result.is_err());
//! ```

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]

extern crate rand;

use std::error::Error as StdError;
use std::fmt::Error as FmtError;
use std::fmt::{Display,Formatter};
use std::thread::sleep;
use std::time::Duration;

pub mod delay;

/// Retry the given operation synchronously until it succeeds, or until the given `Duration`
/// iterator ends.
pub fn retry<I, O, R, E>(iterable: I, mut operation: O) -> Result<R, Error<E>>
where I: IntoIterator<Item=Duration>, O: FnMut() -> Result<R, E> {
    let mut iterator = iterable.into_iter();
    let mut try = 1;
    let mut total_delay = Duration::default();

    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if let Some(delay) = iterator.next() {
                    sleep(delay);
                    try += 1;
                    total_delay += delay;
                } else {
                    return Err(Error::Operation {
                        error: error,
                        total_delay: total_delay,
                        tries: try,
                    });
                }
            }
        }
    }
}

/// An error with a retryable operation.
#[derive(Debug)]
pub enum Error<E> {
    /// The operation's last error, plus the number of times the operation was tried and the
    /// duration spent waiting between tries.
    Operation {
        /// The error returned by the operation on the last try.
        error: E,
        /// The duration spent waiting between retries of the operation.
        ///
        /// Note that this does not include the time spent running the operation itself.
        total_delay: Duration,
        /// The total number of times the operation was tried.
        tries: u64,
    }
}

impl<E> Display for Error<E> where E: StdError {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        write!(formatter, "{}", self.description())
    }
}

impl<E> StdError for Error<E> where E: StdError {
    fn description(&self) -> &str {
        match *self {
            Error::Operation { ref error, .. } => error.description(),
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::Operation { ref error, .. } => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Error, retry};
    use super::delay::{Exponential, Fixed, NoDelay, Range};

    #[test]
    fn succeeds_with_infinite_retries() {
        let mut collection = vec![1, 2, 3, 4, 5].into_iter();

        let value = retry(NoDelay, || {
            match collection.next() {
                Some(n) if n == 5 => Ok(n),
                Some(_) => Err("not 5"),
                None => Err("not 5"),
            }
        }).unwrap();

        assert_eq!(value, 5);
    }

    #[test]
    fn succeeds_with_maximum_retries() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(NoDelay.take(1), || {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        }).unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn fails_after_last_try() {
        let mut collection = vec![1].into_iter();

        let Error::Operation { error, total_delay, tries } =
            retry(NoDelay.take(1), || {
                match collection.next() {
                    Some(n) if n == 2 => Ok(n),
                    Some(_) => Err("not 2"),
                    None => Err("not 2"),
                }
        }).err().unwrap();

        assert_eq!(error, "not 2");
        assert_eq!(total_delay, Duration::default());
        assert_eq!(tries, 2);
    }

    #[test]
    fn succeeds_with_fixed_delay() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(Fixed::from_millis(1), || {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        }).unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn succeeds_with_exponential_delay() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(Exponential::from_millis(1), || {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        }).unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn succeeds_with_ranged_delay() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(Range::from_millis(1, 10), || {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        }).unwrap();

        assert_eq!(value, 2);
    }
}
