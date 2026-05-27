const Mode = enum(u2) {
    off = 0,
    low = 1,
    high = 2,
    turbo = 3,
};

const Register = packed struct {
    enabled: bool, // bit 0
    mode: Mode, // bits 1..2
    level: u5, // bits 3..7

    fn value(self: Register) u8 {
        return @bitCast(self);
    }
};

pub fn main() u8 {
    var reg = Register{ .enabled = true, .mode = .high, .level = 5 };
    reg.level += 1; // level => 6
    const v = reg.value(); // enabled*1 + mode*2 + level*8 = 1 + 4 + 48 => 53
    if (reg.enabled and reg.mode == .high) {
        return v; // 53
    }
    return 1;
}
