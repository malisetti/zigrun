fn sumArray(comptime N: usize, values: [N]u8) u8 {
    var total: u8 = 0;
    for (values) |v| {
        total += v;
    }
    return total;
}

pub fn main() u8 {
    const values = [_]u8{ 4, 6, 8, 10 };
    return sumArray(values.len, values); // 28
}
