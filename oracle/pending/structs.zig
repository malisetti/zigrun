const Point = struct {
    x: i32,
    y: i32,
    fn manhattan(self: Point) i32 {
        const ax = if (self.x < 0) -self.x else self.x;
        const ay = if (self.y < 0) -self.y else self.y;
        return ax + ay;
    }
};

const Line = struct {
    a: Point,
    b: Point,
    fn span(self: Line) i32 {
        return self.a.manhattan() + self.b.manhattan();
    }
};

pub fn main() u8 {
    const l = Line{ .a = .{ .x = 3, .y = -4 }, .b = .{ .x = 5, .y = 2 } };
    return @intCast(l.span());
}
