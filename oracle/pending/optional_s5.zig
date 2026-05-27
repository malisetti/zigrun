var counter: u8 = 0;

fn next() ?u8 {
    if (counter >= 5) return null;
    counter += 1;
    return counter;
}

pub fn main() u8 {
    var sum: u8 = 0;
    while (next()) |v| {
        sum += v;
    }
    return sum; // 1+2+3+4+5 = 15
}
