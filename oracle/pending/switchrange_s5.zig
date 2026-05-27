pub fn main() u8 {
    var total: u8 = 0;
    var i: u8 = 0;
    while (i < 10) : (i += 1) {
        total += switch (i) {
            0, 1 => 1,
            2...4 => 2,
            5...7 => |v| v,
            8, 9 => 10,
            else => 0,
        };
    }
    return total;
}
