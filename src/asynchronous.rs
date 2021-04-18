//! Provides asynchronous retry functionality via `futures`.
//!
//! # Examples
//!
//! ```rust
//! # extern crate futures;
//! # extern crate tokio_timer;
//! # extern crate retry;
//! # use futures::future::Future;
//! use tokio_timer::Timer;
//! use retry::delay::{Exponential, jitter};
//! use retry::async::retry;
//!
//! pub fn main() {
//!     let timer = Timer::default();
//!     let delay = Exponential::from_millis(10)
//!         .map(jitter).take(3);
//!
//!     let mut collection = vec![1, 2, 3].into_iter();
//!
//!     let future = retry(timer, delay, || {
//!         match collection.next() {
//!             Some(n) if n == 3 => Ok("n is 3!"),
//!             Some(_) => Err("n must be 3!"),
//!             None => Err("n was never 3!"),
//!         }
//!     });
//!
//!     let result = future.wait();
//!
//!     assert!(result.is_ok());
//! }
//! ```

use std::error::Error as StdError;
use std::fmt::{Debug, Error as FmtError, Formatter};
use std::io::Error as IoError;
use std::time::Duration;

use futures::future::{Either, Flatten, FutureResult};
use futures::{Async, Future, IntoFuture, Poll};
#[cfg(feature = "async_tokio_core")]
use tokio_core::reactor::{Handle as ReactorHandle, Timeout as ReactorTimeout};
#[cfg(feature = "async_tokio_timer")]
use tokio_timer::{Sleep as TimerSleep, Timer, TimerError};

use super::Error;

/// Produce a future that resolves after a given delay.
pub trait Sleep {
    /// The type of error that the future will result in if it fails.
    type Error: StdError;
    /// The future that `sleep` will return.
    type Future: Future<Error = Self::Error>;
    /// Returns a future that will resolve after a given delay.
    fn sleep(&mut self, duration: Duration) -> Self::Future;
}

#[cfg(feature = "async_tokio_timer")]
impl Sleep for Timer {
    type Error = TimerError;
    type Future = TimerSleep;
    fn sleep(&mut self, duration: Duration) -> Self::Future {
        Timer::sleep(self, duration)
    }
}

#[cfg(feature = "async_tokio_core")]
impl Sleep for ReactorHandle {
    type Error = IoError;
    type Future = Flatten<FutureResult<ReactorTimeout, IoError>>;
    fn sleep(&mut self, duration: Duration) -> Self::Future {
        ReactorTimeout::new(duration, self).into_future().flatten()
    }
}

/// Keep track of the state of our future, whether it
/// currently sleeps or executes the operation.
enum RetryState<S, A>
where
    S: Sleep,
    A: IntoFuture,
{
    Running(A::Future),
    Sleeping(S::Future),
}

/// Future that drives multiple attempts at an operation.
pub struct RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: IntoFuture,
    O: FnMut() -> A,
{
    delay: I::IntoIter,
    state: RetryState<S, A>,
    operation: O,
    sleep: S,
    total_delay: Duration,
    tries: u64,
}

impl<S, I, O, A> Debug for RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: IntoFuture,
    O: FnMut() -> A,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(
            f,
            "RetryFuture {{ total_delay: {:?}, tries: {:?} }}",
            self.total_delay, self.tries
        )
    }
}

/// Retry the given operation asynchronously until it succeeds, or until the given Duration iterator ends.
pub fn retry<S, I, O, A>(sleep: S, iterable: I, operation: O) -> RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: IntoFuture,
    O: FnMut() -> A,
{
    RetryFuture::spawn(sleep, iterable, operation)
}

impl<S, I, O, A> RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: IntoFuture,
    O: FnMut() -> A,
{
    fn spawn(sleep: S, iterable: I, mut operation: O) -> RetryFuture<S, I, O, A> {
        RetryFuture {
            delay: iterable.into_iter(),
            state: RetryState::Running(operation().into_future()),
            operation: operation,
            sleep: sleep,
            total_delay: Duration::default(),
            tries: 1,
        }
    }

    fn attempt(&mut self) -> Poll<A::Item, Error<A::Error>> {
        let future = (self.operation)().into_future();
        self.state = RetryState::Running(future);
        return self.poll();
    }

    fn retry(&mut self, err: A::Error) -> Poll<A::Item, Error<A::Error>> {
        match self.delay.next() {
            None => Err(Error::Operation {
                error: err,
                total_delay: self.total_delay,
                tries: self.tries,
            }),
            Some(duration) => {
                self.total_delay += duration;
                self.tries += 1;
                let future = self.sleep.sleep(duration);
                self.state = RetryState::Sleeping(future);
                return self.poll();
            }
        }
    }
}

impl<S, I, O, A> Future for RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: IntoFuture,
    O: FnMut() -> A,
{
    type Item = A::Item;
    type Error = Error<A::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = match self.state {
            RetryState::Running(ref mut future) => Either::A(future.poll()),
            RetryState::Sleeping(ref mut future) => Either::B(future.poll()),
        };

        match result {
            Either::A(poll_result) => match poll_result {
                Ok(async) => Ok(async),
                Err(err) => self.retry(err),
            },
            Either::B(poll_result) => {
                let poll_async =
                    poll_result.map_err(|err| Error::Internal(err.description().to_string()))?;

                match poll_async {
                    Async::NotReady => Ok(Async::NotReady),
                    Async::Ready(_) => self.attempt(),
                }
            }
        }
    }
}

#[test]
fn attempts_just_once() {
    use std::iter::empty;
    let delay = empty();
    let mut num_calls = 0;
    let timer = Timer::default();
    let res = retry(timer, delay, || {
        num_calls += 1;
        Err::<(), u64>(42)
    })
    .wait();

    assert_eq!(num_calls, 1);
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
    use super::delay::Fixed;
    use std::time::Duration;
    let timer = Timer::default();
    let delay = Fixed::from_millis(100).take(2);
    let mut num_calls = 0;
    let res = retry(timer, delay, || {
        num_calls += 1;
        Err::<(), u64>(42)
    })
    .wait();

    assert_eq!(num_calls, 3);
    assert_eq!(
        res,
        Err(Error::Operation {
            error: 42,
            tries: 3,
            total_delay: Duration::from_millis(200)
        })
    );
}

#[test]
fn attempts_until_success() {
    use super::delay::Fixed;
    let timer = Timer::default();
    let delay = Fixed::from_millis(100);
    let mut num_calls = 0;
    let res = retry(timer, delay, || {
        num_calls += 1;
        if num_calls < 4 {
            Err::<(), u64>(42)
        } else {
            Ok::<(), u64>(())
        }
    })
    .wait();

    assert_eq!(res, Ok(()));
    assert_eq!(num_calls, 4);
}
