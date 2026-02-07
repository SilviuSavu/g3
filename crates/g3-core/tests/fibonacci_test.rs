/// Fibonacci function implementation with tests.

/// Calculates the nth Fibonacci number.
/// 
/// Uses an iterative approach for efficiency.
/// 
/// # Examples
/// 
/// ```
/// let result = fibonacci(0);
/// assert_eq!(result, 0);
/// 
/// let result = fibonacci(1);
/// assert_eq!(result, 1);
/// 
/// let result = fibonacci(10);
/// assert_eq!(result, 55);
/// ```
/// 
/// # Panics
/// 
/// Panics if n is too large and would cause overflow.
pub fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut prev: u64 = 0;
            let mut curr: u64 = 1;
            
            for _ in 2..=n {
                let next = prev.checked_add(curr)
                    .expect("Fibonacci overflow detected");
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
    fn test_fibonacci_zero() {
        assert_eq!(fibonacci(0), 0);
    }

    #[test]
    fn test_fibonacci_one() {
        assert_eq!(fibonacci(1), 1);
    }

    #[test]
    fn test_fibonacci_small_numbers() {
        assert_eq!(fibonacci(2), 1);
        assert_eq!(fibonacci(3), 2);
        assert_eq!(fibonacci(4), 3);
        assert_eq!(fibonacci(5), 5);
        assert_eq!(fibonacci(6), 8);
        assert_eq!(fibonacci(7), 13);
        assert_eq!(fibonacci(8), 21);
        assert_eq!(fibonacci(9), 34);
        assert_eq!(fibonacci(10), 55);
    }

    #[test]
    fn test_fibonacci_larger_number() {
        assert_eq!(fibonacci(20), 6765);
        assert_eq!(fibonacci(30), 832040);
        assert_eq!(fibonacci(40), 102334155);
    }

    #[test]
    fn test_fibonacci_sequence_property() {
        // Verify the sequence property: F(n) = F(n-1) + F(n-2)
        for n in 2..=40 {
            let expected = fibonacci(n - 1) + fibonacci(n - 2);
            assert_eq!(fibonacci(n), expected, "F({}) should equal F({}) + F({})", n, n - 1, n - 2);
        }
    }

    #[test]
    #[should_panic(expected = "Fibonacci overflow detected")]
    fn test_fibonacci_overflow() {
        // This will overflow for large n (around n > 92 for u64)
        fibonacci(100);
    }
}
