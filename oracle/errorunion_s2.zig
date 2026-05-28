const Err = error{Oops};

fn make(ok: bool) Err!u8 {
    if (ok) return 60;
    return Err.Oops;
}

pub fn main() u8 {
    const x = make(true);
    return x catch 2;
}
