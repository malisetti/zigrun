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

## Frontier (pending — each is real Zig that zigrun must learn to match)
- [ ] forloop | oracle/pending/forloop.zig | Implement Zig `for (a..b) |cap| { }` range loops (capture may be `_`). Lower to a C for-loop in codegen. Gate: `oracle/diff.sh` on forloop must match real zig (=7).
- [ ] switch | oracle/pending/switch.zig | Implement `switch (x) { 0 => ..., else => ... }` over integers; lower to C switch or if-chain.
- [ ] inttypes | oracle/pending/inttypes.zig | RE-FOUNDATION: track real types through the pipeline so `u16`/`u32`/`i32` work (not u8-only). Likely needs a value/type model + `@intCast`/`@as`. Operator-architectural — expect a big wave.
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
