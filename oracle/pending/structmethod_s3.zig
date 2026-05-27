const Point = struct {
    x: u8,
    fn addX(self: Point, n: u8) u8 {
        return self.x + n;
    }
};

pub fn main() u8 {
    const p = Point{ .x = 40 };
    return p.addX(10);
}
