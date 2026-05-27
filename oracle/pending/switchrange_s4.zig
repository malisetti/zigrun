pub fn main() u8 {
    const x: u8 = 50;
    return switch (x) {
        0, 1, 2 => 5,
        3...49 => 30,
        50...99 => |v| v + 10,
        else => 1,
    };
}
