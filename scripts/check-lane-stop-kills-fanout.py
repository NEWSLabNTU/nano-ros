#!/usr/bin/env python3
"""Stopping a lane stops its gate fan-out, and the stop is never a pass — issue 1350.

Killing `just ci gate` used to leave `run-gates-parallel.sh`'s `xargs -P 32`
running as an orphan, spawning fresh gates for minutes, while the stop read as
clean. `just` forwards a signal to its direct child only, so the recipe shell
died and nothing below it was told. The fix is `nros_guard_lane`
(`scripts/build/subtree-guard.sh`): the fan-out runs in its own process group
with a trap and a sentinel inside the group.

This drives the REAL `run-gates-parallel.sh` and `run-lane-steps.sh` against a
stub `just` whose gates are long `sleep`s, and asserts on the process table:

  owner-term    SIGTERM the fan-out's owner: rc 143, NO VERDICT, no survivor.
  parent-term   SIGTERM only the shell ABOVE the owner (what `just` does):
                the owner says STOPPED by SIGHUP / NO VERDICT, no survivor.
  owner-kill    SIGKILL the owner (untrappable): the sentinel still takes the
                fan-out down.
  lane-term     SIGTERM `run-lane-steps.sh` mid-step: rc 143, and it names the
                step it stopped in as NO VERDICT; no survivor.
  unknown-step  a lane step that is not a recipe: rc 2, NO VERDICT, nothing ran.
  control       parent-term with the guard DISABLED must leave survivors —
                otherwise this gate could not see the bug it exists for.

"No survivor" is checked twice, 2 s apart, because the bug's signature was NEW
gates appearing after the kill. Every process is found by a per-run token (the
run's own temp dir, and a unique `sleep` duration) and only those are ever
signalled — never a name, never a broad pattern.
"""

import os
import random
import signal
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GATES = os.path.join(ROOT, "scripts", "build", "run-gates-parallel.sh")
LANE = os.path.join(ROOT, "scripts", "ci", "run-lane-steps.sh")

errors = []


def check(cond, msg):
    if not cond:
        errors.append(msg)
    return cond


class Run:
    def __init__(self, base):
        self.dir = tempfile.mkdtemp(prefix="lane-stop-", dir=base)
        self.sleep = f"300.{random.randrange(10**6, 10**7)}"
        self.list = os.path.join(self.dir, "gates.txt")
        with open(self.list, "w") as fh:
            for i in range(40):
                fh.write(f"g{i:02d}\n")
        bindir = os.path.join(self.dir, "bin")
        os.mkdir(bindir)
        with open(os.path.join(bindir, "just"), "w") as fh:
            fh.write(f"""#!/usr/bin/env bash
case "$1" in
  --show) [ "$2" = nope ] && exit 1; exit 0 ;;
  check) exec bash -c 'sleep "$1" & wait' "{self.dir}/gate-$2" "{self.sleep}" ;;
  fanout) exec bash "{GATES}" "{self.list}" ;;
  nope) echo ran > "{self.dir}/nope-ran"; exit 1 ;;
esac
exit 0
""")
        with open(os.path.join(bindir, "cargo"), "w") as fh:
            fh.write("#!/bin/sh\nexit 0\n")
        for f in ("just", "cargo"):
            os.chmod(os.path.join(bindir, f), 0o755)
        self.env = dict(os.environ, PATH=bindir + os.pathsep + os.environ["PATH"],
                        TMPDIR=self.dir, NROS_GATE_JOBS="4")
        for k in ("NROS_GUARD_DISABLE", "NROS_SUBTREE_GUARD", "NROS_LANE_STEP_RECORD"):
            self.env.pop(k, None)
        self.err = os.path.join(self.dir, "stderr")

    def mine(self):
        """Live pids whose argv carries this run's token."""
        out = []
        for d in os.listdir("/proc"):
            if not d.isdigit() or int(d) == os.getpid():
                continue
            try:
                with open(f"/proc/{d}/cmdline", "rb") as fh:
                    cl = fh.read().decode("utf-8", "replace")
                with open(f"/proc/{d}/stat") as fh:
                    st = fh.read()
            except OSError:
                continue
            if st[st.rfind(")") + 2:].split()[0] == "Z":
                continue
            if self.dir in cl or self.sleep in cl:
                out.append(int(d))
        return out

    def sleepers(self):
        n = 0
        for pid in self.mine():
            try:
                with open(f"/proc/{pid}/cmdline", "rb") as fh:
                    if fh.read().startswith(b"sleep\0"):
                        n += 1
            except OSError:
                pass
        return n

    def wait_fanned_out(self, want=4, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.sleepers() >= want:
                return True
            time.sleep(0.2)
        return False

    def survivors(self, settle=12):
        """Pids still alive after `settle` s, and again 3 s later (respawns)."""
        deadline = time.monotonic() + settle
        while time.monotonic() < deadline and self.mine():
            time.sleep(0.3)
        first = self.mine()
        time.sleep(2)
        return sorted(set(first) | set(self.mine()))

    def reap(self):
        for pid in self.mine():
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass

    def stderr(self):
        try:
            with open(self.err, encoding="utf-8", errors="replace") as fh:
                return fh.read()
        except OSError:
            return ""


def spawn(run, argv, env=None):
    fh = open(run.err, "w")
    return subprocess.Popen(argv, cwd=ROOT, env=env or run.env, stdout=subprocess.DEVNULL,
                            stderr=fh, start_new_session=True)


def wait_rc(p, timeout=20):
    try:
        return p.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return None


def owner_term(base):
    r = Run(base)
    try:
        p = spawn(r, ["bash", GATES, r.list])
        if not check(r.wait_fanned_out(), "owner-term: the fan-out never started"):
            return
        p.send_signal(signal.SIGTERM)
        rc = wait_rc(p)
        check(rc == 143, f"owner-term: rc {rc}, want 143 (a stopped lane must not exit 0)")
        check("NO VERDICT" in r.stderr(), f"owner-term: no NO VERDICT line: {r.stderr()[-300:]!r}")
        s = r.survivors()
        check(not s, f"owner-term: {len(s)} process(es) of the fan-out survived: {s}")
    finally:
        r.reap()


def parent_term(base, disabled=False):
    r = Run(base)
    env = dict(r.env)
    if disabled:
        env["NROS_GUARD_DISABLE"] = "1"
    try:
        # The `; :` keeps bash from exec'ing the script, so the parent is a
        # real shell standing above the owner — the recipe shell `just` kills.
        p = spawn(r, ["bash", "-c", f'bash "{GATES}" "{r.list}"; :'], env)
        if not check(r.wait_fanned_out(), "parent-term: the fan-out never started"):
            return None
        p.send_signal(signal.SIGTERM)
        wait_rc(p, 5)
        s = r.survivors(settle=2 if disabled else 12)
        if disabled:
            return s
        check(not s, f"parent-term: {len(s)} process(es) survived a SIGTERM to the shell "
              f"above the owner — the issue 1350 orphan: {s}")
        err = r.stderr()
        check("STOPPED by SIGHUP" in err and "NO VERDICT" in err,
              f"parent-term: the owner did not report its stop: {err[-300:]!r}")
        return s
    finally:
        r.reap()


def owner_kill(base):
    r = Run(base)
    try:
        p = spawn(r, ["bash", GATES, r.list])
        if not check(r.wait_fanned_out(), "owner-kill: the fan-out never started"):
            return
        p.send_signal(signal.SIGKILL)
        wait_rc(p, 5)
        # A NARROWER window than the default, and the width is the assertion
        # (issue 2012). The sentinel's own drain loop is 10 s; a sentinel that
        # cannot tell its group is empty runs all of it and then SIGKILLs
        # itself, ~11 s after the kill, which the default 12 s window passed by
        # 0.4 s idle and failed under the parallel fast line. A working one is
        # gone in ~1 s (one poll), so 6 s separates the two with room for load.
        s = r.survivors(settle=6)
        check(not s, f"owner-kill: {len(s)} process(es) survived a SIGKILLed owner — "
              f"the sentinel did not fire, or did not notice its group was empty "
              f"(issue 2012): {s}")
    finally:
        r.reap()


def lane_term(base):
    r = Run(base)
    try:
        p = spawn(r, ["bash", LANE, "selftest", "fanout", "quick"])
        if not check(r.wait_fanned_out(), "lane-term: the fan-out never started"):
            return
        p.send_signal(signal.SIGTERM)
        rc = wait_rc(p)
        err = r.stderr()
        check(rc == 143, f"lane-term: rc {rc}, want 143")
        check("STOPPED by SIGTERM during step 1 of 2 (fanout) — NO VERDICT" in err,
              f"lane-term: the lane did not name the step it stopped in: {err[-400:]!r}")
        s = r.survivors()
        check(not s, f"lane-term: {len(s)} process(es) survived: {s}")
    finally:
        r.reap()


def unknown_step(base):
    r = Run(base)
    try:
        p = spawn(r, ["bash", LANE, "selftest", "quick", "nope"])
        rc = wait_rc(p)
        err = r.stderr()
        check(rc == 2, f"unknown-step: rc {rc}, want 2 (no verdict, not a red gate)")
        check("NO VERDICT — step `nope` is not a recipe" in err,
              f"unknown-step: no NO VERDICT line: {err[-300:]!r}")
        check(not os.path.exists(os.path.join(r.dir, "nope-ran")),
              "unknown-step: the lane ran a step before refusing")
    finally:
        r.reap()


def self_test(base):
    """The negative control: with the guard DISABLED, a SIGTERM to the shell
    above the owner must leave survivors — otherwise this gate is blind to the
    orphan it exists for."""
    if not parent_term(base, disabled=True):
        errors.append("control: with the guard DISABLED, a SIGTERM to the shell above "
                      "the owner left no survivor — this gate cannot see issue 1350")


def main():
    base = os.path.join(ROOT, "tmp")
    os.makedirs(base, exist_ok=True)
    base = tempfile.mkdtemp(prefix="check-lane-stop-", dir=base)
    # Independent runs (each with its own token), so they go side by side; the
    # control runs in this thread meanwhile.
    cases = (owner_term, parent_term, owner_kill, lane_term, unknown_step)
    try:
        threads = [threading.Thread(target=c, args=(base,)) for c in cases]
        for t in threads:
            t.start()
        self_test(base)
        for t in threads:
            t.join()
    finally:
        subprocess.run(["rm", "-rf", base])
    if errors:
        print("check-lane-stop-kills-fanout: FAILED (issue 1350)", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("check-lane-stop-kills-fanout: OK — a stopped lane kills its whole fan-out "
          "(owner TERM/KILL, parent TERM) and says NO VERDICT; an unknown step is NO "
          "VERDICT; the guard-disabled control leaves orphans")
    return 0


if __name__ == "__main__":
    sys.exit(main())
