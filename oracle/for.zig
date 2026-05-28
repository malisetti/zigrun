pub fn main() u8 {
    const data = [_]u8{ 3, 5, 7, 11, 13 };
    var total: u32 = 0;
    for (data) |v| {
        total += v;
    }
    return @intCast(total); // 39
}
