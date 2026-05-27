// Full use: a true 3-dimensional array, written and summed across all axes.
pub fn main() u8 {
    var cube: [2][3][4]u8 = undefined;
    var sum: u8 = 0;
    for (&cube, 0..) |*plane, i| {
        for (plane, 0..) |*row, j| {
            for (row, 0..) |*cell, k| {
                cell.* = @intCast(i + j + k);
                sum +%= cell.*;
            }
        }
    }
    return sum;
}
