# zigrun — a real subset of Zig, interpreted, built in Rust

A from-scratch Zig-subset interpreter in Rust, built by nfltr orchestration and
gated by an **external oracle**. This crate exists to prove that orchestration +
a real oracle produces a *working* artifact — not just a green one.

## The contract: `zigrun run FILE.zig`

Evaluates the program; the return value of `pub fn main() u8` becomes the
process **exit code**. No `std`, no I/O — the exit code IS the observable result.
That makes the oracle un-fakeable: a program must actually run to the right
answer.

```
zigrun run oracle/add.zig ; echo $?   # -> 7
```

## The oracle = the definition of "done"

`oracle/<name>.zig` paired with `oracle/<name>.exit` (the expected exit code).
`oracle/check.sh [names...]` builds zigrun, runs each program, and compares real
exit codes. **Green means the programs run correctly**, not "unit tests pass."

| program | exercises | exit |
|---|---|---|
| add | integer literals, `+`, `return` | 7 |
| vars | typed `const`/`var`, assignment, `+` | 15 |
| ifelse | `if`/`else`, comparison `>` | 1 |
| while | `while`, mutation, `<` | 15 |
| fn | function decl + call, `*` | 25 |
| fib | recursion, `-`, `+`, `<` | 55 |

This is the **subset** (the language we are committing to support). It grows one
wave at a time — widen it by adding programs to `oracle/`, never by relaxing the
check.

## The orchestration plan (oracle-gated vertical slices)

Each slice makes more of the oracle green end-to-end (lex → parse → sema →
interpret all at once for its features), gated by `oracle/check.sh`:

1. **add** — minimal lexer + parser (fn decl, return, binary `+`) + tree-walking
   interpreter + CLI; `main() u8` return → exit code.
2. **vars + ifelse** — typed const/var, assignment, comparison, `if`/`else`.
3. **while + fn** — loops, function decls/calls with params.
4. **fib** — recursion; the integration program that composes everything.

A slice is **fulfilled** only when `oracle/check.sh <its programs>` exits 0.
That check is the acceptance gate — "done" means "it runs."
