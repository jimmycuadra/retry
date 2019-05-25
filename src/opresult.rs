//! Provides a ternary result for operations.
//!
//! # Examples
//!
//! ```rust
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

/// A result that represents either success, retryable failure, or immediately-returning failure.
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum OperationResult<T, E> {
    /// Contains the success value.
    Ok(T),
    /// Contains the error value if duration is exceeded.
    Retry(E),
    /// Contains an error value to return immediately.
    Err(E),
}

impl<T, E> From<Result<T, E>> for OperationResult<T, E> {
    fn from(item: Result<T, E>) -> Self {
        match item {
            Ok(v) => OperationResult::Ok(v),
            Err(e) => OperationResult::Retry(e),
        }
    }
}
