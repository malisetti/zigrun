pub fn main() u8 {
    const x: u8 = 5;
    return switch (x) {
        0...9 => 42,
        else => 1,
    };
}
