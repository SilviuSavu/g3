/// Calculate the nth Fibonacci number using an iterative approach.
///
/// # Arguments
///
/// * `n` - The index of the Fibonacci number to calculate (0-based)
///
/// # Returns
///
/// The nth Fibonacci number
///
/// # Examples
///
/// ```
/// use playground::fib;
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
        let next = prev + curr;
        prev = curr;
        curr = next;
    }
    
    curr
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
    fn test_fib_ten() {
        assert_eq!(fib(10), 55);
    }
    
    #[test]
    fn test_fib_twenty() {
        assert_eq!(fib(20), 6765);
    }
    
    #[test]
    fn test_fib_large() {
        assert_eq!(fib(50), 12586269025);
    }
}
