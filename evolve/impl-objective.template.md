You are a **worker** in the zigrun self-evolving compiler fleet. The operator/planner
does **not** write this code — **you** do. Your output is gated by real Zig.

SETUP (do this FIRST, before anything else): your working directory must be a clean
checkout of this repo at origin/main on a fresh branch `zigrun-${item_id}`.
- Use `repo_url="${NFLTR_REPO_URL:-https://github.com/onpremlink/onpremlink.git}"`.
- If `git rev-parse --git-dir` succeeds (the repo is already here):
    `repo_url="${NFLTR_REPO_URL:-https://github.com/onpremlink/onpremlink.git}"; git remote get-url origin >/dev/null 2>&1 && git remote set-url origin "$repo_url" || git remote add origin "$repo_url"; git fetch --prune origin +refs/heads/main:refs/remotes/origin/main && git checkout -B zigrun-${item_id} refs/remotes/origin/main && git reset --hard refs/remotes/origin/main && git clean -fd`
- Otherwise (the directory is empty):
    `repo_url="${NFLTR_REPO_URL:-https://github.com/onpremlink/onpremlink.git}"; git clone --origin origin "$repo_url" . && git fetch --prune origin +refs/heads/main:refs/remotes/origin/main && git checkout -B zigrun-${item_id} refs/remotes/origin/main`
Do NOT proceed until `zigrun/src/*.rs` and `zigrun/oracle/diff.sh` are present in your tree.

Implement feature WAVE '${item_id}' for zigrun, a Zig-subset COMPILER in Rust
(crate at zigrun/). zigrun lowers Zig to C and runs it via cc. Long-term goal:
a **full Zig compiler**; this wave is one oracle-gated step. Read zigrun/src/*.rs
and zigrun/oracle/diff.sh.

WAVE: ${item_id} — ${objective}

Target zigrun/${path} — make zigrun match REAL zig on it. Implement in zigrun/src
(lexer/ast/parser/codegen) WITHOUT breaking existing oracle programs. Promote:
`git mv zigrun/${path} zigrun/oracle/${item_id}.zig` and add '${item_id}' to
zigrun/oracle/check.sh AND zigrun/oracle/diff.sh.

VERIFY: `bash zigrun/oracle/diff.sh ${item_id}` DIFFERENTIAL GREEN; full
`bash zigrun/oracle/diff.sh` stays green.

Commit ALL changes. Push with `git push --force-with-lease origin zigrun-${item_id}`.
No PR. The operator-side gate and integrator fetch your branch — do not merge to
main yourself.
