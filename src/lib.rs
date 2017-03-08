//! Crate `retry` provides utilities for retrying operations that can fail.
//!
//! # Usage
//!
//! Retry an operation by passing a closure or function that returns a `Result` to `Retry::run`. The
//! closure or function will be run repeatedly until it returns `Ok`, or until a configured limit is
//! reached, possibly pausing for a configured delay between tries.
//!
//! A `Retry` value can be configured in a few different ways:
//!
//! * The delay before each retry is determined by the argument to `Retry::new`. Any type that
//! implements `DetermineDelay` is valid. The crate includes built-in types for fixed delay,
//! random delay within a range, exponential back-off, and no delay.
//! * A maximum number of retries can be set. If the last retry is reached without success,
//! an `Error` will be returned.
//! * A maximum amount of delay per try can be set. If a retry is reached where the delay would be
//! higher than the maximum allowed, an `Error` will be returned.
//!
//! # Example
//!
//! Imagine an HTTP API with an endpoint that returns 204 No Content while a job is processing, and
//! eventually 200 OK when the job has completed. Retrying until the job is finished would be
//! written:
//!
//! ```
//! # use retry::Retry;
//! # use retry::delay::Exponential;
//! # struct Client;
//! # impl Client {
//! #     fn query_job_status(&self) -> Result<Response, ()> {
//! #         Ok(Response {
//! #             code: 200,
//! #             body: "success",
//! #         })
//! #     }
//! # }
//! # struct Response {
//! #     code: u16,
//! #     body: &'static str,
//! # }
//! # let api_client = Client;
//! let retry = Retry::new(Exponential::from_millis(1000)).maximum_retries(19);
//! let result = retry.run(|| {
//!     match api_client.query_job_status() {
//!         Ok(response) => {
//!             if response.code == 200 {
//!                 Ok(response)
//!             } else {
//!                 Err("API returned non-200")
//!             }
//!         }
//!         Err(error) => Err("API returned an error"),
//!     }
//! });
//! ```
//!
//! This code will run the closure up to 20 times, returning the first successful response, or the
//! final error if the 20th try is reached without success. After each try, the base delay of one
//! second will be increased exponentially.

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]

extern crate rand;

use std::error::Error as StdError;
use std::fmt::Error as FmtError;
use std::fmt::{Display,Formatter};
use std::thread::sleep;
use std::time::Duration;

pub mod delay;

/// Determines the amount of time to wait before each retry.
pub trait DetermineDelay {
    /// Returns the amount of time to wait before the next retry.
    fn next(&mut self) -> Duration;
}

/// Builder object for a retryable operation.
#[derive(Debug)]
pub struct Retry<D> where D: DetermineDelay {
    delay: D,
    jitter: bool,
    maximum_delay_per_try: Option<Duration>,
    maximum_retries: Option<u64>,
}

impl<D> Retry<D> where D: DetermineDelay {
    /// Creates a new retryable operation using the given approach for determining the delay after
    /// each try.
    pub fn new(delay: D) -> Self {
        Retry {
            delay: delay,
            jitter: false,
            maximum_delay_per_try: None,
            maximum_retries: None,
        }
    }

    /// Controls whether or not the delay after each try will be modified by a small random amount.
    pub fn jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;

        self
    }

    /// Sets the maximum delay allowed before a single retry.
    pub fn maximum_delay_per_try(mut self, max: Duration) -> Self {
        self.maximum_delay_per_try = Some(max);

        self
    }

    /// Sets the maximum number of times the operation will be retried before failing.
    pub fn maximum_retries(mut self, maximum_retries: u64) -> Self {
        self.maximum_retries = Some(maximum_retries);

        self
    }

    /// Runs the retryable operation.
    pub fn run<O, R, E>(mut self, mut operation: O) -> Result<R, Error<E>>
    where O: FnMut() -> Result<R, E> {
        let mut try = 1;
        let mut total_delay = Duration::default();

        loop {
            match operation() {
                Ok(value) => return Ok(value),
                Err(error) => {
                    let delay = self.delay.next();

                    if self.reached_maximum_delay_per_try(delay) || self.reached_maximum_retries(try) {
                        return Err(Error::Operation {
                            error: error,
                            total_delay: total_delay,
                            tries: try,
                        });
                    }

                    sleep(delay);

                    try += 1;
                    total_delay += delay;
                }
            }
        }
    }

    /// Placeholder for the asynchronous version of `run`.
    pub fn run_async(self) {}

    fn reached_maximum_delay_per_try(&self, delay: Duration) -> bool {
        if let Some(max) = self.maximum_delay_per_try {
            delay > max
        } else {
            false
        }
    }

    fn reached_maximum_retries(&self, try: u64) -> bool {
        if let Some(max) = self.maximum_retries {
            max < try
        } else {
            false
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

    use super::{Error, Retry};
    use super::delay::{Exponential, Fixed, NoDelay, Range};

    #[test]
    fn succeeds_with_infinite_retries() {
        let mut collection = vec![1, 2, 3, 4, 5].into_iter();

        let value = Retry::new(NoDelay).run(|| {
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

        let value = Retry::new(NoDelay).maximum_retries(1).run(|| {
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
            Retry::new(NoDelay).maximum_retries(1).run(|| {
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

        let value = Retry::new(Fixed::from_millis(1)).run(|| {
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

        let value = Retry::new(Exponential::from_millis(1)).run(|| {
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

        let value = Retry::new(Range::from_millis(1, 10)).run(|| {
            match collection.next() {
                Some(n) if n == 2 => Ok(n),
                Some(_) => Err("not 2"),
                None => Err("not 2"),
            }
        }).unwrap();

        assert_eq!(value, 2);
    }
}
