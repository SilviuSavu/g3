/// Calculates the nth Fibonacci number using an iterative approach.
///
/// The Fibonacci sequence is defined as:
/// - fib(0) = 0
/// - fib(1) = 1
/// - fib(n) = fib(n-1) + fib(n-2) for n > 1
///
/// # Arguments
///
/// * `n` - The position in the Fibonacci sequence (must be non-negative)
///
/// # Returns
///
/// The nth Fibonacci number
///
/// # Examples
///
/// ```
/// use g3_core::math::fibonacci::fib;
///
/// assert_eq!(fib(0), 0);
/// assert_eq!(fib(1), 1);
/// assert_eq!(fib(10), 55);
/// ```
pub fn fib(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut prev: u64 = 0;
            let mut curr: u64 = 1;
            
            for _ in 2..=n {
                let next = prev.saturating_add(curr);
                prev = curr;
                curr = next;
            }
            
            curr
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fib_zero() {
        assert_eq!(fib(0), 0);
    }

    #[test]
    fn test_fib_one() {
        assert_eq!(fib(1), 1);
    }

    #[test]
    fn test_fib_two() {
        assert_eq!(fib(2), 1);
    }

    #[test]
    fn test_fib_three() {
        assert_eq!(fib(3), 2);
    }

    #[test]
    fn test_fib_ten() {
        assert_eq!(fib(10), 55);
    }

    #[test]
    fn test_fib_twenty() {
        assert_eq!(fib(20), 6765);
    }

    #[test]
    fn test_fib_large() {
        // fib(50) = 12586269025
        assert_eq!(fib(50), 12586269025);
    }

    #[test]
    fn test_fib_sequence() {
        // Test that consecutive values follow the Fibonacci rule
        for i in 2..20 {
            assert_eq!(fib(i), fib(i - 1) + fib(i - 2), "Failed at i={}", i);
        }
    }
}
