const Err = error{TooBig};

fn checked(x: u8) Err!u8 {
    if (x > 100) return Err.TooBig;
    return x * 2;
}

fn compute() Err!u8 {
    const v = try checked(30);
    return v;
}

pub fn main() u8 {
    const a = checked(20) catch 0;
    const b = checked(200) catch 5;
    const c = compute() catch 0;
    return a + b + c;
}
