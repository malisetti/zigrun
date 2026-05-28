const Shape = union(enum) {
    point: void,
    square: u8,
    rect: struct { w: u8, h: u8 },
};

fn area(s: Shape) u8 {
    return switch (s) {
        .point => 1,
        .square => |side| side * side,
        .rect => |r| r.w * r.h,
    };
}

pub fn main() u8 {
    const a = area(.{ .square = 6 });
    const b = area(.{ .rect = .{ .w = 4, .h = 5 } });
    return a + b;
}
