const Err = error{ Negative, Overflow };

fn safeDouble(n: i32) Err!i32 {
    if (n < 0) return Err.Negative;
    const d = n * 2;
    if (d > 200) return Err.Overflow;
    return d;
}

fn run() Err!i32 {
    const a = try safeDouble(10); // 20
    const b = try safeDouble(30); // 60
    return a + b; // 80
}

pub fn main() u8 {
    const result = run() catch 1; // 80
    const extra = safeDouble(-1) catch |e| switch (e) {
        Err.Negative => @as(i32, 5),
        Err.Overflow => @as(i32, 10),
    };
    return @intCast(result + extra); // 80 + 5 = 85
}
