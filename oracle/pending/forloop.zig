pub fn main() u8 {
    var count: u8 = 0;
    for (0..7) |_| {
        count = count + 1;
    }
    return count;
}
