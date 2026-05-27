// +increment: `try` propagates errors up a call chain; main catches at the top.
const MyError = error{
    TooBig,
};

fn check(x: u8) MyError!u8 {
    if (x > 100) return MyError.TooBig;
    return x;
}

fn compute() MyError!u8 {
    const a = try check(40); // 40
    const b = try check(30); // 30
    return a + b;            // 70
}

pub fn main() u8 {
    return compute() catch 200; // 70
}
