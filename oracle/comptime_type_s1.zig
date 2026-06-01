fn twice(comptime T: type, x: T) T {
    return x + x;
}

pub fn main() u8 {
    const a: u8 = twice(u8, 21);
    const b: u16 = twice(u16, 20);
    return @intCast(a + b); // 42 + 40 = 82
}
