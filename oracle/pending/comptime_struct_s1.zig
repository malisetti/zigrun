fn Pair(comptime T: type) type {
    return struct {
        a: T,
        b: T,

        fn sum(self: @This()) T {
            return self.a + self.b;
        }
    };
}

pub fn main() u8 {
    const U8Pair = Pair(u8);
    const p = U8Pair{ .a = 30, .b = 12 };
    return p.sum();
}
