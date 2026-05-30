// +increment: a MULTI-error set, switched on the captured error value.
const MathError = error{
    Overflow,
    Negative,
};

fn classify(x: i32) MathError!u8 {
    if (x < 0) return MathError.Negative;
    if (x > 255) return MathError.Overflow;
    return @intCast(x);
}

pub fn main() u8 {
    const v = classify(-5) catch |err| switch (err) {
        error.Negative => 11,
        error.Overflow => 22,
    };
    return v; // -5 -> Negative -> 11
}
