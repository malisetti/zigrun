# zigrun — a Zig-subset compiler (to C), built in Rust

A from-scratch **compiler** for a subset of Zig, written in Rust. It lowers Zig
source to C (`src/codegen.rs`), invokes `cc` to produce a native executable, and
runs it. Built oracle-first and gated by an **external oracle**, to prove that
the result *works* — not just that tests are green.

Coverage of Zig is tracked honestly in [FEATURES.md](FEATURES.md) (currently on
the order of ~10–15% of the language surface — this is an evolving project).

## The contract: `zigrun run FILE.zig`

Compiles the program to C, builds it with `cc`, and runs the native binary; the
return value of `pub fn main() u8` becomes the process **exit code**. No `std`,
no I/O — the exit code IS the observable result, which makes the oracle
un-fakeable: a program must actually compile and run to the right answer.

```
zigrun emit-c oracle/add.zig          # print the generated C
zigrun run    oracle/add.zig ; echo $?  # compile (C + cc) + run -> 7
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

## Self-evolving orchestration (only path for compiler work)

Operators and chat agents **do not** edit `zigrun/src` to land features — see
[`evolve/OPERATOR_BOUNDARY.md`](evolve/OPERATOR_BOUNDARY.md). Progress comes from
workers via [`evolve/supervise.sh`](evolve/supervise.sh) + [`evolve/frontier_run.sh`](evolve/frontier_run.sh)
toward a **full Zig compiler**, gated by [`oracle/diff.sh`](oracle/diff.sh) vs real Zig.

Start the loop: `MODE=frontier bash zigrun/evolve/supervise.sh` (see [`evolve/GOAL.md`](evolve/GOAL.md)).

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
