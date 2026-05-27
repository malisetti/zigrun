pub fn main() u8 {
    var found: u8 = 0;
    outer: for (1..10) |i| {
        for (1..10) |j| {
            if (i * j == 12) {
                found = @intCast(i * 10 + j);
                break :outer;
            }
        }
    }
    return found; // i=2,j=6 -> 26
}
