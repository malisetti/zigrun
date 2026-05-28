# zigrun evolution goal

The frontier loop exists to **build zigrun** (a Zig-subset compiler in Rust) until it
can run a real **Hello, world!** program verified against the **real Zig compiler**.

## North star oracle

- **Wave id:** `helloworld`
- **Spec:** [`oracle/pending/helloworld.zig`](../oracle/pending/helloworld.zig)
- **Gate:** `bash zigrun/oracle/diff.sh helloworld` (exit + stdout + stderr must match `zig run`)

## How the loop self-evolves

1. [`WAVES.md`](WAVES.md) lists pending waves; **north-star waves are first** so
   `frontier-run` always tries them before the long tail.
2. [`frontier_run.sh`](frontier_run.sh) dispatches impl → [`gate_one.sh`](gate_one.sh) →
   [`land_one.sh`](land_one.sh) on a six-worker **cursor-only** fleet.
3. Failed batches **retry** (no stop on zero lands); stale tasks are **deduped** so
   workers stay on one wave each.
4. Each landed wave merges to `main` and shrinks the frontier; the scorecard in
   [`FEATURES.md`](../FEATURES.md) moves toward I/O and `std`.

## What “done” means

`zigrun run zigrun/oracle/helloworld.zig` prints `Hello, world!\n` on stderr (matching
real Zig), exits 0, and the full differential suite stays green.
