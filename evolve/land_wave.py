#!/usr/bin/env python3
"""Autonomous wave-lander for the zigrun evolving-compiler orchestration.

The orchestration OWNS the full loop — no operator in the recover/gate/merge
path. Given a pending wave (whose spec program the operator/planner authored),
it: chooses a worker (ledger-informed), dispatches, waits for terminal
(re-dispatching on the stuck-in-accepted stall instead of bypassing it),
recovers the worker's patch, gates it against REAL ZIG with the operator's
UN-TAMPERED oracle overlaid (so a worker cannot fake green), and on green merges
the verified src to main, promotes the spec, updates the suites + scorecard,
commits, pushes (working around the ref-stuck remote), and records the outcome
to the ledger so routing improves over time.

This is the orchestration getting better at *doing the task*, not me hand-
reconciling each wave. The honest boundary stays: it executes achievable waves;
it cannot conjure the architecture of a re-foundation wave.

Usage:
  python3 evolve/land_wave.py <wave-id>
  python3 evolve/land_wave.py <wave-id> --task <task-id>   # reuse a finished task
  python3 evolve/land_wave.py <wave-id> --worker <agent-id>
  python3 evolve/land_wave.py <wave-id> --dry-run          # plan only, no dispatch
"""
import argparse, base64, json, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

ZIGRUN = Path(__file__).resolve().parent.parent          # .../zigrun
REPO = ZIGRUN.parent                                      # repo root
WAVES = ZIGRUN / "evolve" / "WAVES.md"
LEDGER = Path.home() / ".nfltr" / "ledger.json"
SEP = "\x1f"
STALL_SECS = 240          # accepted-but-not-running this long => re-dispatch
POLL = 18
WAVE_DEADLINE = 1800      # per-attempt terminal wait
DEFAULT_WORKERS = ["agent-b147cc87.native-actor-0", "agent-b147cc87.native-actor-1",
                   "agent-b147cc87.native-actor-2"]


def sh(cmd, cwd=None, env=None, capture=True, check=False):
    return subprocess.run(cmd, cwd=cwd, env=env, text=True,
                          capture_output=capture, check=check)


def api_env():
    env = dict(os.environ)
    if not env.get("NFLTR_API_KEY"):
        keyfile = Path.home() / ".nfltr_new_key"
        if keyfile.exists():
            env["NFLTR_API_KEY"] = keyfile.read_text().strip()
    return env


def nfltr(*args, env=None):
    return sh(["/Users/b/.local/bin/nfltr", *args], env=env or api_env())


# ---- ledger (same shape as pkg/orchestrator OrchestrationLedger) -------------
def ledger_load():
    if LEDGER.exists():
        try:
            return json.loads(LEDGER.read_text()).get("entries", {})
        except Exception:
            return {}
    return {}


def ledger_record(worker, role, success, fail_code, ms):
    entries = ledger_load()
    k = f"{worker}{SEP}{role}"
    e = entries.get(k) or {"worker": worker, "role": role, "successes": 0,
                           "failures": 0, "total_ms": 0}
    if success:
        e["successes"] += 1
    else:
        e["failures"] += 1
        if fail_code:
            e["last_fail_code"] = fail_code
    e["total_ms"] += max(0, int(ms))
    entries[k] = e
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    LEDGER.write_text(json.dumps({"entries": entries}, indent=2))


def pick_worker(role="implementer"):
    """Ledger-informed: best success-rate worker for the role, else first default."""
    entries = ledger_load()
    best, best_rate = None, -1.0
    for e in entries.values():
        if e.get("role") != role:
            continue
        n = e["successes"] + e["failures"]
        rate = e["successes"] / n if n else 0
        if rate > best_rate:
            best, best_rate = e["worker"], rate
    return best or DEFAULT_WORKERS[0]


# ---- WAVES.md parsing --------------------------------------------------------
def find_pending(wave_id):
    for line in WAVES.read_text().splitlines():
        m = re.match(r"- \[ \] (\S+) \| (\S+) \| (.+)", line)
        if m and m.group(1) == wave_id:
            return m.group(2), m.group(3)        # spec_path, objective
    return None, None


# ---- dispatch + wait (with stall re-dispatch) --------------------------------
def task_state(task_id, env):
    r = nfltr("orch", "status", "--task", task_id, "--json", env=env)
    m = re.search(r'"state":\s*"([^"]+)"', r.stdout)
    return m.group(1) if m else "unknown"


def dispatch(worker, objective_file, env):
    r = nfltr("orch", "dispatch", "--worker", worker, "--role", "implementer",
              "--objective-file", objective_file, "--timeout-ms", "1500000",
              "--json", env=env)
    m = re.search(r'"task_id":"([^"]+)"', r.stdout)
    if not m:
        raise RuntimeError(f"dispatch returned no task_id: {r.stdout}{r.stderr}")
    return m.group(1)


def wait_or_redispatch(wave_id, worker, objective_file, env, retries=2):
    """Dispatch, wait for terminal; if it stalls in accepted, cancel + re-dispatch
    on the next worker (the orch HANDLING its own friction, not me bypassing)."""
    workers = [worker] + [w for w in DEFAULT_WORKERS if w != worker]
    for attempt in range(retries + 1):
        w = workers[min(attempt, len(workers) - 1)]
        tid = dispatch(w, objective_file, env)
        print(f"  dispatch attempt {attempt+1}: {wave_id} -> {w}  ({tid})", flush=True)
        start = time.time(); accepted_since = None
        while time.time() - start < WAVE_DEADLINE:
            st = task_state(tid, env)
            if st in ("completed", "failed", "cancelled"):
                if st == "completed":
                    return tid, w, st
                print(f"    terminal={st}; re-dispatching", flush=True); break
            if st == "accepted":
                accepted_since = accepted_since or time.time()
                if time.time() - accepted_since > STALL_SECS:
                    print("    STALL (accepted, never ran) — cancel + re-dispatch", flush=True)
                    nfltr("orch", "cancel", "--task", tid, env=env); break
            else:
                accepted_since = None
            time.sleep(POLL)
        else:
            nfltr("orch", "cancel", "--task", tid, env=env)
            print("    deadline — re-dispatching", flush=True)
    return None, workers[-1], "stalled"


# ---- recover the worker's patch ---------------------------------------------
def recover_patch(task_id, env):
    r = nfltr("orch", "status", "--task", task_id, "--events", "5", env=env)
    raw = r.stdout
    i = raw.find("{", raw.find("Result:"))
    if i < 0:
        return None
    depth, end = 0, None
    for k in range(i, len(raw)):
        if raw[k] == "{": depth += 1
        elif raw[k] == "}":
            depth -= 1
            if depth == 0: end = k + 1; break
    try:
        obj = json.loads(raw[i:end])
        arts = obj.get("artifacts") or []
        if arts and arts[0].get("inline_content"):
            return base64.b64decode(arts[0]["inline_content"])
    except Exception as e:
        print(f"  recover_patch parse error: {e}", flush=True)
    return None


# ---- gate against real zig with OUR un-tampered oracle -----------------------
def gate(wave_id, spec_path, patch_bytes):
    """Apply the worker patch in a scratch tree, overlay the operator's oracle
    (anti-tamper), promote the spec, and run the differential gate (real zig).
    Returns (green, scratch_zigrun_path, detail)."""
    scratch = Path(tempfile.mkdtemp(prefix="zigrun-land."))
    pf = scratch / "patch.diff"; pf.write_bytes(patch_bytes)
    sh(["git", "init", "-q"], cwd=scratch)
    ap = sh(["git", "apply", str(pf)], cwd=scratch)
    if ap.returncode != 0:
        return False, scratch, f"git apply failed: {ap.stderr.strip()[:200]}"
    sz = scratch / "zigrun"
    if not (sz / "src").exists():
        return False, scratch, "patch produced no zigrun/src"
    # overlay OUR oracle (discard whatever the worker shipped) + promote OUR spec
    shutil.rmtree(sz / "oracle", ignore_errors=True)
    shutil.copytree(ZIGRUN / "oracle", sz / "oracle")
    spec_src = ZIGRUN / spec_path  # WAVES.md paths are relative to zigrun/
    shutil.copy(spec_src, sz / "oracle" / f"{wave_id}.zig")
    # run the differential gate (real zig is truth)
    g = sh(["bash", "oracle/diff.sh", wave_id], cwd=sz)
    full = sh(["bash", "oracle/diff.sh"], cwd=sz)
    green = (g.returncode == 0 and full.returncode == 0)
    detail = (g.stdout + g.stderr + "\n" + full.stdout + full.stderr).strip()
    return green, scratch, detail


# ---- merge verified src to main + bookkeep + commit --------------------------
def land_on_main(wave_id, spec_path, scratch, worker, env):
    sz = scratch / "zigrun"
    for f in ("ast.rs", "lexer.rs", "parser.rs", "codegen.rs", "main.rs"):
        src = sz / "src" / f
        if src.exists():
            shutil.copy(src, ZIGRUN / "src" / f)
    # promote spec into the suite (git runs from REPO; spec_path is under zigrun/)
    sh(["git", "mv", f"zigrun/{spec_path}", f"zigrun/oracle/{wave_id}.zig"], cwd=REPO)
    # derive expected exit from real zig and extend the default suites
    for suite in ("check.sh", "diff.sh"):
        p = ZIGRUN / "oracle" / suite
        t = p.read_text()
        t = re.sub(r"(progs=\(add[^)]*?)\)", lambda m: m.group(1) + f" {wave_id})"
                   if wave_id not in m.group(1) else m.group(0), t)
        p.write_text(t)
    # FINAL verify on the real tree (real zig)
    sh(["cargo", "build", "--quiet"], cwd=ZIGRUN)
    final = sh(["bash", "oracle/diff.sh"], cwd=ZIGRUN)
    if final.returncode != 0:
        return False, final.stdout + final.stderr
    msg = (f"feat(zigrun): {wave_id} wave — landed autonomously, verified vs real zig\n\n"
           f"Dispatched, recovered the worker patch, gated against real zig with the\n"
           f"operator's un-tampered oracle, merged the verified src, promoted the spec,\n"
           f"and re-verified the full differential suite on main — no operator in the\n"
           f"recover/gate/merge path.\n\n"
           f"Co-authored-by: Cursor <cursoragent@cursor.com>\n"
           f"Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>")
    sh(["git", "add", "zigrun/src", f"zigrun/oracle/{wave_id}.zig",
        "zigrun/oracle/check.sh", "zigrun/oracle/diff.sh"], cwd=REPO)
    subprocess.run(["git", "commit", "-q", "-F", "-"], cwd=REPO, input=msg, text=True)
    return True, final.stdout


def push():
    target = sh(["git", "rev-parse", "HEAD"], cwd=REPO).stdout.strip()
    for _ in range(6):
        sh(["git", "push", "origin", "HEAD:refs/heads/main"], cwd=REPO)
        srv = sh(["git", "ls-remote", "origin", "main"], cwd=REPO).stdout.split()
        if srv and srv[0] == target:
            return True
        time.sleep(10)
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wave_id")
    ap.add_argument("--task")
    ap.add_argument("--worker")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    env = api_env()

    spec_path, objective = find_pending(a.wave_id)
    if not spec_path:
        print(f"land: no pending wave '{a.wave_id}' in WAVES.md"); sys.exit(2)
    if not (ZIGRUN / spec_path).exists():  # WAVES.md paths are relative to zigrun/
        print(f"land: spec program missing: zigrun/{spec_path} (operator/planner authors the 'what')")
        sys.exit(2)
    sh(["bash", str(ZIGRUN / "oracle" / "ensure_zig.sh")])  # self-provision truth

    worker = a.worker or pick_worker("implementer")
    print(f"== autonomous wave-lander: {a.wave_id} (worker {worker}) ==", flush=True)
    if a.dry_run:
        print(f"  spec: {spec_path}\n  objective: {objective}"); return

    t0 = time.time()
    if a.task:
        task_id, used = a.task, worker
    else:
        objfile = str(REPO / "out" / f"{a.wave_id}-wave.txt")
        if not Path(objfile).exists():
            print(f"land: objective file {objfile} missing (author it, or use --task)"); sys.exit(2)
        task_id, used, st = wait_or_redispatch(a.wave_id, worker, objfile, env)
        if not task_id:
            print("  RESULT: fleet would not accept the work (stalled across retries).")
            print("  The orch surfaced its own friction instead of an operator bypassing it.")
            sys.exit(1)

    patch = recover_patch(task_id, env)
    if not patch:
        ledger_record(used, "implementer", False, "no_patch", (time.time()-t0)*1000)
        print("  RESULT: worker returned no recoverable patch."); sys.exit(1)

    green, scratch, detail = gate(a.wave_id, spec_path, patch)
    print("  --- differential gate (real zig) ---")
    print("  " + detail.replace("\n", "\n  "))
    if not green:
        ledger_record(used, "implementer", False, "gate_red", (time.time()-t0)*1000)
        shutil.rmtree(scratch, ignore_errors=True)
        print(f"  RESULT: RED — {a.wave_id} diverges from real zig. (re-dispatch with this feedback)")
        sys.exit(1)

    ok, finaldetail = land_on_main(a.wave_id, spec_path, scratch, used, env)
    shutil.rmtree(scratch, ignore_errors=True)
    if not ok:
        ledger_record(used, "implementer", False, "merge_regressed", (time.time()-t0)*1000)
        print(f"  RESULT: scratch green but on-main verify regressed:\n{finaldetail}"); sys.exit(1)
    ledger_record(used, "implementer", True, "", (time.time()-t0)*1000)
    pushed = push()
    print(f"  RESULT: {a.wave_id} LANDED autonomously — full suite green vs real zig; "
          f"pushed={pushed}. Now flip WAVES.md/[x] + FEATURES, rerun evolve.sh.")


if __name__ == "__main__":
    main()
