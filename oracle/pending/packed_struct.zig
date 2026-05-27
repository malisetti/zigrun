const Flags = packed struct {
    a: u1,
    b: u1,
    c: u1,
    rest: u5,
};

pub fn main() u8 {
    const f = Flags{ .a = 1, .b = 0, .c = 1, .rest = 0 };
    const byte: u8 = @bitCast(f); // bit0 | bit2 = 0b101 = 5
    const shifted = byte << 1; // 10
    const masked = (shifted | 0b1) & 0xFF; // 11
    return masked + 100; // 111
}
