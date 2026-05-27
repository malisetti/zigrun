pub fn main() u8 {
    const x: ?u8 = null;
    return x orelse 7;
}
