fn half(n: u8) ?u8 {
    if (n % 2 == 0) return n / 2;
    return null;
}

pub fn main() u8 {
    const a = half(100) orelse 0; // present -> 50
    const b = half(7) orelse 9;   // odd -> null -> 9
    return a + b;                 // 59
}
