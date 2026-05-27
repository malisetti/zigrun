const Point = struct {
    x: u8,
    y: u8,
    fn sum(self: Point) u8 {
        return self.x + self.y;
    }
};

pub fn main() u8 {
    const p = Point{ .x = 10, .y = 32 };
    return p.sum(); // 42
}
