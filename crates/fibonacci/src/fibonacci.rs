/// Computes the nth Fibonacci number.
///
/// Fibonacci sequence: 0, 1, 1, 2, 3, 5, 8, 13, 21, ...
/// Uses an efficient iterative algorithm with O(n) time and O(1) space.
///
/// # Arguments
///
/// * `n` - The index (0-based) of the Fibonacci number to compute
///
/// # Returns
///
/// The nth Fibonacci number
///
/// # Examples
///
/// ```
/// use fibonacci_rs::fibonacci::fibonacci;
///
/// assert_eq!(fibonacci(0), 0);
/// assert_eq!(fibonacci(1), 1);
/// assert_eq!(fibonacci(2), 1);
/// assert_eq!(fibonacci(10), 55);
/// ```
pub fn fibonacci(n: u64) -> u64 {
    if n < 2 {
        return n;
    }

    let mut a: u64 = 0;
    let mut b: u64 = 1;

    for _ in 2..=n {
        let next = a + b;
        a = b;
        b = next;
    }

    b
}

/// Computes the nth Fibonacci number using memoization.
///
/// This version trades memory for speed by caching previously computed values.
/// Useful when computing multiple Fibonacci numbers in a single run.
///
/// # Arguments
///
/// * `n` - The index (0-based) of the Fibonacci number to compute
///
/// # Returns
///
/// The nth Fibonacci number
pub fn fibonacci_memo(n: u64) -> u64 {
    fn helper(n: u64, cache: &mut [Option<u64>]) -> u64 {
        if n < 2 {
            return n;
        }

        if let Some(val) = cache[n as usize] {
            return val;
        }

        let result = helper(n - 1, cache) + helper(n - 2, cache);
        cache[n as usize] = Some(result);
        result
    }

    if n < 2 {
        return n;
    }

    let mut cache: Vec<Option<u64>> = vec![None; (n + 1) as usize];
    helper(n, &mut cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fibonacci_base_cases() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
    }

    #[test]
    fn test_fibonacci_sequence() {
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
    fn test_fibonacci_larger_values() {
        assert_eq!(fibonacci(20), 6765);
        assert_eq!(fibonacci(30), 832040);
        assert_eq!(fibonacci(40), 102334155);
    }

    #[test]
    fn test_fibonacci_memo_same_results() {
        for n in 0..30 {
            assert_eq!(fibonacci(n), fibonacci_memo(n), "mismatch at n={n}");
        }
    }
}
