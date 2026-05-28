// +increment: walk the whole 2D array with nested for-loops (row then cell).
pub fn main() u8 {
    const grid = [2][2]u8{ [2]u8{ 10, 20 }, [2]u8{ 30, 40 } };
    var sum: u8 = 0;
    for (grid) |row| {
        for (row) |cell| {
            sum += cell;
        }
    }
    return sum;
}
