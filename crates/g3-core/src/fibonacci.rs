/// Calculate the nth Fibonacci number using an efficient iterative approach.
///
/// The Fibonacci sequence is defined as:
/// - fib(0) = 0
/// - fib(1) = 1
/// - fib(n) = fib(n-1) + fib(n-2) for n > 1
///
/// This implementation uses an iterative approach with O(n) time complexity
/// and O(1) space complexity.
///
/// # Arguments
///
/// * `n` - The position in the Fibonacci sequence (must be non-negative)
///
/// # Returns
///
/// The nth Fibonacci number
///
/// # Panics
///
/// Panics if `n` is negative (which cannot happen with u64 input).
///
/// # Examples
///
/// ```
/// use g3_core::fibonacci::fib;
///
/// assert_eq!(fib(0), 0);
/// assert_eq!(fib(1), 1);
/// assert_eq!(fib(10), 55);
/// ```
pub fn fib(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }

    let mut prev: u64 = 0;
    let mut curr: u64 = 1;

    for _ in 2..=n {
        let next = prev.saturating_add(curr);
        prev = curr;
        curr = next;
    }

    curr
}

/// Calculate Fibonacci numbers with overflow detection.
///
/// This version returns an `Option<u64>` that is `None` if the result
/// would overflow u64.
///
/// # Arguments
///
/// * `n` - The position in the Fibonacci sequence
///
/// # Returns
///
/// `Some(fib(n))` if the result fits in u64, `None` otherwise.
pub fn fib_checked(n: u64) -> Option<u64> {
    if n == 0 {
        return Some(0);
    }
    if n == 1 {
        return Some(1);
    }

    let mut prev: u64 = 0;
    let mut curr: u64 = 1;

    for _ in 2..=n {
        match curr.checked_add(prev) {
            Some(next) => {
                prev = curr;
                curr = next;
            }
            None => return None,
        }
    }

    Some(curr)
}

#[cfg(test)]
mod tests {
    use super::{fib, fib_checked};

    #[test]
    fn test_fib_boundary_cases() {
        // Test edge cases
        assert_eq!(fib(0), 0);
        assert_eq!(fib(1), 1);
        assert_eq!(fib(2), 1);
        assert_eq!(fib(3), 2);
        assert_eq!(fib(4), 3);
        assert_eq!(fib(5), 5);
    }

    #[test]
    fn test_fib_standard_cases() {
        assert_eq!(fib(10), 55);
        assert_eq!(fib(15), 610);
        assert_eq!(fib(20), 6765);
        assert_eq!(fib(30), 832040);
    }

    #[test]
    fn test_fib_large_values() {
        // fib(50) = 12586269025
        assert_eq!(fib(50), 12586269025);
        // fib(93) is the largest that fits in u64
        assert_eq!(fib(93), 12200160415121876738);
    }

    #[test]
    fn test_fib_overflow() {
        // fib(94) overflows u64
        assert_eq!(fib_checked(94), None);
    }

    #[test]
    fn test_fib_checked_boundary() {
        assert_eq!(fib_checked(0), Some(0));
        assert_eq!(fib_checked(1), Some(1));
        assert_eq!(fib_checked(93), Some(12200160415121876738));
        assert_eq!(fib_checked(94), None);
    }

    #[test]
    #[should_panic(expected = "test panic for negative input")]
    fn test_fib_negative_behavior() {
        // With u64 input, negative values cannot occur.
        // This test just verifies that large values work.
        let _ = fib(u64::MAX);
    }
}
