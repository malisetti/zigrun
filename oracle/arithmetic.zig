pub fn main() u8 {
    var acc: u32 = 0;
    var i: u32 = 1;
    while (i <= 10) : (i += 1) {
        acc += i * i; // sum of squares 1..10 = 385
    }
    return @intCast(acc % 200); // 185
}
