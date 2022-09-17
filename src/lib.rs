//! Crate `retry` provides utilities for retrying operations that can fail.
//!
//! # Usage
//!
//! Retry an operation using the [`retry`] function. [`retry`] accepts an iterator over
//! [`Duration`]s and a closure that returns a [`Result`] (or [`OperationResult`]; see below). The
//! iterator is used to determine how long to wait after each unsuccessful try and how many times to
//! try before giving up and returning [`Result::Err`]. The closure determines either the final
//! successful value, or an error value, which can either be returned immediately or used to
//! indicate that the operation should be retried.
//!
//! Any type that implements [`Iterator`] with an associated `Item` type of [`Duration`] can be
//! used to determine retry behavior, though a few useful implementations are provided in the
//! [`delay`] module, including a fixed delay and exponential backoff.
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
//! The [`Iterator`] API can be used to limit or modify the delay strategy. For example, to limit
//! the number of retries to 1:
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
//!
#![cfg_attr(
    feature = "random",
    doc = r##"
To apply random jitter to any delay strategy, the [`delay::jitter`] function can be used in
combination with the [`Iterator`] API:

```
# use retry::retry;
# use retry::delay::{Exponential, jitter};
let mut collection = vec![1, 2, 3].into_iter();

let result = retry(Exponential::from_millis(10).map(jitter).take(3), || {
    match collection.next() {
        Some(n) if n == 3 => Ok("n is 3!"),
        Some(_) => Err("n must be 3!"),
        None => Err("n was never 3!"),
    }
});

assert!(result.is_ok());
```
"##
)]
//!
//! To deal with fatal errors, return [`OperationResult`], which is like [`Result`], but with a
//! third case to distinguish between errors that should cause a retry and errors that should
//! immediately return, halting retry behavior. (Internally, [`OperationResult`] is always used, and
//! closures passed to [`retry`] that return plain [`Result`] are converted into
//! [`OperationResult`].)
//!
//! ```
//! # use retry::retry;
//! # use retry::delay::Fixed;
//! use retry::OperationResult;
//! let mut collection = vec![1, 2].into_iter();
//! let value = retry(Fixed::from_millis(1), || {
//!     match collection.next() {
//!         Some(n) if n == 2 => OperationResult::Ok(n),
//!         Some(_) => OperationResult::Retry("not 2"),
//!         None => OperationResult::Err("not found"),
//!     }
//! }).unwrap();
//!
//! assert_eq!(value, 2);
//! ```
//!
//! If your operation needs to know how many times it's been tried, use the [`retry_with_index`]
//! function. This works the same as [`retry`], but passes the number of the current try to the
//! closure as an argument.
//!
//! ```
//! # use retry::retry_with_index;
//! # use retry::delay::Fixed;
//! # use retry::OperationResult;
//! let mut collection = vec![1, 2, 3, 4, 5].into_iter();
//!
//! let result = retry_with_index(Fixed::from_millis(100), |current_try| {
//!     if current_try > 3 {
//!         return OperationResult::Err("did not succeed within 3 tries");
//!     }
//!
//!     match collection.next() {
//!         Some(n) if n == 5 => OperationResult::Ok("n is 5!"),
//!         Some(_) => OperationResult::Retry("n must be 5!"),
//!         None => OperationResult::Retry("n was never 5!"),
//!     }
//! });
//!
//! assert!(result.is_err());
//! ```
//!
//! # Features
//!
//! - `random`: offer some random delay utilities (on by default)

#![deny(missing_debug_implementations, missing_docs, warnings)]

use std::{
    error::Error as StdError,
    fmt::{Display, Error as FmtError, Formatter},
    thread::sleep,
    time::Duration,
};

pub mod delay;
mod opresult;

#[doc(inline)]
pub use opresult::OperationResult;

/// Retry the given operation synchronously until it succeeds, or until the given [`Duration`]
/// iterator ends.
pub fn retry<I, O, R, E, OR>(iterable: I, mut operation: O) -> Result<R, Error<E>>
where
    I: IntoIterator<Item = Duration>,
    O: FnMut() -> OR,
    OR: Into<OperationResult<R, E>>,
{
    retry_with_index(iterable, |_| operation())
}

/// Retry the given operation synchronously until it succeeds, or until the given [`Duration`]
/// iterator ends, with each iteration of the operation receiving the number of the attempt as an
/// argument.
pub fn retry_with_index<I, O, R, E, OR>(iterable: I, mut operation: O) -> Result<R, Error<E>>
where
    I: IntoIterator<Item = Duration>,
    O: FnMut(u64) -> OR,
    OR: Into<OperationResult<R, E>>,
{
    let mut iterator = iterable.into_iter();
    let mut current_try = 1;
    let mut total_delay = Duration::default();

    loop {
        match operation(current_try).into() {
            OperationResult::Ok(value) => return Ok(value),
            OperationResult::Retry(error) => {
                if let Some(delay) = iterator.next() {
                    sleep(delay);
                    current_try += 1;
                    total_delay += delay;
                } else {
                    return Err(Error {
                        error,
                        total_delay,
                        tries: current_try,
                    });
                }
            }
            OperationResult::Err(error) => {
                return Err(Error {
                    error,
                    total_delay,
                    tries: current_try,
                });
            }
        }
    }
}

/// An error with a retryable operation.
#[derive(Debug, PartialEq, Eq)]
pub struct Error<E> {
    /// The error returned by the operation on the last try.
    pub error: E,
    /// The duration spent waiting between retries of the operation.
    ///
    /// Note that this does not include the time spent running the operation itself.
    pub total_delay: Duration,
    /// The total number of times the operation was tried.
    pub tries: u64,
}

impl<E> Display for Error<E>
where
    E: Display,
{
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        Display::fmt(&self.error, formatter)
    }
}

impl<E> StdError for Error<E>
where
    E: StdError,
{
    #[allow(deprecated)]
    fn description(&self) -> &str {
        self.error.description()
    }

    fn cause(&self) -> Option<&dyn StdError> {
        Some(&self.error)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::delay::{Exponential, Fixed, NoDelay};
    use super::opresult::OperationResult;
    use super::{retry, retry_with_index, Error};

    #[test]
    fn succeeds_with_infinite_retries() {
        let mut collection = vec![1, 2, 3, 4, 5].into_iter();

        let value = retry(NoDelay, || match collection.next() {
            Some(n) if n == 5 => Ok(n),
            Some(_) => Err("not 5"),
            None => Err("not 5"),
        })
        .unwrap();

        assert_eq!(value, 5);
    }

    #[test]
    fn succeeds_with_maximum_retries() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(NoDelay.take(1), || match collection.next() {
            Some(n) if n == 2 => Ok(n),
            Some(_) => Err("not 2"),
            None => Err("not 2"),
        })
        .unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn fails_after_last_try() {
        let mut collection = vec![1].into_iter();

        let res = retry(NoDelay.take(1), || match collection.next() {
            Some(n) if n == 2 => Ok(n),
            Some(_) => Err("not 2"),
            None => Err("not 2"),
        });

        assert_eq!(
            res,
            Err(Error {
                error: "not 2",
                tries: 2,
                total_delay: Duration::from_millis(0)
            })
        );
    }

    #[test]
    fn fatal_errors() {
        let mut collection = vec![1].into_iter();

        let res = retry(NoDelay.take(2), || match collection.next() {
            Some(n) if n == 2 => OperationResult::Ok(n),
            Some(_) => OperationResult::Err("no retry"),
            None => OperationResult::Err("not 2"),
        });

        assert_eq!(
            res,
            Err(Error {
                error: "no retry",
                tries: 1,
                total_delay: Duration::from_millis(0)
            })
        );
    }

    #[test]
    fn succeeds_with_fixed_delay() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(Fixed::from_millis(1), || match collection.next() {
            Some(n) if n == 2 => Ok(n),
            Some(_) => Err("not 2"),
            None => Err("not 2"),
        })
        .unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn fixed_delay_from_duration() {
        assert_eq!(
            Fixed::from_millis(1_000).next(),
            Fixed::from(Duration::from_secs(1)).next(),
        );
    }

    #[test]
    fn succeeds_with_exponential_delay() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(Exponential::from_millis(1), || match collection.next() {
            Some(n) if n == 2 => Ok(n),
            Some(_) => Err("not 2"),
            None => Err("not 2"),
        })
        .unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn succeeds_with_exponential_delay_with_factor() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(
            Exponential::from_millis_with_factor(1000, 2.0),
            || match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            },
        )
        .unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    #[cfg(feature = "random")]
    fn succeeds_with_ranged_delay() {
        use super::delay::Range;

        let mut collection = vec![1, 2].into_iter();

        let value = retry(Range::from_millis_exclusive(1, 10), || {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        })
        .unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn succeeds_with_index() {
        let mut collection = vec![1, 2, 3].into_iter();

        let value = retry_with_index(NoDelay, |current_try| match collection.next() {
            Some(n) if n == current_try => Ok(n),
            Some(_) => Err("not current_try"),
            None => Err("not current_try"),
        })
        .unwrap();

        assert_eq!(value, 1);
    }
}
