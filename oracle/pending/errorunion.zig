const Err = error{Bad};

fn compute(x: u8) Err!u8 {
    if (x == 0) return Err.Bad;
    return x * 2;
}

pub fn main() u8 {
    const r = compute(21) catch return 1;
    return r; // 42
}
