pub fn main() u8 {
    const x: u64 = 100000;
    return @intCast(x % 251);
}
