pub fn main() u8 {
    var arr = [_]u8{ 1, 2, 3, 4, 5, 6 };
    const s: []u8 = &arr;
    for (s, 0..) |*x, i| {
        x.* += @intCast(i);
    }
    const mid: []const u8 = s[1..5];
    var sum: u8 = 0;
    for (mid) |v| {
        sum += v;
    }
    return sum;
}
