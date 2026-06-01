const std = @import("std");

pub fn main() u8 {
    const a: u8 = 7;
    const b: i32 = -3;
    std.debug.print("a={} b={} ok={}\n", .{ a, b, b < 0 });
    return 42;
}
