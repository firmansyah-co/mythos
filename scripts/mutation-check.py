#!/usr/bin/env python3
"""Mutation check: break the code on purpose and show that the tests notice.

Usage: scripts/mutation-check.py SPEC.json [--keep]

SPEC.json (paths are relative to the repository root):

    {
      "setup": [{"cmd": ["pnpm", "install", "--frozen-lockfile"], "cwd": "frontend"}],
      "test":  {"cmd": ["cargo", "test", "-p", "mythos-update"], "cwd": ".", "timeout": 1800},
      "mutations": [
        {"name": "what this mutation removes",
         "file": "crates/x/src/y.rs",
         "replace": [{"old": "exact text", "new": "exact text"}]}
      ]
    }

  setup      optional; each command runs once in the copy, in order, before the baseline.
  test       the command that must be green on the baseline and red on every mutation.
             "cwd" defaults to "."; "timeout" (seconds) is optional, and a run that
             exceeds it is "timed out", which is not red.
  mutations  each "old" must occur exactly once in the file (checked in order, after the
             replacements before it); "new" may be empty to delete "old".

What it does:
  1. Copies the HEAD commit (git archive) into target/mutation/<run>/ and sets
     CARGO_TARGET_DIR inside that copy. The working tree is never edited. A spec that
     names a file not in HEAD is refused before anything runs.
  2. Runs setup, then the test command: the baseline. A red baseline stops the check.
  3. For each mutation: applies it, reads the file back to confirm it landed, runs the
     test command and classifies the result:
       red             tests failed (their names are listed)
       survived        everything passed: the tests do not hold what was removed
       did not compile a build error and no test failed: not counted as red
       did not apply   an "old" text was not found exactly once: nothing was run
       failed, no test names / timed out   the run failed some other way: not red
     Then writes the file anew from `git show <commit>:<file>` (a new modification time,
     so cargo rebuilds it).
  4. Runs the baseline again; it must still be green.
  5. Prints a Markdown table. Logs of every run are in target/mutation/<run>.logs/.

Exit 0 only if both baselines were green and every mutation is red; otherwise exit 1 and
the rules that failed are printed. The copy is removed at the end unless --keep is given.
Standard library only.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
CARGO_FAILED = re.compile(r"^test (\S+) \.\.\. FAILED$", re.M)
CARGO_RESULT = re.compile(r"^test result: ", re.M)
# vitest: "FAIL  path > suite > test" in the failure list, "× suite > test" in the run.
VITEST_FAILED = re.compile(r"^\s*(?:FAIL|×)\s+(\S.*?)(?:\s+\d+ms)?$", re.M)
VITEST_RESULT = re.compile(r"^\s*Tests\s+\d", re.M)
COMPILE_ERROR = re.compile(
    r"^error(\[E\d+\])?:|could not compile|^\s*Error: Transform failed|SyntaxError", re.M
)
NAMES_SHOWN = 8


def fail(msg):
    print(f"mutation-check: {msg}", file=sys.stderr)
    sys.exit(1)


def git(*args, binary=False):
    out = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, check=False)
    if out.returncode != 0:
        fail(f"git {' '.join(args)} failed: {out.stderr.decode(errors='replace').strip()}")
    return out.stdout if binary else out.stdout.decode()


def load_spec(path):
    try:
        spec = json.loads(Path(path).read_text())
    except (OSError, json.JSONDecodeError) as e:
        fail(f"cannot read spec {path}: {e}")

    def command(c, where):
        if not isinstance(c, dict) or not isinstance(c.get("cmd"), list) or not c["cmd"] \
                or not all(isinstance(a, str) for a in c["cmd"]):
            fail(f'{where} must be {{"cmd": [strings...], "cwd": optional}}')
        return {"cmd": c["cmd"], "cwd": c.get("cwd", "."), "timeout": c.get("timeout")}

    setup = [command(c, f"setup[{i}]") for i, c in enumerate(spec.get("setup", []))]
    test = command(spec.get("test"), "test")
    muts = spec.get("mutations")
    if not isinstance(muts, list) or not muts:
        fail("mutations must be a non-empty list")
    for i, m in enumerate(muts):
        if not isinstance(m.get("name"), str) or not isinstance(m.get("file"), str):
            fail(f"mutations[{i}] needs a name and a file")
        reps = m.get("replace")
        if not isinstance(reps, list) or not reps or not all(
            isinstance(r, dict) and isinstance(r.get("old"), str) and r["old"]
            and isinstance(r.get("new"), str) for r in reps
        ):
            fail(f'mutations[{i}] ({m["name"]}): replace must be a list of {{"old": non-empty, "new": text}}')
    return setup, test, muts


def run(cmd, copy, log_path):
    """Run a command in the copy. Returns (exit code or None on timeout, output text)."""
    env = {**os.environ, "CARGO_TARGET_DIR": str(copy / "target")}
    try:
        p = subprocess.run(cmd["cmd"], cwd=copy / cmd["cwd"], env=env, capture_output=True,
                           timeout=cmd["timeout"], check=False)
        rc, out = p.returncode, (p.stdout + p.stderr).decode(errors="replace")
    except subprocess.TimeoutExpired as e:
        rc = None
        out = ((e.stdout or b"") + (e.stderr or b"")).decode(errors="replace")
    except OSError as e:
        rc, out = 127, f"could not start {cmd['cmd'][0]}: {e}\n"
    out = ANSI.sub("", out)
    log_path.write_text(out)
    return rc, out


def failed_names(out):
    names = CARGO_FAILED.findall(out)
    names += [n for n in VITEST_FAILED.findall(out) if " > " in n or n.endswith((".ts", ".tsx"))]
    return list(dict.fromkeys(names))


def classify(rc, out):
    names = failed_names(out)
    if rc is None:
        return "timed out", names
    if rc == 0:
        return "survived", []
    if names:
        return "red", names
    if COMPILE_ERROR.search(out):
        return "did not compile", []
    return "failed, no test names", []


def tests_ran(out):
    return bool(CARGO_RESULT.search(out) or VITEST_RESULT.search(out))


def baseline(test, copy, logs, label):
    rc, out = run(test, copy, logs / f"{label}.log")
    if rc == 0 and tests_ran(out):
        print(f"{label}: green", flush=True)
        return True
    why = "timed out" if rc is None else ("no test ran" if rc == 0 else f"exit {rc}")
    names = failed_names(out)
    print(f"{label}: NOT green ({why}){': ' + ', '.join(names[:NAMES_SHOWN]) if names else ''}"
          f" — log {logs / (label + '.log')}", flush=True)
    return False


def main():
    ap = argparse.ArgumentParser(description="Show that tests fail when the code they guard is broken.")
    ap.add_argument("spec")
    ap.add_argument("--keep", action="store_true", help="keep the copy under target/mutation/")
    a = ap.parse_args()
    setup, test, muts = load_spec(a.spec)

    commit = git("rev-parse", "HEAD").strip()
    missing = sorted({m["file"] for m in muts
                      if subprocess.run(["git", "-C", str(ROOT), "cat-file", "-e", f"{commit}:{m['file']}"],
                                        capture_output=True).returncode != 0})
    if missing:
        fail(f"not in HEAD ({commit[:12]}): {', '.join(missing)}")

    run_id = time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"
    base = ROOT / "target" / "mutation"
    copy, logs = base / run_id, base / f"{run_id}.logs"
    copy.mkdir(parents=True)
    logs.mkdir()
    archive = subprocess.run(["git", "-C", str(ROOT), "archive", commit], capture_output=True, check=True)
    subprocess.run(["tar", "-x", "-C", str(copy)], input=archive.stdout, check=True)

    print(f"commit {commit}")
    print(f"test   {' '.join(test['cmd'])}  (cwd {test['cwd']})")
    print(f"copy   {copy}\nlogs   {logs}\n", flush=True)

    rules_failed = []
    rows = []
    try:
        for i, s in enumerate(setup):
            rc, _ = run(s, copy, logs / f"setup-{i}.log")
            if rc != 0:
                rules_failed.append(f"setup {' '.join(s['cmd'])} failed (exit {rc}), log {logs}/setup-{i}.log")
                return report(commit, test, rows, rules_failed)
        if not baseline(test, copy, logs, "baseline"):
            rules_failed.append("the baseline is not green: a mutation check on a red baseline proves nothing")
            return report(commit, test, rows, rules_failed)

        for n, m in enumerate(muts, 1):
            path = copy / m["file"]
            original = git("show", f"{commit}:{m['file']}", binary=True)
            text = original.decode()
            applied = True
            for r in m["replace"]:
                if text.count(r["old"]) != 1:
                    applied = False
                    break
                text = text.replace(r["old"], r["new"])
            if not applied or text == original.decode():
                rows.append((m["name"], "did not apply", []))
                print(f"[{n}/{len(muts)}] {m['name']}: did not apply", flush=True)
                continue
            try:
                path.write_text(text)
                back = path.read_text()
                landed = all((r["new"] in back) if r["new"] else (r["old"] not in back) for r in m["replace"])
                if not landed:
                    rows.append((m["name"], "did not apply", []))
                    print(f"[{n}/{len(muts)}] {m['name']}: did not land on read-back", flush=True)
                    continue
                rc, out = run(test, copy, logs / f"mutation-{n}.log")
                outcome, names = classify(rc, out)
                rows.append((m["name"], outcome, names))
                print(f"[{n}/{len(muts)}] {m['name']}: {outcome}"
                      f"{' (' + str(len(names)) + ')' if names else ''}", flush=True)
            finally:
                # Written anew, not moved back: a new modification time, so cargo rebuilds it.
                path.unlink()
                path.write_bytes(original)
            if path.read_bytes() != original:
                rules_failed.append(f"{m['file']} could not be restored")
                return report(commit, test, rows, rules_failed)

        if not baseline(test, copy, logs, "baseline-after"):
            rules_failed.append("the baseline after the mutations is not green")
        not_red = [f"{name} ({outcome})" for name, outcome, _ in rows if outcome != "red"]
        if not_red:
            rules_failed.append("not every mutation is red: " + "; ".join(not_red))
        return report(commit, test, rows, rules_failed)
    finally:
        if a.keep:
            print(f"\nkept {copy}")
        else:
            shutil.rmtree(copy, ignore_errors=True)


def report(commit, test, rows, rules_failed):
    print(f"\n### Mutation check\n\ncommit `{commit}`  \ntest `{' '.join(test['cmd'])}` (cwd `{test['cwd']}`)\n")
    if rows:
        print("| mutation | outcome | red tests | names |")
        print("|---|---|---|---|")
        for name, outcome, names in rows:
            shown = ", ".join(f"`{x}`" for x in names[:NAMES_SHOWN])
            if len(names) > NAMES_SHOWN:
                shown += f" and {len(names) - NAMES_SHOWN} more"
            cell = lambda s: s.replace("|", "\\|")
            print(f"| {cell(name)} | {outcome} | {len(names) if outcome == 'red' else 0} | {cell(shown)} |")
    print()
    if rules_failed:
        for r in rules_failed:
            print(f"FAIL: {r}")
        return 1
    print("PASS: both baselines green and every mutation red")
    return 0


if __name__ == "__main__":
    sys.exit(main())
