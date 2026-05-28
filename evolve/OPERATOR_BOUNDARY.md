# Operator / planner boundary (mandatory)

**Cursor, Codex, and human operators do NOT implement zigrun.**

All compiler progress must come from the **self-evolving orchestration** only:

1. [`supervise.sh`](supervise.sh) keeps the cursor fleet + [`frontier_run.sh`](frontier_run.sh) alive.
2. [`frontier_run.sh`](frontier_run.sh) drives `nfltr orch frontier-run` (impl → gate → integrate).
3. **Workers** write Rust in `zigrun/src/` on branches `zigrun-<wave_id>`.
4. [`gate_one.sh`](gate_one.sh) + [`land_one.sh`](land_one.sh) merge only when **real Zig** agrees (`oracle/diff.sh`).

## Forbidden for operators / chat agents

- Editing `zigrun/src/*.rs` to land a wave (bypasses workers + gate).
- Running `land_one.sh`, `gate_one.sh`, or flipping `WAVES.md` by hand.
- Manually `git merge` / `git push` worker branches to `main`.
- Dispatching one-off `nfltr orch` tasks outside `frontier_run.sh`.

## Allowed for operators / chat agents

- Fix **orchestration glue** (`evolve/*.sh`, `cmd/nfltr` frontier driver, docs).
- Add **oracle specs** (`oracle/pending/*.zig`) and **WAVES.md** lines (the backlog).
- Start/monitor: `MODE=frontier bash zigrun/evolve/supervise.sh`
- Inspect logs: `out/frontier-run.log`, `out/supervise-frontier.log`, `nfltr orch workers`

## Long-term goal

Grow zigrun toward a **full Zig compiler**, wave by wave, with real Zig as ground truth —
not a one-shot hello-world patch by the operator.
