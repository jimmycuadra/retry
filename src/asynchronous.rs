//! Provides asynchronous retry functionality via `futures`.
//!
//! # Examples
//!
//! ```rust
//! # use futures::future::Future;
//! use tokio::time::delay_for;
//! use retry::delay::{Exponential, jitter};
//! use retry::asynchronous::retry;
//! #[tokio::main]
//! pub async fn main() {
//!     let timer = delay_for;
//!     let delay = Exponential::from_millis(10)
//!         .map(jitter).take(3);
//!
//!     let mut collection = vec![1, 2, 3].into_iter();
//!
//!     let future = retry(timer, delay, move || {
//!         let next = collection.next();
//!         async move {
//!             match next {
//!                 Some(n) if n == 3 => Ok("n is 3!"),
//!                 Some(_) => Err("n must be 3!"),
//!                 None => Err("n was never 3!"),
//!             }
//!         }
//!     });
//!
//!     let result = future.await;
//!
//!     assert!(result.is_ok());
//! }
//! ```

use super::Error;
use futures::TryFuture;
use std::fmt::{Debug, Error as FmtError, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// Produce a future that resolves after a given delay.
pub trait Sleep {
    /// The future that `sleep` will return.
    type Future: Future<Output = ()>;
    /// Returns a future that will resolve after a given delay.
    fn sleep(&mut self, duration: Duration) -> Self::Future;
}

impl<Fn, Fut> Sleep for Fn
where
    Fn: FnMut(Duration) -> Fut,
    Fut: Future<Output = ()>,
{
    type Future = Fut;
    fn sleep(&mut self, duration: Duration) -> Self::Future {
        (self)(duration)
    }
}
/// Keep track of the state of our future, whether it
/// currently sleeps or executes the operation.
enum RetryState<S, A>
where
    S: Sleep,
    A: TryFuture,
{
    Running(A),
    Sleeping(S::Future),
}

/// Future that drives multiple attempts at an operation.
pub struct RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: TryFuture,
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
    A: TryFuture,
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
    A: TryFuture,
    O: FnMut() -> A,
{
    RetryFuture::spawn(sleep, iterable, operation)
}

impl<S, I, O, A> RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: TryFuture,
    O: FnMut() -> A,
{
    fn spawn(sleep: S, iterable: I, mut operation: O) -> RetryFuture<S, I, O, A> {
        RetryFuture {
            delay: iterable.into_iter(),
            state: RetryState::Running(operation()),
            operation,
            sleep,
            total_delay: Duration::default(),
            tries: 1,
        }
    }

    fn attempt(&mut self) {
        let future = (self.operation)();
        self.state = RetryState::Running(future);
    }

    fn retry(&mut self, err: A::Error) -> Result<(), Error<A::Error>> {
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
                Ok(())
            }
        }
    }
}

impl<S, I, O, A> Future for RetryFuture<S, I, O, A>
where
    S: Sleep,
    I: IntoIterator<Item = Duration>,
    A: TryFuture,
    O: FnMut() -> A,
{
    type Output = Result<A::Ok, Error<A::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        unsafe {
            // 1. Making `Pin`ned projection of state
            match &mut Pin::get_unchecked_mut(self.as_mut()).state {
                RetryState::Running(future) => {
                    let future = Pin::new_unchecked(future);
                    let result = future.try_poll(cx);
                    match result {
                        Poll::Ready(Ok(r#async)) => Poll::Ready(Ok(r#async)),
                        Poll::Ready(Err(err)) => {
                            // 2.a Mutating state as if it is unpinned.
                            //     Generally speaking, it is unsafe, but safe in this time.
                            //     As the future in `state` is "done" and will not be used anymore,
                            //     discarding it is safe
                            match Pin::get_unchecked_mut(self.as_mut()).retry(err) {
                                Ok(()) => self.poll(cx),
                                Err(e) => Poll::Ready(Err(e)),
                            }
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
                RetryState::Sleeping(future) => {
                    let future = Pin::new_unchecked(future);
                    let result = future.poll(cx);
                    match result {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(()) => {
                            // 2.b Same as 2.a
                            Pin::get_unchecked_mut(self.as_mut()).attempt();
                            self.poll(cx)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[tokio::test]
    async fn attempts_just_once() {
        use futures::future::err;
        use std::iter::empty;
        use tokio::time::delay_for;
        let delay = empty();
        let mut num_calls = 0;
        let timer = delay_for;
        let res = retry(timer, delay, || {
            num_calls += 1;
            err::<(), u64>(42)
        })
        .await;

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

    #[tokio::test]
    async fn attempts_until_max_retries_exceeded() {
        use crate::delay::Fixed;
        use futures::future::err;
        use std::time::Duration;
        use tokio::time::delay_for;
        let timer = delay_for;
        let delay = Fixed::from_millis(100).take(2);
        let mut num_calls = 0;
        let res = retry(timer, delay, || {
            num_calls += 1;
            err::<(), u64>(42)
        })
        .await;

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

    #[tokio::test]
    async fn attempts_until_success() {
        use crate::delay::Fixed;
        use futures::future::{err, ok};
        use tokio::time::delay_for;
        let timer = delay_for;
        let delay = Fixed::from_millis(100);
        let mut num_calls = 0;
        let res = retry(timer, delay, || {
            num_calls += 1;
            if num_calls < 4 {
                err::<(), u64>(42)
            } else {
                ok::<(), u64>(())
            }
        })
        .await;

        assert_eq!(res, Ok(()));
        assert_eq!(num_calls, 4);
    }
}
