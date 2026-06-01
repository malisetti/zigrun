pub fn main() u8 {
    const len = comptime blk: {
        var total: u8 = 0;
        var i: u8 = 0;
        while (i < 7) : (i += 1) {
            total += i;
        }
        break :blk total;
    };

    var values: [len]u8 = undefined;
    for (&values, 0..) |*v, i| {
        v.* = @intCast(i % 5);
    }
    return @intCast(values.len); // 0+1+2+3+4+5+6 = 21
}
