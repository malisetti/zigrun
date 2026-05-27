const Value = union(enum) {
    num: u8,
    flag: bool,
};

pub fn main() u8 {
    const v = Value{ .num = 7 };
    return v.num;
}
