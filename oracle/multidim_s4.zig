// +increment: mutate a 2D array in place via pointer captures + index captures.
pub fn main() u8 {
    var grid: [3][3]u8 = undefined;
    var sum: u8 = 0;
    for (&grid, 0..) |*row, i| {
        for (row, 0..) |*cell, j| {
            cell.* = @intCast(i * 3 + j);
            sum += cell.*;
        }
    }
    return sum;
}
