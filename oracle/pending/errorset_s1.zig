// Smallest first use: declare an error set, return its union, recover with catch.
const MyError = error{
    Oops,
};

fn mightFail() MyError!u8 {
    return 42;
}

pub fn main() u8 {
    // success path: v = 42 (catch branch unused)
    const v = mightFail() catch 0;
    return v;
}
