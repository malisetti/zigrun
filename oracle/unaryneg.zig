pub fn main() u8 {
    const a: i32 = 10;
    const b: i32 = -a + 13;
    return @intCast(b);
}
