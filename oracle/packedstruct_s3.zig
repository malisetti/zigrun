const Flags = packed struct {
    a: u4, // low 4 bits
    b: u4, // high 4 bits
};

pub fn main() u8 {
    const f = Flags{ .a = 0x2, .b = 0x3 };
    const byte: u8 = @bitCast(f); // (b << 4) | a = 0x32 => 50
    return byte;
}
