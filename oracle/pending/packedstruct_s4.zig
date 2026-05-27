const RGB = packed struct {
    r: u3, // bits 0..2
    g: u3, // bits 3..5
    b: u2, // bits 6..7
};

pub fn main() u8 {
    const raw: u8 = 0b10_101_010; // = 170
    const c: RGB = @bitCast(raw); // r=2 (010), g=5 (101), b=2 (10)
    return @as(u8, c.r) + @as(u8, c.g) + @as(u8, c.b); // 2 + 5 + 2 => 9
}
