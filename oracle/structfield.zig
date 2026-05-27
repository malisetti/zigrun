const Point = struct {
    x: u8,
    y: u8,
};

pub fn main() u8 {
    const p = Point{ .x = 42, .y = 99 };
    return p.x + p.y;
}
