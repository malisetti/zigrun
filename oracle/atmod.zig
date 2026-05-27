pub fn main() u8 {
    const a: u32 = 100;
    const b: u32 = 7;
    const m = @mod(a, b);
    const r = @rem(a, b);
    return @intCast(m + r + 38);
}
