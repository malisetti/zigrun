const Vec = struct {
    x: u8,
    y: u8,
    fn sum(self: Vec) u8 {
        return self.x + self.y;
    }
    fn scale(self: *Vec, k: u8) void {
        self.x *= k;
        self.y *= k;
    }
    fn total(self: *Vec, k: u8) u8 {
        self.scale(k);
        return self.sum();
    }
};

pub fn main() u8 {
    var v = Vec{ .x = 3, .y = 4 };
    return v.total(2);
}
