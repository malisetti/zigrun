const Counter = struct {
    n: u8,
    fn incr(self: *Counter) void {
        self.n += 1;
    }
};

pub fn main() u8 {
    var c = Counter{ .n = 5 };
    c.incr();
    c.incr();
    c.incr();
    return c.n;
}
