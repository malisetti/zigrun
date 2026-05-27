pub fn main() u8 {
    const arr = [_]u8{ 10, 20, 30, 40 };
    const s: []const u8 = &arr;
    var sum: u8 = 0;
    for (s) |x| {
        sum += x;
    }
    return sum;
}
