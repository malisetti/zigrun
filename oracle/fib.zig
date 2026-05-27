fn fib(n: u8) u8 {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

pub fn main() u8 {
    return fib(10);
}
