pub fn main() u8 {
    const a: i32 = -5;
    const b: i32 = a + 8;
    return @intCast(b);
}
