const Packet = union(enum) {
    bytes: []const u8,
    code: u8,
};

fn score(p: Packet) u8 {
    return switch (p) {
        .bytes => |b| @intCast(b.len),
        .code => |c| c,
    };
}

pub fn main() u8 {
    const data = [_]u8{ 1, 2, 3, 4 };
    return score(.{ .bytes = data[1..4] });
}
