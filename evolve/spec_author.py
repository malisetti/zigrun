#!/usr/bin/env python3
"""Self-driving spec source for the orch — DECOMPOSITION-first, then discovery.

The achievable atomic features are done; what remains (error unions, optionals,
tagged unions, slices, …) is too big for one worker patch. So this primarily
DECOMPOSES a hard target into a LADDER of small, individually-landable steps
(step 1 = smallest first use; each step adds one increment), each validated
against REAL ZIG (compiles+runs => ground truth; deterministic 1..200, !=101;
zigrun currently fails it => a genuine gap). The orch lands the ladder step by
step — each landed step makes the next a smaller gap. When all hard targets are
laddered, it falls back to open-ended atomic discovery.

Truth stays EXTERNAL: the planner only proposes programs; real zig decides.
Exit 0 = queued new wave(s); 1 = nothing new to propose.
"""
import re, subprocess, sys, tempfile, os
from pathlib import Path

ZIGRUN = Path(__file__).resolve().parent.parent
WAVES = ZIGRUN / "evolve" / "WAVES.md"
DONE = ZIGRUN / "evolve" / ".decomposed_targets"
ANCHOR = "## Frontier (pending — each is real Zig that zigrun must learn to match)\n"

# Hard features to decompose, roughly easy -> hard.
TARGETS = ["optional", "slice", "errorset", "errorunion", "multidim",
           "switchrange", "packedstruct", "structmethod", "taggedunion"]


def sh(cmd, timeout=120, cwd=None):
    try:
        return subprocess.run(cmd, text=True, capture_output=True, timeout=timeout, cwd=cwd)
    except subprocess.TimeoutExpired as e:
        return subprocess.CompletedProcess(cmd, 124, (e.stdout or ""), "TIMEOUT")


def ensure_zig():
    r = sh(["bash", str(ZIGRUN / "oracle" / "ensure_zig.sh")])
    out = (r.stdout or "").strip().splitlines()
    return out[-1] if out else "zig"


def parse_programs(text):
    return [(m.group(1).strip().lower(), m.group(2).strip() + "\n")
            for m in re.finditer(r"FEATURE:\s*([A-Za-z0-9_]+)\s*```(?:zig)?\s*\n(.*?)```", text, re.S)]


def real_zig(zig, src):
    f = tempfile.NamedTemporaryFile(suffix=".zig", delete=False, mode="w"); f.write(src); f.close()
    try:
        r = sh([zig, "run", f.name])
        return (None if "error:" in (r.stderr or "").lower() else r.returncode)
    finally:
        os.unlink(f.name)


def zigrun_exit(zb, src):
    f = tempfile.NamedTemporaryFile(suffix=".zig", delete=False, mode="w"); f.write(src); f.close()
    try:
        return sh([zb, "run", f.name]).returncode
    finally:
        os.unlink(f.name)


def is_gap(zig, zb, src):
    ze = real_zig(zig, src)
    if ze is None or ze == 0 or ze == 101 or ze > 255:
        return None
    return ze if zigrun_exit(zb, src) != ze else None


def queue(wave_id, src, ze, note):
    (ZIGRUN / "oracle" / "pending" / f"{wave_id}.zig").write_text(src)
    line = f"- [ ] {wave_id} | oracle/pending/{wave_id}.zig | {note} (real zig={ze})\n"
    WAVES.write_text(WAVES.read_text().replace(ANCHOR, ANCHOR + line, 1))


def decompose(target, zig, zb):
    """Ask the planner for a 5-step incremental ladder; queue the valid steps in order."""
    prompt = (
        f"Decompose the Zig 0.15 feature '{target}' into a LADDER of exactly 5 SMALL programs "
        f"that build it INCREMENTALLY: step 1 is the absolute smallest first use, each later "
        f"step adds ONE increment, step 5 is a full use. Each program MUST compile and run "
        f"under real zig, have `pub fn main() u8` returning a DETERMINISTIC value 1..200 (never "
        f"0 or 101), and use NO @import/std/IO. For EACH, output `FEATURE: {target}_s<N>` then "
        f"the program in a ```zig fence, ordered step 1 to 5."
    )
    progs = parse_programs(sh(["claude", "-p", prompt], timeout=300).stdout or "")
    valid = []  # collect in s1..s5 order
    for n in range(1, 6):
        for feat, src in progs:
            if feat != f"{target}_s{n}":
                continue
            ze = is_gap(zig, zb, src)
            if ze is not None:
                valid.append((feat, src, ze))
            break
    # queue() inserts at the top of the frontier, so queue in REVERSE — that puts
    # s1 (the smallest step) at the top, landed FIRST (the whole point of laddering).
    for feat, src, ze in reversed(valid):
        queue(feat, src, ze, f"ladder step for '{target}'")
    return [f for f, _, _ in valid]


def atomic_discover(zig, zb):
    prompt = (
        "Generate 5 SMALL, DIVERSE, valid Zig 0.15 programs exercising DIFFERENT features. Each: "
        "compiles+runs under real zig; `pub fn main() u8` returns a deterministic 1..200 (not "
        "0/101); no @import/std/IO. For each output `FEATURE: <one_word>` then a ```zig fence."
    )
    for feat, src in parse_programs(sh(["claude", "-p", prompt], timeout=300).stdout or ""):
        if f"] {feat} |" in WAVES.read_text() or f"/{feat}.zig" in WAVES.read_text():
            continue
        ze = is_gap(zig, zb, src)
        if ze is not None:
            queue(feat, src, ze, "self-discovered atomic gap")
            print(f"DISCOVERED atomic gap: {feat} (real zig={ze})", flush=True)
            return True
    return False


def main():
    zig = ensure_zig()
    sh(["cargo", "build", "--quiet"], cwd=str(ZIGRUN), timeout=300)
    zb = str(ZIGRUN / "target" / "debug" / "zigrun")
    done = set(DONE.read_text().split()) if DONE.exists() else set()

    for target in TARGETS:
        if target in done:
            continue
        with DONE.open("a") as f:
            f.write(target + "\n")
        steps = decompose(target, zig, zb)
        if steps:
            print(f"DECOMPOSED '{target}' into {len(steps)} landable steps: {' '.join(steps)}", flush=True)
            return 0
        print(f"could not decompose '{target}' into validated steps — next target", flush=True)

    if atomic_discover(zig, zb):
        return 0
    print("spec_author: no new ladder or atomic gap to propose", flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
