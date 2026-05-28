pub fn main() u8 {
    const x: u8 = 15;
    return switch (x) {
        0...9 => 10,
        10...19 => 20,
        else => 1,
    };
}
