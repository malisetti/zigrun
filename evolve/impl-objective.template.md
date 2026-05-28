Implement feature WAVE '${item_id}' for zigrun, a Zig-subset COMPILER in Rust
(crate at zigrun/). zigrun lowers Zig to C and runs it via cc; main()'s return
is the exit code, and stdout is compared too. Work from the CURRENT repo (do
NOT `git reset --hard origin/main`). Read zigrun/src/*.rs and zigrun/oracle/diff.sh.

WAVE: ${item_id} — ${objective}

Target ${path} — make your zigrun match REAL zig on it. Implement across
zigrun/src (lexer/ast/parser/codegen) WITHOUT breaking any existing oracle
program. Promote: `git mv ${path} zigrun/oracle/${item_id}.zig` and add
'${item_id}' to the default suite in zigrun/oracle/check.sh AND zigrun/oracle/diff.sh.

VERIFY (un-fakeable; runs real zig AND your zigrun): `bash zigrun/oracle/diff.sh
${item_id}` must print DIFFERENTIAL GREEN and `bash zigrun/oracle/diff.sh`
(full suite) stays green. If you cannot make it fully green, commit what
compiles and say what is incomplete.

Commit ALL changes. Push your work to branch `zigrun-${item_id}` on origin —
the downstream gate + integrator slices fetch from that branch. No PR.
