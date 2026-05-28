const MathError = error{ Overflow, Negative };

fn safeAdd(a: u8, b: u8) MathError!u8 {
    const sum: u16 = @as(u16, a) + @as(u16, b);
    if (sum > 200) return MathError.Overflow;
    return @intCast(sum);
}

fn pipeline(a: u8, b: u8, c: u8) MathError!u8 {
    const s1 = try safeAdd(a, b);
    const s2 = try safeAdd(s1, c);
    return s2;
}

pub fn main() u8 {
    const result = pipeline(40, 50, 30);
    if (result) |val| {
        return val;
    } else |err| switch (err) {
        MathError.Overflow => return 200,
        MathError.Negative => return 1,
    }
}
