const Err = error{ TooSmall, TooBig };

fn check(n: u8) Err!u8 {
    if (n < 10) return Err.TooSmall;
    if (n > 150) return Err.TooBig;
    return n;
}

pub fn main() u8 {
    const r = check(200);
    return r catch |err| switch (err) {
        Err.TooSmall => 11,
        Err.TooBig => 99,
    };
}
