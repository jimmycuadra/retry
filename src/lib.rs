//! Crate retry provides functionality for retrying an operation until its return value satisfies a
//! specific condition.
//!
//! # Usage
//!
//! Retry an operation by calling the `retry` function, supplying a maximum number of times to try,
//! the number of milliseconds to wait after each unsuccessful try, a closure that produces a value,
//! and a closure that takes a reference to that value and returns a boolean indicating the success
//! or failure of the operation. The function will return a `Result` containing either the value
//! that satisfied the condition or an error indicating that a satisfactory value could not be
//! produced.
//!
//! You can also construct a retryable operation incrementally using the `Retry` type. `Retry::new`
//! takes the same two closures as mutable references, and returns a `Retry` value. You can then
//! call the `try` and `wait` methods on this value to add a maximum number of tries and a wait
//! time, respectively. Finally, run the `execute` method to produce the `Result`.
//!
//! If a maximum number of tries is not supplied, the operation will be executed infinitely or until
//! success. If a wait time is not supplied, there will be no wait between attempts.
//!
//! # Failures
//!
//! Retrying will fail when:
//!
//! 1. The operation reaches the maximum number of tries without success.
//! 2. A value of 0 is supplied for the maximum number of tries. It must be at least 1.
//!
//! # Examples
//!
//! Imagine an HTTP API with an endpoint that returns 204 No Content while a job is processing, and
//! eventually 200 OK when the job has completed. Retrying until the job is finished would be
//! written:
//!
//! ```
//! # use retry::retry;
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
//! match retry(10, 500, || api_client.query_job_status(), |response| response.code == 200) {
//!     Ok(response) => println!("Job completed with result: {}", response.body),
//!     Err(error) => println!("Job completion could not be verified: {}", error),
//! }
//! ```
//!
//!
//! This retries the API call up to 10 times, waiting 500 milliseconds after each unsuccessful
//! attempt. The same result can be achieved by building a `Retry` object incrementally:
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

extern crate rand;

use std::error::Error;
use std::fmt::{Display,Formatter};
use std::fmt::Error as FmtError;
use std::thread::sleep_ms;

use rand::distributions::{IndependentSample, Range};
use rand::thread_rng;

/// Builder object for a retryable operation.
#[derive(Debug)]
pub struct Retry<'a, F: FnMut() -> R + 'a, G: FnMut(&R) -> bool + 'a, R> {
    condition_fn: &'a mut G,
    tries: Option<u32>,
    value_fn: &'a mut F,
    timeout: Option<u32>,
    wait: Wait,
}

impl<'a, F: FnMut() -> R, G: FnMut(&R) -> bool, R> Retry<'a, F, G, R> {
    /// Builds a new `Retry` object.
    pub fn new(
        value_fn: &'a mut F,
        condition_fn: &'a mut G
    ) -> Retry<'a, F, G, R> where F: FnMut() -> R, G: FnMut(&R) -> bool {
        Retry {
            condition_fn: condition_fn,
            tries: None,
            value_fn: value_fn,
            wait: Wait::None,
            timeout: None,
        }
    }

    /// Begins executing the retryable operation.
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

            match self.wait {
                Wait::Exponential(_multiplier) => {
                    let multiplier = (_multiplier + try) as f64;
                    sleep_ms(multiplier.exp() as u32);
                },
                Wait::Fixed(ms) => sleep_ms(ms),
                Wait::None => {},
                Wait::Range(min, max) => {
                    let range = Range::new(min, max);
                    let mut rng = thread_rng();
                    sleep_ms(range.ind_sample(&mut rng));
                },
            }

            try += 1;
        }
    }

    /// Sets the maximum number of milliseconds retries will be made before failing.
    pub fn timeout(mut self, max: u32) -> Retry<'a, F, G, R> {
        self.timeout = Some(max);

        self
    }

    /// Sets the maximum number of tries to make before failing.
    pub fn try(mut self, tries: u32) -> Retry<'a, F, G, R> {
        self.tries = Some(tries);

        self
    }

    /// Sets the number of milliseconds to wait between tries.
    ///
    /// Mutually exclusive with `wait_between` and `wait_exponentially`.
    pub fn wait(mut self, wait: u32) -> Retry<'a, F, G, R> {
        self.wait = Wait::Fixed(wait);

        self
    }

    /// Sets a range for a randomly chosen number of milliseconds to wait between tries. A new
    /// random value from the range is chosen for each try.
    ///
    /// Mutually exclusive with `wait` and `wait_exponentially`.
    pub fn wait_between(mut self, min: u32, max: u32) -> Retry<'a, F, G, R> {
        self.wait = Wait::Range(min, max);

        self
    }

    /// Sets a multiplier in milliseconds to use in exponential backoff between tries.
    ///
    /// Mutually exclusive with `wait` and `wait_between`.
    pub fn wait_exponentially(mut self, multiplier: u32) -> Retry<'a, F, G, R> {
        self.wait = Wait::Exponential(multiplier);

        self
    }
}

#[derive(Debug)]
enum Wait {
    Exponential(u32),
    Fixed(u32),
    None,
    Range(u32, u32),
}

/// Invokes a function a certain number of times or until a condition is satisfied with a fixed
/// wait after each unsuccessful try.
pub fn retry<F, G, R>(
    tries: u32,
    wait: u32,
    mut value_fn: F,
    mut condition_fn: G
) -> Result<R, RetryError> where F: FnMut() -> R, G: FnMut(&R) -> bool {
    Retry::new(&mut value_fn, &mut condition_fn).try(tries).wait(wait).execute()
}

/// Invokes a function exponential backoff between tries is satisfied with a fixed
/// wait after each unsuccessful try.
pub fn retry_exponentially<F, G, R>(
    tries: u32,
    wait: u32,
    mut value_fn: F,
    mut condition_fn: G
) -> Result<R, RetryError> where F: FnMut() -> R, G: FnMut(&R) -> bool {
    Retry::new(&mut value_fn, &mut condition_fn).try(tries).wait_exponentially(wait).execute()
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
    use super::{Retry, retry, retry_exponentially};

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
        ).wait(1).execute().ok().unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn sets_wait_exponentially() {
        let mut collection = vec![1, 2].into_iter();

        let value = Retry::new(
            &mut || collection.next().unwrap(),
            &mut |value| *value == 2
        ).wait_exponentially(1).execute().ok().unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn retry_function() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(2, 0, || collection.next().unwrap(), |value| *value == 2).ok().unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn retry_exponentially_function() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry_exponentially(2, 0, || collection.next().unwrap(), |value| *value == 2).ok().unwrap();

        assert_eq!(value, 2);
    }
}
