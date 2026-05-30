pub fn main() u8 {
    const data = [_]u8{ 3, 5, 7, 11 };
    var sum: u8 = 0;
    for (data) |v| {
        sum += v;
    }
    return sum;
}
