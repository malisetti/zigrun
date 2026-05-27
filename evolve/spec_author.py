#!/usr/bin/env python3
"""Self-authoring spec generator — the orchestration proposes its OWN next wave.

A planner LLM (claude) writes a small valid Zig program for the next backlog
feature; REAL ZIG then validates it mechanically:
  - real zig compiles + runs it           -> it's valid Zig + gives ground truth
  - the exit code is deterministic, 1..200, != 101 (the error sentinel)
  - zigrun currently does NOT match it     -> it's a genuine gap (a RED target)
A valid spec is added to the frontier. Truth stays EXTERNAL (real zig), so a
self-authored spec can never assert a wrong answer — the LLM only proposes the
program; zig decides what it does. This is what lets the orch feed itself.

Exit 0 = authored one new spec; exit 1 = nothing left it could validly author.
"""
import re, subprocess, sys
from pathlib import Path

ZIGRUN = Path(__file__).resolve().parent.parent
WAVES = ZIGRUN / "evolve" / "WAVES.md"
ATTEMPTED = ZIGRUN / "evolve" / ".authored_features"

# The planner's curriculum: ordered feature hints, each becomes one wave.
BACKLOG = [
    ("arraysum", "a fixed-size array `var a: [5]u8 = .{...}` summed with a for-loop"),
    ("arrayidx", "fixed-size array declaration and indexing `a[i]` to return an element"),
    ("atmod", "the @mod or @rem builtin on two integers"),
    ("enumval", "an enum with a few variants and a switch returning a number per variant"),
    ("structfield", "a struct with integer fields: construct it, read a field, return it"),
    ("nestedexpr", "a deeply nested arithmetic expression combining +,-,*,/,% with parentheses"),
]


def sh(cmd, timeout=120, cwd=None):
    return subprocess.run(cmd, text=True, capture_output=True, timeout=timeout, cwd=cwd)


def ensure_zig():
    r = sh(["bash", str(ZIGRUN / "oracle" / "ensure_zig.sh")])
    out = (r.stdout or "").strip().splitlines()
    return out[-1] if out else "zig"


def extract_zig(text):
    """Pull just the Zig program out of an LLM response (it may add prose)."""
    m = re.search(r"```(?:zig)?\s*\n(.*?)```", text, re.S)
    if m:
        return m.group(1).strip() + "\n"
    lines = text.splitlines()  # fallback: from the first top-level decl
    for i, l in enumerate(lines):
        if re.match(r"\s*(const |var |pub fn |fn )", l):
            return "\n".join(lines[i:]).strip() + "\n"
    return text.strip() + "\n"


def gen_program(desc):
    prompt = (
        f"Write ONE small, valid Zig program (Zig 0.15) exercising this feature: {desc}.\n"
        "Hard requirements:\n"
        "- MUST compile and run under the real `zig` compiler.\n"
        "- MUST have `pub fn main() u8` returning a DETERMINISTIC value between 1 and 200 "
        "(the program's result, used as the process exit code). Never return 0 or 101.\n"
        "- No `@import`, no std, no I/O, no printing — pure computation only.\n"
        "- Put the program in a single ```zig code fence and nothing else of substance."
    )
    r = sh(["claude", "-p", prompt], timeout=150)
    return extract_zig(r.stdout or "")


def validate(wave_id, src, zig, zigrun_bin):
    spec = ZIGRUN / "oracle" / "pending" / f"{wave_id}.zig"
    spec.write_text(src)
    zr = sh([zig, "run", str(spec)])
    if "error:" in (zr.stderr or "").lower():
        spec.unlink(missing_ok=True)
        return False, "real zig rejected the program"
    ze = zr.returncode
    if ze == 0 or ze == 101 or ze > 255:
        spec.unlink(missing_ok=True)
        return False, f"unusable result exit={ze} (need 1..200, !=101)"
    rr = sh([zigrun_bin, "run", str(spec)])
    if rr.returncode == ze:
        spec.unlink(missing_ok=True)
        return False, f"zigrun already matches ({ze}) — not a gap"
    return True, f"VALID: real zig={ze}, zigrun diverges ({rr.returncode})"


def already_known(wave_id):
    s = WAVES.read_text()
    return (f"] {wave_id} |" in s) or (f"/{wave_id}.zig" in s)


def main():
    zig = ensure_zig()
    sh(["cargo", "build", "--quiet"], cwd=str(ZIGRUN), timeout=300)
    zigrun_bin = str(ZIGRUN / "target" / "debug" / "zigrun")
    done = set(ATTEMPTED.read_text().split()) if ATTEMPTED.exists() else set()

    for wave_id, desc in BACKLOG:
        if wave_id in done or already_known(wave_id):
            continue
        with ATTEMPTED.open("a") as f:
            f.write(wave_id + "\n")
        for attempt in range(3):
            src = gen_program(desc)
            if "fn main" not in src:
                print(f"author {wave_id} attempt {attempt+1}: LLM gave no usable program", flush=True)
                continue
            ok, why = validate(wave_id, src, zig, zigrun_bin)
            print(f"author {wave_id} attempt {attempt+1}: {why}", flush=True)
            if ok:
                anchor = "## Frontier (pending — each is real Zig that zigrun must learn to match)\n"
                line = f"- [ ] {wave_id} | oracle/pending/{wave_id}.zig | {desc} (self-authored, real-zig-validated)\n"
                WAVES.write_text(WAVES.read_text().replace(anchor, anchor + line, 1))
                print(f"AUTHORED: {wave_id} added to the frontier", flush=True)
                return 0
        print(f"could not author a valid spec for {wave_id} after 3 tries — moving on", flush=True)
    print("self-author: backlog exhausted / all attempted — nothing new to propose", flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
