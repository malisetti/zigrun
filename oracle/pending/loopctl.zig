pub fn main() u8 {
    var i: u8 = 0;
    var s: u8 = 0;
    while (i < 100) {
        i = i + 1;
        if (i == 3) {
            continue;
        }
        if (i > 6) {
            break;
        }
        s = s + i;
    }
    return s;
}
