const Err = error{TooSmall};

fn check(n: u8) Err!u8 {
    if (n < 10) return Err.TooSmall;
    return n * 2;
}

pub fn main() u8 {
    const r = check(60) catch 1;
    return r; // 120
}
