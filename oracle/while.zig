pub fn main() u8 {
    var i: u8 = 0;
    var s: u8 = 0;
    while (i < 5) {
        i = i + 1;
        s = s + i;
    }
    return s;
}
