fn IntFor(comptime wide: bool) type {
    return if (wide) u16 else u8;
}

pub fn main() u8 {
    const Small = IntFor(false);
    const Wide = IntFor(true);
    const a: Small = 37;
    const b: Wide = 5;
    return @intCast(a + b); // 42
}
