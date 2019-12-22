//! Provides asynchronous retry functionality.
//!
//! # Examples
//!
//! ```rust
//! use retry::delay::{Exponential, jitter};
//! use retry::r#async::retry;
//! use async_std::task::block_on;
//! use async_std::sync::{Arc, Mutex};
//! let num_calls = Arc::new(Mutex::new(0));
//!
//! fn main() {
//!     let delay = Exponential::from_millis(10)
//!         .map(jitter).take(3);
//!
//!     let collection = Arc::new(Mutex::new(vec![1, 2, 3].into_iter()));
//!     let collection = &collection;
//!
//!     let result = block_on(retry(delay, || async move {
//!         match collection.lock().await.next() {
//!             Some(n) if n == 3 => Ok("n is 3!"),
//!             Some(_) => Err("n must be 3!"),
//!             None => Err("n was never 3!"),
//!         }
//!     }));
//!
//!     assert!(result.is_ok());
//! }
//! ```

use crate::Error;
use crate::OperationResult;
use async_std::task;
use std::future::Future;
use std::time::Duration;

/// Retry the given operation asynchronously until it succeeds, or until the given `Duration`
/// iterator ends.
pub async fn retry<I, O, R, E, F, OR>(durations: I, mut operation: O) -> Result<R, Error<E>>
where
    I: IntoIterator<Item = Duration>,
    O: FnMut() -> F,
    F: Future<Output = OR>,
    OR: Into<OperationResult<R, E>>,
{
    let mut durations = durations.into_iter();
    let mut current_try = 1;
    let mut total_delay = Duration::default();

    loop {
        match operation().await.into() {
            OperationResult::Ok(value) => return Ok(value),
            OperationResult::Retry(error) => {
                if let Some(delay) = durations.next() {
                    task::sleep(delay).await;
                    current_try += 1;
                    total_delay += delay;
                } else {
                    return Err(Error::Operation {
                        error,
                        total_delay,
                        tries: current_try,
                    });
                }
            }
            OperationResult::Err(error) => {
                return Err(Error::Operation {
                    error,
                    total_delay,
                    tries: current_try,
                });
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        OperationResult,
        Error,
        r#async::retry,
        delay::{Fixed, NoDelay},
    };
    use std::{
        iter::empty,
        time::Duration
    };
    use async_std::{
        task::block_on,
        sync::{Arc, Mutex}
    };

    #[test]
    fn attempts_just_once() {
        let delay = empty();
        let res = block_on(retry(delay, || async move {
            Err::<(), u64>(42)
        }));

        assert_eq!(
            res,
            Err(Error::Operation {
                error: 42,
                tries: 1,
                total_delay: Duration::from_millis(0)
            })
        );
    }

    #[test]
    fn attempts_until_max_retries_exceeded() {
        let delay = Fixed::from_millis(10).take(2);
        let res = block_on(retry(delay, || async move {
            Err::<(), u64>(42)
        }));

        assert_eq!(
            res,
            Err(Error::Operation {
                error: 42,
                tries: 3,
                total_delay: Duration::from_millis(20)
            })
        );
    }

    #[test]
    fn attempts_until_success() {
        let delay = Fixed::from_millis(10);
        let num_calls = Arc::new(Mutex::new(0));
        let num_calls = &num_calls;
        let res = block_on(retry(delay, {
            || async move {
                let num_calls = num_calls.clone();
                let mut lock = num_calls.lock().await;
                *lock += 1;
                if *lock < 4 {
                    Err::<u64, u64>(42)
                } else {
                    Ok::<u64, u64>(*lock)
                }
        }}));

        assert_eq!(res, Ok(4));
    }

    #[test]
    fn fatal_errors() {

        let res: Result<(), Error<&str>> = block_on(retry(NoDelay.take(2), || async move {
           OperationResult::Err("no retry")
        }));

        assert_eq!(
            res,
            Err(Error::Operation {
                error: "no retry",
                tries: 1,
                total_delay: Duration::from_millis(0)
            })
        );
    }
}
