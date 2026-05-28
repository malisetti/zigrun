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
- [x] signedints | oracle/signedints.zig | signed integer types `i8`..`i64` — the differential probe found zigrun ERRORS on i32 where real zig works (e.g. `-5 + 8 = 3`). Extend the type model to signed widths. (landed autonomously vs real zig)
- [x] u64wide | oracle/u64wide.zig | `u64` integer width + `@intCast` — extend the int type model to 64-bit. (landed autonomously vs real zig)
- [x] unaryneg | oracle/unaryneg.zig | unary minus `-x` on signed integers. (landed autonomously vs real zig)
- [x] boollogic | oracle/boollogic.zig | `bool` type + `true`/`false` literals + logical `and`/`or`/`!`. (landed autonomously vs real zig)
- [x] arraysum | oracle/arraysum.zig | fixed-size array summed with a for-loop (self-authored, real-zig-validated) (landed autonomously vs real zig)
- [x] arrayidx | oracle/arrayidx.zig | fixed-size array declaration + indexing a[i] (self-authored, real-zig-validated) (landed autonomously vs real zig)
- [x] atmod | oracle/atmod.zig | the @mod or @rem builtin on two integers (self-authored, real-zig-validated) (landed autonomously vs real zig)
- [x] structfield | oracle/structfield.zig | a struct with integer fields: construct it, read a field, return it (self-authored, real-zig-validated) (landed autonomously vs real zig)
- [x] arrays | oracle/arrays.zig | self-DISCOVERED gap (planner-generated, real zig=36, zigrun diverged) (landed autonomously vs real zig)
- [x] optional | oracle/optional.zig | self-discovered atomic gap (real zig=77) (landed autonomously vs real zig)
- [x] recursion | oracle/recursion.zig | self-discovered atomic gap (real zig=89) (landed autonomously vs real zig)
- [x] arithmetic | oracle/arithmetic.zig | self-discovered atomic gap (real zig=185) (landed autonomously vs real zig)
- [x] enum | oracle/enum.zig | self-discovered atomic gap (real zig=25) (landed autonomously vs real zig)
- [x] enums | oracle/enums.zig | self-discovered atomic gap (real zig=55) (landed autonomously vs real zig)
- [x] taggedunion_s5 | oracle/taggedunion_s5.zig | ladder step for 'taggedunion' (real zig=17) (landed autonomously vs real zig)
- [x] taggedunion_s3 | oracle/taggedunion_s3.zig | ladder step for 'taggedunion' (real zig=50) (landed autonomously vs real zig)
- [x] structmethod_s2 | oracle/structmethod_s2.zig | ladder step for 'structmethod' (real zig=42) (landed autonomously vs real zig)
- [x] errorunion_s5 | oracle/errorunion_s5.zig | ladder step for 'errorunion' (real zig=120) (landed autonomously vs real zig)
- [x] errorset_s2 | oracle/errorset_s2.zig | ladder step for 'errorset' (real zig=60) (landed autonomously vs real zig)
- [x] taggedunion_s1 | oracle/taggedunion_s1.zig | ladder step for 'taggedunion' (real zig=7) (landed autonomously vs real zig)
- [x] switchrange_s1 | oracle/switchrange_s1.zig | ladder step for 'switchrange' (real zig=42) (landed autonomously vs real zig)

## Frontier (pending — each is real Zig that zigrun must learn to match)
- [ ] bitwise | oracle/pending/bitwise.zig | self-discovered atomic gap (real zig=35)
- [ ] for | oracle/pending/for.zig | self-discovered atomic gap (real zig=39)
- [ ] labeledloop | oracle/pending/labeledloop.zig | self-discovered atomic gap (real zig=26)
- [ ] errorunion | oracle/pending/errorunion.zig | self-discovered atomic gap (real zig=42)
- [ ] struct | oracle/pending/struct.zig | self-discovered atomic gap (real zig=42)
- [ ] errors | oracle/pending/errors.zig | self-discovered atomic gap (real zig=120)
- [ ] loops | oracle/pending/loops.zig | self-discovered atomic gap (real zig=26)
- [ ] taggedunion_s4 | oracle/pending/taggedunion_s4.zig | ladder step for 'taggedunion' (real zig=56)
- [ ] taggedunion_s2 | oracle/pending/taggedunion_s2.zig | ladder step for 'taggedunion' (real zig=10)
- [ ] structmethod_s5 | oracle/pending/structmethod_s5.zig | ladder step for 'structmethod' (real zig=14)
- [ ] structmethod_s4 | oracle/pending/structmethod_s4.zig | ladder step for 'structmethod' (real zig=8)
- [ ] structmethod_s3 | oracle/pending/structmethod_s3.zig | ladder step for 'structmethod' (real zig=50)
- [ ] structmethod_s1 | oracle/pending/structmethod_s1.zig | ladder step for 'structmethod' (real zig=7)
- [ ] packedstruct_s5 | oracle/pending/packedstruct_s5.zig | ladder step for 'packedstruct' (real zig=53)
- [ ] packedstruct_s4 | oracle/pending/packedstruct_s4.zig | ladder step for 'packedstruct' (real zig=9)
- [ ] packedstruct_s3 | oracle/pending/packedstruct_s3.zig | ladder step for 'packedstruct' (real zig=50)
- [ ] packedstruct_s2 | oracle/pending/packedstruct_s2.zig | ladder step for 'packedstruct' (real zig=8)
- [ ] packedstruct_s1 | oracle/pending/packedstruct_s1.zig | ladder step for 'packedstruct' (real zig=5)
- [ ] switchrange_s5 | oracle/pending/switchrange_s5.zig | ladder step for 'switchrange' (real zig=46)
- [ ] switchrange_s4 | oracle/pending/switchrange_s4.zig | ladder step for 'switchrange' (real zig=60)
- [ ] switchrange_s3 | oracle/pending/switchrange_s3.zig | ladder step for 'switchrange' (real zig=150)
- [ ] switchrange_s2 | oracle/pending/switchrange_s2.zig | ladder step for 'switchrange' (real zig=20)
- [ ] multidim_s5 | oracle/pending/multidim_s5.zig | ladder step for 'multidim' (real zig=72)
- [ ] multidim_s4 | oracle/pending/multidim_s4.zig | ladder step for 'multidim' (real zig=36)
- [ ] multidim_s3 | oracle/pending/multidim_s3.zig | ladder step for 'multidim' (real zig=100)
- [ ] multidim_s2 | oracle/pending/multidim_s2.zig | ladder step for 'multidim' (real zig=50)
- [ ] multidim_s1 | oracle/pending/multidim_s1.zig | ladder step for 'multidim' (real zig=10)
- [ ] errorunion_s4 | oracle/pending/errorunion_s4.zig | ladder step for 'errorunion' (real zig=99)
- [ ] errorunion_s3 | oracle/pending/errorunion_s3.zig | ladder step for 'errorunion' (real zig=80)
- [ ] errorunion_s2 | oracle/pending/errorunion_s2.zig | ladder step for 'errorunion' (real zig=60)
- [ ] errorunion_s1 | oracle/pending/errorunion_s1.zig | ladder step for 'errorunion' (real zig=50)
- [ ] errorset_s5 | oracle/pending/errorset_s5.zig | ladder step for 'errorset' (real zig=49)
- [ ] errorset_s4 | oracle/pending/errorset_s4.zig | ladder step for 'errorset' (real zig=11)
- [ ] errorset_s3 | oracle/pending/errorset_s3.zig | ladder step for 'errorset' (real zig=70)
- [ ] errorset_s1 | oracle/pending/errorset_s1.zig | ladder step for 'errorset' (real zig=42)
- [ ] slice_s5 | oracle/pending/slice_s5.zig | ladder step for 'slice' (real zig=24)
- [ ] slice_s4 | oracle/pending/slice_s4.zig | ladder step for 'slice' (real zig=100)
- [ ] slice_s3 | oracle/pending/slice_s3.zig | ladder step for 'slice' (real zig=33)
- [ ] slice_s2 | oracle/pending/slice_s2.zig | ladder step for 'slice' (real zig=33)
- [ ] slice_s1 | oracle/pending/slice_s1.zig | ladder step for 'slice' (real zig=4)
- [ ] optional_s5 | oracle/pending/optional_s5.zig | ladder step for 'optional' (real zig=15)
- [ ] optional_s4 | oracle/pending/optional_s4.zig | ladder step for 'optional' (real zig=59)
- [ ] optional_s3 | oracle/pending/optional_s3.zig | ladder step for 'optional' (real zig=35)
- [ ] optional_s2 | oracle/pending/optional_s2.zig | ladder step for 'optional' (real zig=7)
- [ ] optional_s1 | oracle/pending/optional_s1.zig | ladder step for 'optional' (real zig=42)
- [ ] error_unions | oracle/pending/error_unions.zig | self-DISCOVERED gap (planner-generated, real zig=85, zigrun diverged)
- [ ] tagged_unions | oracle/pending/tagged_unions.zig | self-DISCOVERED gap (planner-generated, real zig=55, zigrun diverged)
- [ ] switch_ranges | oracle/pending/switch_ranges.zig | self-DISCOVERED gap (planner-generated, real zig=71, zigrun diverged)
- [ ] optionals | oracle/pending/optionals.zig | self-DISCOVERED gap (planner-generated, real zig=105, zigrun diverged)
- [ ] comptime | oracle/pending/comptime.zig | self-DISCOVERED gap (planner-generated, real zig=110, zigrun diverged)
- [ ] structs | oracle/pending/structs.zig | self-DISCOVERED gap (planner-generated, real zig=14, zigrun diverged)
- [ ] packed_struct | oracle/pending/packed_struct.zig | self-DISCOVERED gap (planner-generated, real zig=111, zigrun diverged)
- [ ] multidim_arrays | oracle/pending/multidim_arrays.zig | self-DISCOVERED gap (planner-generated, real zig=45, zigrun diverged)
- [ ] error_union | oracle/pending/error_union.zig | self-DISCOVERED gap (planner-generated, real zig=105, zigrun diverged)
- [ ] print | (spec deferred) | NEEDS ORACLE WORK FIRST: `std.debug.print` writes to STDERR but diff.sh compares stdout — gate must observe stderr (or use std.io stdout writer) before this wave is false-green-safe.

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
