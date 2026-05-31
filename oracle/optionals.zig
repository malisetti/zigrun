fn find(haystack: []const u8, needle: u8) ?usize {
    for (haystack, 0..) |c, i| {
        if (c == needle) return i;
    }
    return null;
}

pub fn main() u8 {
    const data = "hello world";
    const idx = find(data, 'w') orelse 0; // 6
    const idx2 = find(data, 'z') orelse 99; // not found -> 99
    return @intCast(idx + idx2); // 105
}
