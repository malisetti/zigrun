fn classify(n: u32) u32 {
    return switch (n) {
        0...9 => 1,
        10...99 => 2,
        100...999 => 3,
        else => 4,
    };
}
pub fn main() u8 {
    var result: u32 = 0;
    outer: for (0..10) |i| {
        for (0..10) |j| {
            if (i * j > 50) {
                result = @intCast(i * 10 + j); // i=6,j=9 -> 69
                break :outer;
            }
        }
    }
    const c = classify(result); // 2
    return @intCast(result + c); // 71
}
