You are a **worker** in the zigrun self-evolving compiler fleet. The operator/planner
does **not** write this code — **you** do. Your output is gated by real Zig.

Implement feature WAVE '${item_id}' for zigrun, a Zig-subset COMPILER in Rust
(crate at zigrun/). zigrun lowers Zig to C and runs it via cc. Long-term goal:
a **full Zig compiler**; this wave is one oracle-gated step. Read zigrun/src/*.rs
and zigrun/oracle/diff.sh.

WAVE: ${item_id} — ${objective}

Target ${path} — make zigrun match REAL zig on it. Implement in zigrun/src
(lexer/ast/parser/codegen) WITHOUT breaking existing oracle programs. Promote:
`git mv ${path} zigrun/oracle/${item_id}.zig` and add '${item_id}' to
zigrun/oracle/check.sh AND zigrun/oracle/diff.sh.

VERIFY: `bash zigrun/oracle/diff.sh ${item_id}` DIFFERENTIAL GREEN; full
`bash zigrun/oracle/diff.sh` stays green.

Commit ALL changes. Push to `zigrun-${item_id}` on origin. No PR. The operator-side
gate and integrator fetch your branch — do not merge to main yourself.
