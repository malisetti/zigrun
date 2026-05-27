fn fib(n: u32) u32 {
    if (n < 2) return n;
    return fib(n - 1) + fib(n - 2);
}

pub fn main() u8 {
    return @intCast(fib(11)); // 89
}
