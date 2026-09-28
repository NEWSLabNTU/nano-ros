#!/usr/bin/env python3
"""The jobserver stall watchdog fires on a stall, and ONLY on a stall — issue 1403.

`scripts/build/make-stall-watchdog.py` wraps every fixture/jobserver `make`. A
watchdog that cannot fire is a comment, and one that fires on a slow-but-working
build turns a real verdict into a false "no verdict" — so both directions are
exercised here, deterministically, against fake makes rather than a real
deadlock (which nobody can produce on demand; that is the whole issue):

  stall       a fake make that holds a jobserver FIFO and blocks READING it,
              with no children. A second process OUTSIDE its tree also holds
              the FIFO. Must: exit NO_VERDICT_RC, print the NO VERDICT line,
              leave a diagnostic naming the FIFO, its FIONREAD count and the
              outside holder, kill the fake's tree, and NOT kill the holder.
  productive  a fake make whose only child runs LONGER than the threshold.
              Busy is not stalled: exit 0, no output, no diagnostic.
  passthrough a failing fake make keeps its own exit status; a quick one
              returns at once even with the default 15 s poll.
  disabled    NROS_JOBSERVER_STALL_SECS=0 execs the command in place.

The stall case is this gate's negative control and runs on every invocation
(`self_test()` below). Buildless; ~10 s, most of it the productive child.
"""

import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WATCHDOG = os.path.join(ROOT, "scripts", "build", "make-stall-watchdog.py")
NO_VERDICT_RC = 75
STALL = "2"
POLL = "0.5"

errors = []


def check(cond, msg):
    if not cond:
        errors.append(msg)
    return cond


def group_members(pgid):
    out = []
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/stat") as fh:
                data = fh.read()
        except OSError:
            continue
        rest = data[data.rfind(")") + 2:].split()
        if rest[0] != "Z" and int(rest[2]) == pgid:
            out.append(int(d))
    return out


def run_watchdog(tmp, fake_body, env_extra, timeout=40):
    """Run the watchdog over `bash -c fake_body` in a NEW session we own, so
    whatever it leaves behind is findable by that group id and killable by it
    — never by a name."""
    diag = tempfile.mkdtemp(prefix="diag-", dir=tmp)
    os.rmdir(diag)  # the watchdog must create it, and only when it fires
    env = dict(os.environ, NROS_JOBSERVER_STALL_SECS=STALL, NROS_JOBSERVER_STALL_POLL_SECS=POLL)
    env.update(env_extra)
    start = time.monotonic()
    p = subprocess.Popen(
        [sys.executable, WATCHDOG, "--label", "selftest", "--diag-dir", diag,
         "--", "bash", "-c", fake_body],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env,
        start_new_session=True)
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        out, err = p.communicate()
        errors.append(f"watchdog did not return within {timeout}s — it never fired "
                      f"(stderr tail: {err[-300:]!r})")
    elapsed = time.monotonic() - start
    time.sleep(0.2)
    leftovers = group_members(p.pid)
    for pid in leftovers:
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
    return p.returncode, out, err, diag, elapsed, leftovers, p.pid


def self_test(tmp):
    """The negative control: a stalled make MUST be caught."""
    fifo = os.path.join(tmp, "GMfifo-selftest")
    os.mkfifo(fifo)
    # A second FIFO with two bytes queued proves FIONREAD reports a real count
    # rather than a constant 0.
    fifo2 = os.path.join(tmp, "GMfifo-queued")
    os.mkfifo(fifo2)
    pidfile = os.path.join(tmp, "fake.pid")
    # An unrelated process holding the SAME fifo, outside the watchdog's tree
    # — the "other jobserver holder" the diagnostic must name, and must spare.
    holder = subprocess.Popen(["bash", "-c", f"exec 3<>{fifo}; exec sleep 60"],
                              start_new_session=True)
    try:
        body = (f"echo $$ > {pidfile}; export MAKEFLAGS=' -j4 --jobserver-auth=fifo:{fifo}'; "
                f"exec 4<>{fifo2}; printf xx >&4; exec 3<>{fifo}; read -r -u 3 line")
        rc, out, err, diag, _el, leftovers, _ = run_watchdog(tmp, body, {})
        check(rc == NO_VERDICT_RC, f"stall: rc {rc}, want {NO_VERDICT_RC}")
        m = re.search(r"^NO VERDICT: jobserver stall — (\S+)$", err, re.M)
        if check(m, f"stall: no `NO VERDICT: jobserver stall — <path>` line in stderr: {err[-400:]!r}"):
            path = m.group(1)
            check(os.path.dirname(path) == os.path.abspath(diag),
                  f"stall: diag {path} not in --diag-dir {diag}")
            text = open(path).read() if os.path.exists(path) else ""
            check(text, f"stall: diag file {path} missing or empty")
            fake = open(pidfile).read().strip()
            for needle, why in (
                (f"make pid:       {fake}", "the stalled pid"),
                (f"/proc/{fake}/wchan", "the wchan (or its unreadable reason)"),
                (f"/proc/{fake}/stack", "the stack (or its unreadable reason)"),
                (f"jobserver-auth", None),
                (f"fifo:{fifo}", "the jobserver FIFO from MAKEFLAGS"),
                # Blocked in read on an EMPTY fifo — the issue's shape, zero tokens.
                (f"-> {fifo}  [named FIFO]  bytes-available(FIONREAD)=0", "the FIFO fd and its token count"),
                (f"-> {fifo2}  [named FIFO]  bytes-available(FIONREAD)=2", "a real FIONREAD count"),
                (f"pid {holder.pid} ", "the outside holder"),
                (f"-> {fifo} is one of THIS make's pipe/FIFO fds", "that the holder shares make's FIFO"),
                ("NOT in this make's tree", "that the holder is outside the tree"),
                ("-- gdb backtrace --", "the gdb section"),
            ):
                check(needle in text, f"stall: diag lacks {why or needle!r} ({needle!r})")
            check(text in err, "stall: diagnostics not also printed to stderr")
        check(not leftovers, f"stall: process group not gone, survivors {leftovers}")
        check(holder.poll() is None, "stall: the watchdog killed a process OUTSIDE make's tree")
    finally:
        holder.kill()
        holder.wait()


def main():
    base = os.path.join(ROOT, "tmp")
    os.makedirs(base, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="make-stall-watchdog-", dir=base)
    try:
        self_test(tmp)

        # Busy, not stalled: the only child outlives the threshold 2.5x.
        rc, out, err, diag, _el, leftovers, _ = run_watchdog(
            tmp, "sleep 5; echo built", {})
        check(rc == 0, f"productive: rc {rc}, want 0 (err {err[-300:]!r})")
        check(out == "built\n" and err == "", f"productive: extra output {out!r} {err!r}")
        check(not os.path.exists(diag), "productive: wrote a diagnostic for a working build")
        check(not leftovers, f"productive: survivors {leftovers}")

        # Exit status passes through; the default 15 s poll must not delay return.
        rc, out, err, _d, elapsed, leftovers, _ = run_watchdog(
            tmp, "exit 3", {"NROS_JOBSERVER_STALL_POLL_SECS": ""})
        check(rc == 3 and err == "", f"passthrough: rc {rc} err {err!r}, want 3 and silence")
        check(elapsed < 5, f"passthrough: returned after {elapsed:.1f}s, poll must not delay exit")
        rc, _o, _e, _d, _el, _l, _ = run_watchdog(tmp, "kill -TERM $$", {})
        check(rc == 128 + signal.SIGTERM, f"passthrough: signal death rc {rc}, want 143")

        # Disabled: exec in place, so the command IS the wrapper's pid.
        rc, out, err, _d, _el, _l, wpid = run_watchdog(
            tmp, "echo $$", {"NROS_JOBSERVER_STALL_SECS": "0"})
        check(rc == 0 and out.strip() == str(wpid),
              f"disabled: want exec in place (pid {wpid}), got rc {rc} out {out!r}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if errors:
        print("check-make-stall-watchdog: FAILED (issue 1403)", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("check-make-stall-watchdog: OK — stall caught with diagnostics and a NO VERDICT "
          "exit; productive, failing, quick and disabled runs untouched")
    return 0


if __name__ == "__main__":
    sys.exit(main())
