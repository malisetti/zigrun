// Full use: two error sets MERGED with `||`, `try` propagation through a chain,
// and an exhaustive switch over the merged set's errors across multiple inputs.
const ParseError = error{
    Empty,
    Invalid,
};

const RangeError = error{
    OutOfRange,
};

const FullError = ParseError || RangeError;

fn parse(x: i32) ParseError!u8 {
    if (x == 0) return ParseError.Empty;
    if (x < 0) return ParseError.Invalid;
    return @intCast(@mod(x, 100));
}

fn rangeCheck(x: u8) RangeError!u8 {
    if (x > 50) return RangeError.OutOfRange;
    return x;
}

fn process(x: i32) FullError!u8 {
    const p = try parse(x);      // may raise ParseError
    const r = try rangeCheck(p); // may raise RangeError
    return r;
}

pub fn main() u8 {
    var total: u8 = 0;
    const inputs = [_]i32{ 42, 0, -3, 175 };
    for (inputs) |inp| {
        const v = process(inp) catch |err| switch (err) {
            error.Empty => 1,
            error.Invalid => 2,
            error.OutOfRange => 4,
        };
        total += v;
    }
    return total; // 42 + 1 + 2 + 4 = 49
}
