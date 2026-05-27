pub fn main() u8 {
    const arr = [_]u8{ 5, 6, 7, 8 };
    const s: []const u8 = &arr;
    return @intCast(s.len);
}
