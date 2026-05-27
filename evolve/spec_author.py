#!/usr/bin/env python3
"""Self-DISCOVERING spec generator — perpetuity straight from the oracle.

No human curriculum. The planner LLM generates a batch of diverse valid Zig
programs; each is run through BOTH the real `zig` compiler and `zigrun`, and any
program where they DIVERGE is, by definition, a gap zigrun must close — a
self-discovered wave whose ground truth is real zig's behavior. The orch's
"what's next" therefore comes from the oracle itself: every valid Zig program
zigrun gets wrong is a target. As coverage grows, divergences get rarer/harder,
so the loop pushes toward the frontier and self-stops when it can only surface
gaps it has already failed.

Truth stays EXTERNAL: the LLM only proposes programs; real zig decides what they
do and which are gaps. Exit 0 = discovered + queued a new gap; 1 = none found.
"""
import re, subprocess, sys, tempfile, os
from pathlib import Path

ZIGRUN = Path(__file__).resolve().parent.parent
WAVES = ZIGRUN / "evolve" / "WAVES.md"
ATTEMPTED = ZIGRUN / "evolve" / ".authored_features"


def sh(cmd, timeout=120, cwd=None):
    return subprocess.run(cmd, text=True, capture_output=True, timeout=timeout, cwd=cwd)


def ensure_zig():
    r = sh(["bash", str(ZIGRUN / "oracle" / "ensure_zig.sh")])
    out = (r.stdout or "").strip().splitlines()
    return out[-1] if out else "zig"


def gen_batch():
    """Ask the planner LLM for diverse candidate programs. Returns [(feature, src)]."""
    prompt = (
        "Generate 8 SMALL, DIVERSE, valid Zig 0.15 programs, each exercising a DIFFERENT "
        "language feature — pick a varied spread from: fixed/multi-dim arrays, slices, "
        "structs (incl. nested, methods), enums, tagged unions, optionals `?T`, error unions "
        "`!T` with try/catch, switch on ranges, defer, labeled loops/breaks, packed structs, "
        "bit operations, comptime, vectors, character/byte logic.\n"
        "Each program MUST: compile and run under the real `zig` compiler; have "
        "`pub fn main() u8` returning a DETERMINISTIC value between 1 and 200 (never 0 or 101); "
        "use NO `@import`, no std, no I/O.\n"
        "For EACH program, output exactly a line `FEATURE: <one_word_name>` immediately followed "
        "by the program in a ```zig code fence. Nothing else between them."
    )
    out = sh(["claude", "-p", prompt], timeout=240).stdout or ""
    pairs = []
    for m in re.finditer(r"FEATURE:\s*([A-Za-z0-9_]+)\s*```(?:zig)?\s*\n(.*?)```", out, re.S):
        pairs.append((m.group(1).strip().lower(), m.group(2).strip() + "\n"))
    return pairs


def real_zig_result(zig, src):
    """(accepted, exit_code). accepted=False if real zig rejects the program."""
    f = tempfile.NamedTemporaryFile(suffix=".zig", delete=False, mode="w")
    f.write(src); f.close()
    try:
        r = sh([zig, "run", f.name])
        if "error:" in (r.stderr or "").lower():
            return False, None
        return True, r.returncode
    finally:
        os.unlink(f.name)


def zigrun_result(zigrun_bin, src):
    f = tempfile.NamedTemporaryFile(suffix=".zig", delete=False, mode="w")
    f.write(src); f.close()
    try:
        return sh([zigrun_bin, "run", f.name]).returncode
    finally:
        os.unlink(f.name)


def already_known(feat):
    s = WAVES.read_text()
    return (f"] {feat} |" in s) or (f"/{feat}.zig" in s)


def main():
    zig = ensure_zig()
    sh(["cargo", "build", "--quiet"], cwd=str(ZIGRUN), timeout=300)
    zigrun_bin = str(ZIGRUN / "target" / "debug" / "zigrun")
    done = set(ATTEMPTED.read_text().split()) if ATTEMPTED.exists() else set()

    for batch_try in range(3):              # a few batches if the first yields no new gap
        for feat, src in gen_batch():
            if feat in done or already_known(feat):
                continue
            accepted, ze = real_zig_result(zig, src)
            if not accepted or ze is None or ze == 0 or ze == 101 or ze > 255:
                continue                     # not valid/usable Zig for the gate
            if zigrun_result(zigrun_bin, src) == ze:
                continue                     # zigrun already matches — not a gap
            # DISCOVERED a real divergence — queue it as a self-found wave
            (ATTEMPTED).open("a").write(feat + "\n")
            (ZIGRUN / "oracle" / "pending" / f"{feat}.zig").write_text(src)
            anchor = "## Frontier (pending — each is real Zig that zigrun must learn to match)\n"
            line = (f"- [ ] {feat} | oracle/pending/{feat}.zig | self-DISCOVERED gap "
                    f"(planner-generated, real zig={ze}, zigrun diverged)\n")
            WAVES.write_text(WAVES.read_text().replace(anchor, anchor + line, 1))
            print(f"DISCOVERED gap: {feat} (real zig={ze}) — queued from the oracle", flush=True)
            return 0
        print(f"batch {batch_try + 1}: no new divergence found", flush=True)
    print("discover: no new gap (zigrun matches real zig on everything proposed, or all attempted)", flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
