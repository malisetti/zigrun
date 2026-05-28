const Value = union(enum) {
    num: u8,
    flag: bool,
};

pub fn main() u8 {
    const v = Value{ .num = 50 };
    switch (v) {
        .num => |n| return n,
        .flag => |b| return if (b) 1 else 2,
    }
}
