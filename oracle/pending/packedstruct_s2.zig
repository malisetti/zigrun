const Flags = packed struct {
    a: u4,
    b: u4,
};

pub fn main() u8 {
    const f = Flags{ .a = 3, .b = 5 };
    return @as(u8, f.a) + @as(u8, f.b); // 3 + 5 => 8
}
