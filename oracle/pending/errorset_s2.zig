// +increment: actually RETURN an error and recover it via catch.
const MyError = error{
    Oops,
};

fn mightFail(fail: bool) MyError!u8 {
    if (fail) return MyError.Oops;
    return 10;
}

pub fn main() u8 {
    const a = mightFail(false) catch 99; // a = 10
    const b = mightFail(true) catch 50;  // b = 50 (error caught)
    return a + b;                        // 60
}
