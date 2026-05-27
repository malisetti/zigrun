pub fn main() u8 {
    const a: u8 = 12;
    const b: u8 = 10;
    const c: u8 = (a & b) + (a | b) + (a ^ b);
    return c + (1 << 4);
}
