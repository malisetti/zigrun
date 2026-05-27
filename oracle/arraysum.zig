pub fn main() u8 {
    var a: [5]u8 = .{ 10, 20, 30, 40, 50 };
    a[2] += 5;
    var sum: u8 = 0;
    for (a) |x| {
        sum += x;
    }
    return sum;
}
