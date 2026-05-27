// Smallest first use: declare a 2D array, read one element.
pub fn main() u8 {
    const grid = [2][2]u8{ [2]u8{ 10, 20 }, [2]u8{ 30, 40 } };
    return grid[0][0];
}
