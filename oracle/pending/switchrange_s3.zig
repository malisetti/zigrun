pub fn main() u8 {
    const x: u8 = 100;
    return switch (x) {
        0...9 => 10,
        10...99 => 20,
        100 => 150,
        else => 1,
    };
}
