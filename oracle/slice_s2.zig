pub fn main() u8 {
    const arr = [_]u8{ 11, 22, 33, 44 };
    const s: []const u8 = &arr;
    return s[2];
}
