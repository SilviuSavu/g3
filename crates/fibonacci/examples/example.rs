use fibonacci_rs::fibonacci::fibonacci;

fn main() {
    println!("Fibonacci sequence (first 10 numbers):");
    for i in 0..10 {
        println!("fibonacci({}) = {}", i, fibonacci(i));
    }
    
    println!("\nlarger values:");
    println!("fibonacci(20) = {}", fibonacci(20));
    println!("fibonacci(30) = {}", fibonacci(30));
    println!("fibonacci(40) = {}", fibonacci(40));
}
