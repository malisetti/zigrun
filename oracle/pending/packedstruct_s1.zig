const Flags = packed struct {
    a: u4,
};

pub fn main() u8 {
    const f = Flags{ .a = 5 };
    return f.a; // u4 widens to u8 => 5
}
