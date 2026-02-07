/// Computes the nth Fibonacci number.
///
/// # Arguments
///
/// * `n` - The index of the Fibonacci number to compute (0-indexed)
///
/// # Returns
///
/// The nth Fibonacci number, or None if n is too large for the return type.
///
/// # Examples
///
/// ```
/// use g3_cli::fibonacci::fib;
///
/// assert_eq!(fib(0), Some(0));
/// assert_eq!(fib(1), Some(1));
/// assert_eq!(fib(10), Some(55));
/// ```
pub fn fib(n: u64) -> Option<u64> {
    if n == 0 {
        return Some(0);
    }
    if n == 1 {
        return Some(1);
    }

    let mut a: u64 = 0;
    let mut b: u64 = 1;

    for _ in 2..=n {
        match a.checked_add(b) {
            Some(c) => {
                a = b;
                b = c;
            }
            None => return None,
        }
    }

    Some(b)
}

#[cfg(test)]
mod tests {
    use super::fib;

    #[test]
    fn test_fib_zero() {
        assert_eq!(fib(0), Some(0));
    }

    #[test]
    fn test_fib_one() {
        assert_eq!(fib(1), Some(1));
    }

    #[test]
    fn test_fib_two() {
        assert_eq!(fib(2), Some(1));
    }

    #[test]
    fn test_fib_ten() {
        assert_eq!(fib(10), Some(55));
    }

    #[test]
    fn test_fib_twenty() {
        assert_eq!(fib(20), Some(6765));
    }

    #[test]
    fn test_fib_thirty() {
        assert_eq!(fib(30), Some(832040));
    }
}
