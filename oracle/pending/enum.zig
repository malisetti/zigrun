const Color = enum(u8) {
    red = 10,
    green = 20,
    blue = 30,
};
pub fn main() u8 {
    const c = Color.green;
    return switch (c) {
        .red => 1,
        .green => @intFromEnum(c) + 5, // 25
        .blue => 3,
    };
}
