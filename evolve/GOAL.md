# zigrun evolution goal

The frontier loop exists to grow **zigrun** into a **full Zig compiler** (Rust
implementation), verified continuously against the **real Zig compiler** —
not via operator or chat-agent edits to `zigrun/src/`.

See [OPERATOR_BOUNDARY.md](OPERATOR_BOUNDARY.md): **only workers land code.**

## Milestones (ordered)

| Milestone | Gate | Status |
|-----------|------|--------|
| Hello world (`std.debug.print`) | `bash oracle/diff.sh helloworld` | tracked in WAVES.md |
| Broader language surface | each wave in WAVES.md | ongoing |
| `comptime`, full type system, `std` | architectural waves | honest ceiling — needs planner steering |

## How the loop self-evolves

1. [`WAVES.md`](WAVES.md) is the backlog; north-star and ladder waves are listed first.
2. [`frontier_run.sh`](frontier_run.sh) runs `nfltr orch frontier-run` (batch impl → gate → integ).
3. [`supervise.sh`](supervise.sh) keeps six **cursor** workers + integrator up; restarts the driver.
4. [`gate_one.sh`](gate_one.sh) overlays untampered oracle; **real Zig** is truth.
5. [`land_one.sh`](land_one.sh) merges green branches to `main` and flips `[ ]` → `[x]`.

## Start the loop (operator only starts the engine)

```bash
cd /path/to/onpremlink
MODE=frontier SUP_BUDGET=14400 nohup bash zigrun/evolve/supervise.sh \
  >> out/supervise-frontier.log 2>&1 &
```

Do **not** implement waves yourself after starting this.

## Honest ceiling

Incremental waves (operators, control flow, structs, I/O stubs) are automatable.
Phase transitions (`comptime`, complete semantics, full `std`) need backlog curation
in WAVES.md — still executed **by workers through frontier-run**, not by hand-merging
compiler patches in chat.
