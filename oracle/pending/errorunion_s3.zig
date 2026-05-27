const Err = error{Oops};

fn make(ok: bool) Err!u8 {
    if (ok) return 70;
    return Err.Oops;
}

fn doubled(ok: bool) Err!u8 {
    const v = try make(ok);
    return v + 10;
}

pub fn main() u8 {
    return doubled(true) catch 3;
}
