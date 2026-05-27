pub fn main() u8 {
    const x: ?u8 = 30;
    if (x) |v| {
        return v + 5;
    } else {
        return 1;
    }
}
