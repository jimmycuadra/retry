//! Crate retry provides a set of functions for retrying an operation continuously until its
//! return value satisfies a specific condition.

use std::error::Error;
use std::fmt::{Display,Formatter};
use std::fmt::Error as FmtError;
use std::thread::sleep_ms;

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

/// Invokes a function a certain number of times or until a condition is satisfied.
///
/// `value_fn` is a closure that will be executed to produce a value. `condition_fn` is a closure
/// that takes the value produced by `value_fn` and returns a boolean indicating whether or not
/// some desired condition is true. `retry` will invoke `value_fn` up to `tries` times, returning
/// the value as soon as `condition_fn` is satisfied, or returning an error when the last try was
/// unsuccessful. It will wait `wait` milliseconds after each unsuccessful try.
///
/// # Failures
///
/// Will fail when:
///
/// 1. `value_fn` has been been invoked `tries` times and has still not satisfied `condition_fn`.
/// 2. `tries` is zero. It must be at least 1.
///
/// # Examples
///
/// Imagine an HTTP API with an endpoint that returns 204 No Content while a job is processing, and
/// eventually 200 OK when the job has completed. Retrying until the job is finished would be
/// written:
///
/// ```
/// # use retry::retry;
/// # struct Client;
/// # impl Client {
/// #     fn query_job_status(&self) -> Response {
/// #         Response {
/// #             code: 200,
/// #             body: "success",
/// #         }
/// #     }
/// # }
/// # struct Response {
/// #     code: u16,
/// #     body: &'static str,
/// # }
/// # let api_client = Client;
/// match retry(10, 500, || api_client.query_job_status(), |response| response.code == 200) {
///     Ok(response) => println!("Job completed with result: {}", response.body),
///     Err(error) => println!("Job completion could not be verified: {}", error),
/// }
/// ```
///
/// This retries the API call up to 10 times, waiting 500 milliseconds after each unsuccesful
/// attempt.
pub fn retry<F, G, R>(
    tries: u32,
    wait: u32,
    mut value_fn: F,
    mut condition_fn: G
) -> Result<R, RetryError> where F: FnMut() -> R, G: FnMut(&R) -> bool {
    if tries == 0 {
        return Err(RetryError { message: "tries must be non-zero" });
    }

    for _ in 0..tries {
        let value = value_fn();

        if condition_fn(&value) {
            return Ok(value);
        }

        sleep_ms(wait);
    }

    Err(RetryError { message: "reached last try without condition match" })
}


/// Invokes a function infinitely until a condition is satisfied.
///
/// Works the same as `retry`, but will try an infinite number of times. Since it will never return
/// unsuccessfully, its return value is not wrapped in a `Result`.
pub fn infinite_retry<F, G, R>(
    wait: u32,
    mut value_fn: F,
    mut condition_fn: G
) -> R where F: FnMut() -> R, G: FnMut(&R) -> bool {
    loop {
        let value = value_fn();

        if condition_fn(&value) {
            return value;
        }

        sleep_ms(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::{infinite_retry, retry};

    #[test]
    fn succeeds_on_first_try() {
        let value = retry(1, 0, || 1, |value| *value == 1).ok().unwrap();

        assert_eq!(value, 1);
    }

    #[test]
    fn requires_non_zero_tries() {
        let error = retry(0, 0, || 1, |value| *value == 1).err().unwrap();

        assert_eq!(error.message, "tries must be non-zero");
    }

    #[test]
    fn succeeds_on_subsequent_try() {
        let mut collection = vec![1, 2].into_iter();

        let value = retry(2, 0, || collection.next().unwrap(), |value| *value == 2).ok().unwrap();

        assert_eq!(value, 2);
    }

    #[test]
    fn fails_after_last_try() {
        let mut collection = vec![1].into_iter();

        let error = retry(1, 0, || collection.next().unwrap(), |value| *value == 2).err().unwrap();

        assert_eq!(error.message, "reached last try without condition match");
    }

    #[test]
    fn succeeds_without_try_count() {
        let mut collection = vec![1, 2, 3, 4, 5].into_iter();

        let value = infinite_retry(0, || collection.next().unwrap(), |value| *value == 5);

        assert_eq!(value, 5);
    }
}
