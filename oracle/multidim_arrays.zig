pub fn main() u8 {
    const grid = [3][3]u8{ .{ 1, 2, 3 }, .{ 4, 5, 6 }, .{ 7, 8, 9 } };
    var sum: u8 = 0;
    for (grid) |row| {
        for (row) |cell| {
            sum += cell;
        }
    }
    return sum; // 45
}
