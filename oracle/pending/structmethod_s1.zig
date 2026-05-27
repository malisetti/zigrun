const Counter = struct {
    fn value(self: Counter) u8 {
        _ = self;
        return 7;
    }
};

pub fn main() u8 {
    const c = Counter{};
    return c.value();
}
