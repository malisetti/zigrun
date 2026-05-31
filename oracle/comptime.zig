fn fib(comptime n: u32) u32 {
    if (n < 2) return n;
    return fib(n - 1) + fib(n - 2);
}

pub fn main() u8 {
    const len = comptime fib(10); // 55
    var arr: [len]u8 = undefined;
    var sum: u32 = 0;
    for (&arr, 0..) |*e, i| {
        e.* = @intCast(i % 5);
        sum += e.*;
    }
    return @intCast(sum); // 11 blocks of (0+1+2+3+4) = 110
}
