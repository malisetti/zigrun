pub fn main() u8 {
    const a: bool = true;
    const b: bool = false;
    if (a and !b) { return 1; }
    return 0;
}
