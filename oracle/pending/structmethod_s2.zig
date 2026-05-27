const Point = struct {
    x: u8,
    fn getX(self: Point) u8 {
        return self.x;
    }
};

pub fn main() u8 {
    const p = Point{ .x = 42 };
    return p.getX();
}
