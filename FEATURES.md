# zigrun feature coverage vs Zig

zigrun compiles a **subset** of Zig to C. This scorecard tracks coverage
*honestly* so "% of Zig" is measurable, not asserted. `✅` done · `⚠️` partial ·
`❌` not yet.

## Pipeline
- ✅ Lexer · ✅ recursive-descent parser · ✅ C backend (emit C → `cc` → native binary)
- ❌ direct machine-code / LLVM backend · ❌ own object emission / linking

## Types
- ✅ `u8`
- ❌ other integer widths (`u16`/`u32`/`u64`/`usize`, `i8`…`i64`)
- ❌ `bool` (currently modeled as `u8`) · ❌ `f32`/`f64` · ❌ `void`/`noreturn`
- ❌ arrays · ❌ slices · ❌ pointers · ❌ optionals `?T` · ❌ error unions `!T`
- ❌ `struct` · ❌ `enum` · ❌ `union` · ❌ tagged unions
- ⚠️ `comptime_int` (integer literals only)

## Functions
- ✅ declaration · ✅ params · ✅ `return` · ✅ recursion · ✅ multiple functions
- ❌ generics (`anytype` / `comptime` params) · ❌ varargs · ❌ function pointers
- ⚠️ `pub` parsed but ignored (no real export/visibility semantics)

## Control flow
- ✅ `if`/`else` · ✅ `while`
- ❌ `for` · ❌ `switch` · ❌ `break`/`continue` · ❌ `defer`/`errdefer`
- ❌ labeled blocks/loops · ❌ `orelse`/`catch` · ❌ `unreachable`

## Operators
- ✅ `+ - * / %` · ✅ `< > <= >= == !=`
- ✅ bitwise `& | ^ << >>` (binary) · ❌ unary `~` · ❌ logical `and`/`or` (short-circuit) · ❌ unary `-`/`!`
- ❌ wrapping/saturating (`+%`, `+|`) · ❌ `|abs|`

## Variables & semantics
- ✅ `const`/`var` declaration · ✅ assignment
- ⚠️ type annotations are parsed but IGNORED (everything is `u8`)
- ❌ mutability enforcement (`const` reassignment not rejected) · ❌ shadowing rules
- ⚠️ `u8` arithmetic WRAPS (C `uint8_t`) vs Zig's checked semantics — a divergence
- ❌ comptime evaluation · ❌ `@builtins` (`@import`/`@as`/`@intCast`/…)
- ❌ std library · ❌ I/O / `print` · ❌ error handling · ❌ allocators/memory · ❌ async

## Honest coverage

Roughly **~13 of ~80** tracked feature items are implemented — on the order of
**10–15% of Zig's language-feature surface**, and far less of the real compiler's
machinery (no comptime, no std, no backend beyond C). **This is NOT 50%.**
Reaching meaningful coverage is the ongoing evolving work below — the scorecard
is the truth, updated as each wave lands.

## The evolving loop (how coverage grows)

One oracle-gated wave at a time:

1. Add oracle program(s) that need a new feature → they go **RED**.
2. Implement it (lexer → parser → codegen).
3. The `compile-to-C + cc + run` gate goes **GREEN** (the feature actually runs).
4. Merge; update this scorecard.

The oracle is the spec; green means the feature really compiles and runs. Planned
high-value waves, roughly in order:

- integer types (`u16`/`u32`/`i32`) with real type tracking through the pipeline
- `bool` as a distinct type
- bitwise / logical / unary operators
- `for` and `switch`
- `struct` (fields, construction, field access)
- a minimal `@import("std")` + `print` so output is observable beyond exit codes
- enums, optionals, error unions
