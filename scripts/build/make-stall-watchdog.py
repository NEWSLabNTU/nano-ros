#!/usr/bin/env python3
"""Run a jobserver `make` and turn a silent stall into an explicit NO VERDICT.

Issue 1403. `just threadx_linux build-examples` once left GNU make 4.4.1
blocked in `pipe_read` on its own `--jobserver-style=fifo` FIFO with ZERO
children and nothing left to run. A lane in that state produces no verdict: it
looks like a slow build until a timeout kills it, and a timeout reads as
infrastructure flake. The cause is NOT known, and this file does not try to fix
it. It does the two things that make the next occurrence a bug someone can
take:

  1. EVIDENCE. When make has had no live child and burned no CPU for
     `NROS_JOBSERVER_STALL_SECS`, dump what the kernel will tell us about it —
     its stack (usually root-only) or wchan/status/syscall, its fds, the
     jobserver FIFO and how many tokens sit in it (FIONREAD), and every OTHER
     process on the host that holds a jobserver — to a file in the build's log
     dir and to stderr.
  2. A VERDICT OF "NONE". Kill make and its descendants — found by walking
     parent pids down from the ONE pid this wrapper started, never by name —
     and exit `NO_VERDICT_RC` with the line
         NO VERDICT: jobserver stall — <diag path>
     which `just nightly-triage` reads as "the lane never ran".

Why the descendant walk and not `kill -- -<pgid>`: under
`scripts/build/subtree-guard.sh` the make shares the OUTERMOST launcher's
process group on purpose (issue 0762), so killing "make's group" would kill the
whole build tree including the launcher. Giving make its own group instead
would break the guard's reach, which that file explains is worse. Walking ppids
from the pid we spawned reaches exactly what we started.

Normal builds are untouched: the wrapper is silent unless it fires, returns the
command's own exit status (128+N for a signal, as a shell would), and returns
as soon as the command exits (`Popen.wait(timeout)` wakes on exit; the poll
interval only bounds how often we LOOK).

Knobs:
  NROS_JOBSERVER_STALL_SECS       seconds of "no live child, no CPU" before the
                                  watchdog fires. Default 600. 0 disables it —
                                  the wrapper then execs the command in place.
  NROS_JOBSERVER_STALL_POLL_SECS  sampling interval. Default 15.

Usage:
  make-stall-watchdog.py --label <name> --diag-dir <dir> -- <command> [args...]
"""

import argparse
import datetime
import os
import select
import shutil
import signal
import stat
import subprocess
import sys
import time

# EX_TEMPFAIL. Distinct from make's own 1/2, the shell's 126/127, `timeout`'s
# 124 and every 128+N signal status, so a caller can tell "this lane never
# answered" from "this lane answered no" by the number alone.
NO_VERDICT_RC = 75
MARKER = "NO VERDICT: jobserver stall"

# The status lines that bear on "blocked with no child": run state, threads,
# and the signal masks — a blocked or ignored SIGCHLD is one way a make stops
# hearing about its children.
STATUS_KEYS = ("Name", "State", "PPid", "Threads", "SigPnd", "ShdPnd", "SigBlk",
               "SigIgn", "SigCgt", "voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")

DEFAULT_STALL_SECS = 600
DEFAULT_POLL_SECS = 15


def _env_seconds(name, default):
    raw = os.environ.get(name, "")
    if raw == "":
        return float(default)
    try:
        v = float(raw)
    except ValueError:
        sys.stderr.write(f"make-stall-watchdog: {name}={raw!r} is not a number of seconds\n")
        raise SystemExit(2)
    if v < 0:
        sys.stderr.write(f"make-stall-watchdog: {name}={raw!r} must be >= 0\n")
        raise SystemExit(2)
    return v


def _read(path, binary=False):
    """(content, None) or (None, reason) — never raises."""
    try:
        with open(path, "rb") as fh:
            data = fh.read()
    except OSError as exc:
        return None, f"{type(exc).__name__}: {exc.strerror or exc}"
    if binary:
        return data, None
    return data.decode("utf-8", "replace"), None


def _proc_stat(pid):
    """(ppid, state, cpu_ticks) from /proc/<pid>/stat, or None."""
    data, _ = _read(f"/proc/{pid}/stat")
    if not data:
        return None
    # comm may contain spaces and parens; everything after the LAST ')' is fixed.
    rest = data[data.rfind(")") + 2:].split()
    try:
        return int(rest[1]), rest[0], int(rest[11]) + int(rest[12])
    except (IndexError, ValueError):
        return None


def _all_pids():
    return [int(d) for d in os.listdir("/proc") if d.isdigit()]


def _children_map():
    kids = {}
    for pid in _all_pids():
        st = _proc_stat(pid)
        if st:
            kids.setdefault(st[0], []).append((pid, st[1]))
    return kids


def _descendants(root, kids=None):
    kids = _children_map() if kids is None else kids
    out, todo = [], [root]
    while todo:
        p = todo.pop()
        for c, _state in kids.get(p, []):
            out.append(c)
            todo.append(c)
    return out


def _cmdline(pid):
    data, err = _read(f"/proc/{pid}/cmdline", binary=True)
    if data is None:
        return None, err
    return " ".join(a.decode("utf-8", "replace") for a in data.split(b"\0") if a), None


def _jobserver_auth(text):
    """Every `--jobserver-auth=...` / `--jobserver-fds=...` value in `text`."""
    out = []
    for tok in text.replace("\0", " ").split():
        for key in ("--jobserver-auth=", "--jobserver-fds="):
            if tok.startswith(key):
                out.append(tok[len(key):])
    return out


def _pipe_bytes(path):
    """Bytes queued in the pipe/FIFO at `path` (a /proc fd link), via FIONREAD."""
    import fcntl
    import struct
    import termios
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    except OSError as exc:
        return f"unreadable ({exc.strerror})"
    try:
        buf = fcntl.ioctl(fd, termios.FIONREAD, struct.pack("i", 0))
        return str(struct.unpack("i", buf)[0])
    except OSError as exc:
        return f"FIONREAD failed ({exc.strerror})"
    finally:
        os.close(fd)


def _pipe_fds(pid):
    """[(fd, link, key)] for every pipe/FIFO fd of `pid`; key identifies the
    pipe object across processes (inode). Plus the reason fds were unreadable."""
    out = []
    try:
        fds = os.listdir(f"/proc/{pid}/fd")
    except OSError as exc:
        return out, f"{type(exc).__name__}: {exc.strerror}"
    for fd in fds:
        p = f"/proc/{pid}/fd/{fd}"
        try:
            link = os.readlink(p)
            st = os.stat(p)
        except OSError:
            continue
        if stat.S_ISFIFO(st.st_mode):
            out.append((int(fd), link, (st.st_dev, st.st_ino)))
    return out, None


def collect_diagnostics(pid, label, cmd, idle_secs, stall_secs):
    lines = []
    w = lines.append
    now = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    w(f"== jobserver stall diagnostics (issue 1403) — {label} ==")
    w(f"time:           {now}")
    w(f"make pid:       {pid}")
    w(f"command:        {' '.join(cmd)}")
    w(f"idle for:       {idle_secs:.0f}s with no live child and no CPU "
      f"(threshold NROS_JOBSERVER_STALL_SECS={stall_secs:g})")
    w("")

    kids = _children_map()
    zombies = [c for c, s in kids.get(pid, []) if s == "Z"]
    w(f"live children:  {[c for c, s in kids.get(pid, []) if s != 'Z']}")
    w(f"zombie children (not yet reaped by make): {zombies}")
    w("")

    w("-- make's kernel state --")
    for name in ("stack", "wchan", "syscall", "status"):
        text, err = _read(f"/proc/{pid}/{name}")
        if text is not None and name == "status":
            text = "\n".join(l for l in text.split("\n") if l.split(":")[0] in STATUS_KEYS)
        if text is None:
            w(f"/proc/{pid}/{name}: NOT READABLE — {err}")
        else:
            w(f"/proc/{pid}/{name}:")
            for l in (text.rstrip("\n").split("\n") if text.strip() else ["<empty>"]):
                w(f"    {l}")
    w("")

    w("-- make's file descriptors --")
    try:
        for fd in sorted(os.listdir(f"/proc/{pid}/fd"), key=int):
            try:
                link = os.readlink(f"/proc/{pid}/fd/{fd}")
            except OSError as exc:
                link = f"<{exc.strerror}>"
            info, _ = _read(f"/proc/{pid}/fdinfo/{fd}")
            flags = ""
            if info:
                for l in info.split("\n"):
                    if l.startswith("flags:"):
                        flags = l.split()[1]
            w(f"    fd {fd:>3} -> {link}  flags={flags}")
    except OSError as exc:
        w(f"    NOT READABLE — {exc.strerror}")
    w("")

    w("-- jobserver --")
    env, env_err = _read(f"/proc/{pid}/environ", binary=True)
    cmdline, _ = _cmdline(pid)
    inherited = []
    if env is not None:
        for var in env.split(b"\0"):
            if var.startswith((b"MAKEFLAGS=", b"CARGO_MAKEFLAGS=", b"MFLAGS=")):
                inherited += _jobserver_auth(var.decode("utf-8", "replace"))
    else:
        w(f"make environ: NOT READABLE — {env_err}")
    inherited += _jobserver_auth(cmdline or "")
    w(f"jobserver inherited from a parent (MAKEFLAGS / argv): {inherited or 'none — this make OWNS its jobserver'}")
    jobs = None
    for tok in (cmdline or "").split():
        if tok.startswith("-j") and tok[2:].isdigit():
            jobs = int(tok[2:])
    make_pipes, err = _pipe_fds(pid)
    # fds 0-2 are stdio, often pipes to a logger; the jobserver is never there.
    # A named FIFO (make 4.4 `--jobserver-style=fifo`) is certainly it; an
    # anonymous pipe at fd >= 3 may be (the pre-4.4 style) or may be make's own
    # signal pipe, and is labelled so rather than guessed.
    make_pipes = [(fd, link, key) for fd, link, key in make_pipes if fd >= 3]
    if err:
        w(f"make pipe/FIFO fds: NOT READABLE — {err}")
    keys = {}
    for fd, link, key in make_pipes:
        keys.setdefault(key, []).append(fd)
        kind = "anonymous pipe (maybe)" if link.startswith("pipe:") else "named FIFO"
        w(f"    fd {fd} -> {link}  [{kind}]  bytes-available(FIONREAD)={_pipe_bytes(f'/proc/{pid}/fd/{fd}')}")
        if link.startswith("/") and not link.endswith("(deleted)"):
            try:
                w(f"        ls -l: {subprocess.run(['ls', '-l', link], capture_output=True, text=True, timeout=5).stdout.strip()}")
            except (OSError, subprocess.TimeoutExpired):
                pass
    if jobs:
        w(f"-j{jobs}: an idle top-level make holds {jobs - 1} token byte(s) in its FIFO; "
          "fewer means a token is held (or was lost) somewhere")
    w("")

    w("-- other processes holding a jobserver --")
    mine = set(_descendants(pid, kids)) | {pid}
    unreadable_fd = unreadable_env = 0
    found = 0
    for other in sorted(_all_pids()):
        if other == pid or other == os.getpid():
            continue
        reasons = []
        cl, _ = _cmdline(other)
        if cl is None:
            continue  # exited during the scan
        if _jobserver_auth(cl):
            reasons.append(f"argv jobserver {_jobserver_auth(cl)}")
        oenv, _ = _read(f"/proc/{other}/environ", binary=True)
        if oenv is None:
            unreadable_env += 1
        else:
            for var in oenv.split(b"\0"):
                if var.startswith((b"MAKEFLAGS=", b"CARGO_MAKEFLAGS=")):
                    auth = _jobserver_auth(var.decode("utf-8", "replace"))
                    if auth:
                        reasons.append(f"{var.split(b'=')[0].decode()} jobserver {auth}")
        opipes, oerr = _pipe_fds(other)
        if oerr:
            unreadable_fd += 1
        for fd, link, key in opipes:
            if key in keys:
                reasons.append(f"fd {fd} -> {link} is one of THIS make's pipe/FIFO fds (>= 3)")
            elif "GMfifo" in link:
                reasons.append(f"fd {fd} -> {link} (a make FIFO)")
        if reasons:
            found += 1
            st = _proc_stat(other)
            where = "descendant of this make" if other in mine else "NOT in this make's tree"
            w(f"    pid {other} ppid {st[0] if st else '?'} state {st[1] if st else '?'} "
              f"[{where}]: {cl[:200]}")
            for r in reasons:
                w(f"        {r}")
    w(f"    {found} process(es) found; environ unreadable for {unreadable_env}, "
      f"fds unreadable for {unreadable_fd} (other users' processes — not scanned)")
    w("")

    w("-- gdb backtrace --")
    w(_gdb_backtrace(pid))
    return "\n".join(lines) + "\n"


def _gdb_backtrace(pid):
    gdb = shutil.which("gdb")
    if not gdb:
        return "skipped: gdb not on PATH"
    scope, _ = _read("/proc/sys/kernel/yama/ptrace_scope")
    scope = (scope or "").strip()
    # gdb runs as OUR child, so it is not an ancestor of make: yama scope >= 1
    # refuses the attach. Say so instead of spending a timeout proving it.
    if scope not in ("", "0"):
        return (f"skipped: kernel.yama.ptrace_scope={scope} refuses a non-ancestor attach "
                "(never escalated). Re-run the build with ptrace_scope=0, or as root:\n"
                f"    gdb -p <make pid> -batch -ex 'thread apply all bt'")
    try:
        r = subprocess.run([gdb, "-p", str(pid), "-batch", "-nx",
                            "-ex", "thread apply all bt"],
                           capture_output=True, text=True, timeout=60,
                           stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return "gdb timed out after 60s"
    except OSError as exc:
        return f"gdb failed to start: {exc}"
    return (r.stdout + r.stderr).strip() or f"gdb produced no output (rc {r.returncode})"


def _say(text):
    try:
        sys.stderr.write(text)
        sys.stderr.flush()
    except (OSError, ValueError):
        pass


def kill_tree(proc):
    """TERM then KILL `proc` and its descendants, found by ppid from its pid."""
    victims = [proc.pid] + _descendants(proc.pid)
    for p in victims:
        try:
            os.kill(p, signal.SIGTERM)
        except OSError:
            pass
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    for p in [proc.pid] + _descendants(proc.pid) + victims:
        try:
            os.kill(p, signal.SIGKILL)
        except OSError:
            pass
    proc.wait()


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True)
    ap.add_argument("--diag-dir", required=True)
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    args = ap.parse_args(argv)
    cmd = args.cmd[1:] if args.cmd[:1] == ["--"] else args.cmd
    if not cmd:
        ap.error("no command given after --")

    stall = _env_seconds("NROS_JOBSERVER_STALL_SECS", DEFAULT_STALL_SECS)
    if stall == 0:
        os.execvp(cmd[0], cmd)
    poll = min(_env_seconds("NROS_JOBSERVER_STALL_POLL_SECS", DEFAULT_POLL_SECS), stall) or 1.0

    proc = subprocess.Popen(cmd)

    # Forward a signal aimed at the wrapper alone. When the whole group is
    # signalled (subtree-guard, a terminal ^C) make gets it directly too; a
    # duplicate is harmless because make blocks the signal inside its handler.
    def forward(signum, _frame):
        try:
            os.kill(proc.pid, signum)
        except OSError:
            pass
    for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(s, forward)

    # A pidfd makes the wait a real block that wakes on exit. `Popen.wait(t)`
    # is a sleep-poll loop (~20 wakeups/s) — harmless, but over a multi-hour
    # build it is the busy loop this wrapper promised not to be.
    try:
        pidfd = os.pidfd_open(proc.pid)
    except (AttributeError, OSError):
        pidfd = None

    idle_since = None
    last_cpu = None
    while True:
        if pidfd is not None:
            select.select([pidfd], [], [], poll)
            rc = proc.poll()
        else:
            try:
                rc = proc.wait(timeout=poll)
            except subprocess.TimeoutExpired:
                rc = None
        if rc is not None:
            return rc if rc >= 0 else 128 - rc
        st = _proc_stat(proc.pid)
        if st is None:
            continue  # exited between wait and sample; the next wait reaps it
        live = [c for c, s in _children_map().get(proc.pid, []) if s != "Z"]
        cpu = st[2]
        now = time.monotonic()
        if live or cpu != last_cpu:
            idle_since = None if live else now
            last_cpu = cpu
            continue
        if idle_since is None:
            idle_since = now
        if now - idle_since < stall:
            continue

        report = collect_diagnostics(proc.pid, args.label, cmd, now - idle_since, stall)
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        path = os.path.abspath(os.path.join(args.diag_dir, f"jobserver-stall-{proc.pid}-{stamp}.txt"))
        try:
            os.makedirs(args.diag_dir, exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(report)
        except OSError as exc:
            path = f"<not written: {exc}; the report is on stderr above>"
        # Kill BEFORE talking: a closed stderr (a `| head`, a dead log pipe)
        # must not be able to stop the kill and leave the stall standing.
        kill_tree(proc)
        _say(report)
        _say(f"make-stall-watchdog: killed pid {proc.pid} and its descendants (nothing else)\n")
        _say(f"{MARKER} — {path}\n")
        _say("  make sat idle with no child and no progress; the build did NOT fail and did "
             "NOT pass. See docs/issues/1403-fixture-jobserver-deadlock-looks-like-a-hang.md\n")
        return NO_VERDICT_RC


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
