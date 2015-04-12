//! Crate retry provides functionality for retrying an operation until its return value satisfies a
//! specific condition.
//!
//! # Usage
//!
//! Create a retryable operation by passing two mutable closure references to `Retry::new`. The
//! first argument, `value_fn`, will be executed to produce a value. The second argument,
//! `condition_fn`, takes the value produced by `value_fn` and returns a boolean indicating whether
//! or not some desired condition is true. Call `execute` on the returned `Retry` object to begin
//! executing the operation, ultimately returning a `Result` containing the final value or an
//! error.
//!
//! By default, the operation will be retried an infinite number of times with no delay between
//! tries. To change either of these values, you may call the following two methods on the `Retry`
//! object:
//!
//! 1. `try`: The maximum number of tries to make.  If the operation reaches this number of tries
//!    without passsing the condition, an error will be returned.
//! 2. `wait` The number of milliseconds to wait after each unsuccessful try.
//!
//! # Failures
//!
//! Executing a `Retry` will fail when:
//!
//! 1. The operation reaches the maximum number of tries without success.
//! 2. A value of 0 is supplied to `try`. It must be at least 1.
//!
//! # Examples
//!
//! Imagine an HTTP API with an endpoint that returns 204 No Content while a job is processing, and
//! eventually 200 OK when the job has completed. Retrying until the job is finished would be
//! written:
//!
//! ```
//! # use retry::Retry;
//! # struct Client;
//! # impl Client {
//! #     fn query_job_status(&self) -> Response {
//! #         Response {
//! #             code: 200,
//! #             body: "success",
//! #         }
//! #     }
//! # }
//! # struct Response {
//! #     code: u16,
//! #     body: &'static str,
//! # }
//! # let api_client = Client;
//! match Retry::new(
//!     &mut || api_client.query_job_status(),
//!     &mut |response| response.code == 200
//! ).try(10).wait(500).execute() {
//!     Ok(response) => println!("Job completed with result: {}", response.body),
//!     Err(error) => println!("Job completion could not be verified: {}", error),
//! }
//! ```
//!
//! This retries the API call up to 10 times, waiting 500 milliseconds after each unsuccessful
//! attempt.

use std::error::Error;
use std::fmt::{Display,Formatter};
use std::fmt::Error as FmtError;
use std::thread::sleep_ms;

/// Builder object for a retryable operation.
#[derive(Debug)]
pub struct Retry<'a, F: FnMut() -> R + 'a, G: FnMut(&R) -> bool + 'a, R> {
    condition_fn: &'a mut G,
    tries: Option<u32>,
    value_fn: &'a mut F,
    wait: u32,
}

impl<'a, F: FnMut() -> R, G: FnMut(&R) -> bool, R> Retry<'a, F, G, R> {
    /// Build a new `Retry` object.
    pub fn new(
        value_fn: &'a mut F,
        condition_fn: &'a mut G
    ) -> Retry<'a, F, G, R> where F: FnMut() -> R, G: FnMut(&R) -> bool {
        Retry {
            condition_fn: condition_fn,
            tries: None,
            value_fn: value_fn,
            wait: 0,
        }
    }

    /// Begin executing the retryable operation.
    pub fn execute(self) -> Result<R, RetryError> {
        if self.tries.is_some() && self.tries.unwrap() == 0 {
            return Err(RetryError { message: "tries must be non-zero" });
        }

        let mut try = 0;

        loop {
            if self.tries.is_some() && self.tries.unwrap() == try {
                return Err(RetryError { message: "reached last try without condition match" })
            }

            let value = (self.value_fn)();

            if (self.condition_fn)(&value) {
                return Ok(value);
            }

            sleep_ms(self.wait);
            try += 1;
        }
    }

    /// Set a maximum number of tries to make before failing.
    pub fn try(mut self, tries: u32) -> Retry<'a, F, G, R> {
        self.tries = Some(tries);

        self
    }

    /// Set the number of milliseconds to wait after an unsuccesful try before trying again.
    pub fn wait(mut self, wait: u32) -> Retry<'a, F, G, R> {
        self.wait = wait;

        self
    }
}

/// An error indicating that a retry call failed.
#[derive(Debug)]
pub struct RetryError {
    message: &'static str
}

impl Display for RetryError {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        write!(formatter, "{}", self.message)
    }
}

impl Error for RetryError {
    fn description(&self) -> &str {
        self.message
    }
}

#[cfg(test)]
mod tests {
    use super::Retry;

    #[test]
    fn succeeds_without_try_count() {
        let mut collection = vec![1, 2, 3, 4, 5].into_iter();

        let value = Retry::new(
            &mut || collection.next().unwrap(),
            &mut |value| *value == 5
        ).execute().unwrap();

        assert_eq!(value, 5);
    }

    #[test]
    fn succeeds_on_first_try() {
        let value = Retry::new(&mut || 1, &mut |value| *value == 1).try(1).execute().ok().unwrap();

        assert_eq!(value, 1);
    }

    #[test]
    fn requires_non_zero_tries() {
        let error = Retry::new(&mut || 1, &mut |value| *value == 1).try(0).execute().err().unwrap();

        assert_eq!(error.message, "tries must be non-zero");
    }

    #[test]
    fn succeeds_on_subsequent_try() {
        let mut collection = vec![1, 2].into_iter();

        let value = Retry::new(
            &mut || collection.next().unwrap(),
            &mut |value| *value == 2
        ).try(2).execute().ok().unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn fails_after_last_try() {
        let mut collection = vec![1].into_iter();

        let error = Retry::new(
            &mut || collection.next().unwrap(),
            &mut |value| *value == 2
        ).try(1).execute().err().unwrap();

        assert_eq!(error.message, "reached last try without condition match");
    }

    #[test]
    fn sets_custom_wait_time() {
        let mut collection = vec![1, 2].into_iter();

        let value = Retry::new(
            &mut || collection.next().unwrap(),
            &mut |value| *value == 2
        ).try(2).wait(1).execute().ok().unwrap();

        assert_eq!(value, 2);
    }
}
