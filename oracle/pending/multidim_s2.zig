// +increment: index multiple cells across both dimensions and combine them.
pub fn main() u8 {
    const grid = [2][2]u8{ [2]u8{ 10, 20 }, [2]u8{ 30, 40 } };
    return grid[0][1] + grid[1][0];
}
