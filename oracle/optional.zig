pub fn main() u8 {
    const maybe: ?u8 = 77;
    const val = maybe orelse 0;
    return val;
}
