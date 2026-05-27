const Err = error{Oops};

pub fn main() u8 {
    const x: Err!u8 = 50;
    return x catch 1;
}
