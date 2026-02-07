/// Calculate the nth Fibonacci number
/// 
/// # Arguments
/// * `n` - The position in the Fibonacci sequence (0-indexed)
/// 
/// # Returns
/// The nth Fibonacci number
/// 
/// # Examples
/// ```
/// let result = fib(0);
/// assert_eq!(result, 0);
/// 
/// let result = fib(1);
/// assert_eq!(result, 1);
/// 
/// let result = fib(10);
/// assert_eq!(result, 55);
/// ```
pub fn fib(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a: u64 = 0;
            let mut b: u64 = 1;
            
            for _ in 2..=n {
                let c = a.saturating_add(b);
                a = b;
                b = c;
            }
            
            b
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
    fn test_fib_ten() {
        assert_eq!(fib(10), 55);
    }

    #[test]
    fn test_fib_sequence() {
        let sequence = [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55];
        for (i, &expected) in sequence.iter().enumerate() {
            assert_eq!(fib(i as u64), expected, "Failed at index {}", i);
        }
    }

    #[test]
    fn test_fib_large() {
        // fib(50) = 12586269025
        assert_eq!(fib(50), 12586269025);
    }

    #[test]
    fn test_fib_edge_cases() {
        assert_eq!(fib(2), 1);
        assert_eq!(fib(3), 2);
        assert_eq!(fib(4), 3);
        assert_eq!(fib(5), 5);
        assert_eq!(fib(6), 8);
        assert_eq!(fib(7), 13);
        assert_eq!(fib(8), 21);
        assert_eq!(fib(9), 34);
    }
}
