const Shape = union(enum) {
    circle: u32,
    square: u32,
    rect: struct { w: u32, h: u32 },
};

pub fn main() u8 {
    const shapes = [_]Shape{
        .{ .circle = 3 },
        .{ .square = 4 },
        .{ .rect = .{ .w = 5, .h = 6 } },
    };
    var total: u32 = 0;
    for (shapes) |s| {
        total += switch (s) {
            .circle => |r| r * r,
            .square => |side| side * side,
            .rect => |rc| rc.w * rc.h,
        };
    }
    return @intCast(total);
}
