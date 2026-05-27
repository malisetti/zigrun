pub fn main() u8 {
    const x: u8 = 2;
    const y: u8 = switch (x) {
        0 => 10,
        1 => 20,
        2 => 30,
        else => 0,
    };
    return y;
}
