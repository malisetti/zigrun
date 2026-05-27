pub fn main() u8 {
    const arr = [_]u8{ 11, 22, 33, 44, 55 };
    const s: []const u8 = &arr;
    const sub: []const u8 = s[1..4];
    return sub[1];
}
