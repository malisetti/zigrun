const Op = enum { add, sub, mul };

const Instr = union(Op) {
    add: u8,
    sub: u8,
    mul: u8,
};

fn apply(acc: u8, instr: Instr) u8 {
    return switch (instr) {
        .add => |x| acc + x,
        .sub => |x| acc - x,
        .mul => |x| acc * x,
    };
}

pub fn main() u8 {
    const program = [_]Instr{
        .{ .add = 5 },
        .{ .mul = 4 },
        .{ .sub = 3 },
    };
    var acc: u8 = 0;
    for (program) |instr| {
        acc = apply(acc, instr);
    }
    return acc;
}
