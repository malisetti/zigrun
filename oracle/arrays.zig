pub fn main() u8 {
    var grid: [3][3]u8 = undefined;
    var sum: u8 = 0;
    for (0..3) |i| {
        for (0..3) |j| {
            grid[i][j] = @intCast(i * 3 + j);
            sum += grid[i][j];
        }
    }
    return sum; // 0+1+...+8 = 36
}
