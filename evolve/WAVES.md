# zigrun evolution backlog — the long-running, resumable plan

Each line is one feature wave: `- [status] <id> | <oracle program> | <objective>`.
`[x]` = landed (in the suite), `[ ]` = pending (the frontier). `evolve.sh`
advances the frontier one wave at a time, gated by the differential oracle
(real `zig` is ground truth). This file + git history + the ledger are the
durable state — stop and resume any time.

## Landed (differentially green vs real zig)
- [x] add | oracle/add.zig | int literals, `+`, return→exit code
- [x] vars | oracle/vars.zig | typed const/var, assignment
- [x] ifelse | oracle/ifelse.zig | if/else, comparison
- [x] while | oracle/while.zig | while loops, mutation
- [x] fn | oracle/fn.zig | functions + params, call
- [x] fib | oracle/fib.zig | recursion (integration)
- [x] bitops | oracle/bitops.zig | bitwise & | ^ << >>
- [x] forloop | oracle/forloop.zig | Zig range for-loops `for (a..b) |cap| {}` (landed via cursor agent, commit 0484779a; differentially green vs real zig = 7)
- [x] switch | oracle/switch.zig | Zig integer switch expressions (via fleet worker patch, verified vs real zig = 30)
- [x] elseif | oracle/elseif.zig | else-if chains `else if (cond) {}` — LANDED AUTONOMOUSLY by evolve/land_wave.py (fleet impl + recover + real-zig gate + merge + push, no operator in the loop), verified vs real zig = 30
- [x] loopctl | oracle/loopctl.zig | `break` and `continue` in loops: add Break/Continue statements (lexer keywords + parser + codegen lower to C break/continue). Gate: diff.sh loopctl == real zig (18). (landed autonomously vs real zig)
- [x] inttypes | oracle/inttypes.zig | RE-FOUNDATION: track real types through the pipeline so `u16`/`u32`/`i32` work (not u8-only). Likely needs a value/type model + `@intCast`/`@as`. Operator-architectural — expect a big wave. (landed autonomously vs real zig)

## Frontier (pending — each is real Zig that zigrun must learn to match)
- [ ] print | oracle/pending/print.zig | Minimal `@import("std")` + `std.debug.print` for integers so STDOUT is observable; the differential gate then compares stdout vs real zig, not just exit codes.

## How a wave lands
1. `evolve.sh` picks the next `[ ]`, ensures zig, runs the differential gate → RED.
2. Dispatch the objective to a worker (inner LLM writes the compiler code) OR implement directly.
3. Recover the work; run `oracle/diff.sh <id>` — GREEN iff zigrun matches real zig.
4. Promote: move the program into the suite, flip `[ ]`→`[x]`, update FEATURES.md, record the ledger.

## The honest ceiling
This loop reliably drives the *incremental* waves (more operators, control flow,
structs, monomorphized generics). It STALLS at the phase-transition core —
`comptime` (a compile-time Zig interpreter), the full type system, real codegen
semantics, `std`. Those are not appendable green steps; they need operator
architectural steering. The engine carries the reachable ~60-70%; the hard core
needs a driver.
