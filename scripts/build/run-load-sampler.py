#!/usr/bin/env python3
"""Runnable-process samples for one gate lane, and the report on them.

phase-489 W13. A gate lane runs under ONE token pool (`nros_pool_own`,
scripts/build/jobserver-pool.sh), and every jobserver client in it draws from
that pool. A child that ignores the pool (a direct `cc -j`, a make below 4.4,
a QEMU, a Python pool, a ninja handed an explicit `-j`) is not bounded by it.
That is accepted and made VISIBLE, never hidden: this samples the host's
runnable processes while the lane runs and, afterwards, reports the peak
against the pool's size, and names each gate whose processes peaked above its
declared weight (`[group('cpu:<n>')]`, default 1), so it can be given a
resource group or taught the pool.

Which processes are the lane's: those whose `NROS_GATE_RUN` equals the
sampler's own (gate-lane.sh exports one id per run). `NROS_GATE_LANE` alone
cannot say it: it is the lane NAME, `dev` in every worktree, so another
worktree's pre-push gates counted as this lane's (issue 2683). Without a run
id the old rule stands: anything carrying `NROS_GATE_LANE`.

WAITS (issue 2683, opt-in: `NROS_GATE_WAITS=1` on `just ci gate` or any gate
lane). Runnable counts say how busy a gate was, not why it was slow. With
WAITS on, each sample also classifies every one of the lane's THREADS by its
kernel wait channel (`/proc/<pid>/task/<tid>/wchan`; Yama's ptrace scope hides
`syscall` and fd links, so wchan is the view a same-user process has):
`run`, `pipe` (a jobserver token or a parent reading a child's output),
`child` (waiting for a child), `lock` (a file lock: cargo's build-dir or
package-cache lock), `disk` (page and write-back waits) or `other`, into
`<file>.waits`, and the report adds a per-gate table of thread-seconds in
each. Measured with it, three green step-8 runs of `just ci gate` on a 32-CPU
host waited on cargo's lock far more than they ran, never starved for tokens,
and differed mostly in disk waits. A red run with WAITS on is what tells a
starved pool from a lock queue from a busy disk.

ABOVE THE POOL (issue 2625). A gate's peak above its weight is ordinary: its
cargo holds pool tokens, so the peak measures the host and the moment as much
as the gate (one gate measured 2 to 54 across nine runs on one host). A peak
above the whole POOL is different: the pool cannot have granted it, so some
child ran outside the jobserver. With `--pool <n>`, each sample in which a gate
runs more threads than that records the gate's top commands into
`<file>.over`, and the report names them. Reported, not failed: whether it
happens depends on timing, so a failure on it would be a new flake.

Usage:
  run-load-sampler.py sample <file> [--interval S] [--pool N]
                                                     append samples until killed
  run-load-sampler.py report <file> <tokens> [<from> <to>]
                                                     print the summary, over the
                                                     samples in [from, to] epoch
                                                     seconds when given
  run-load-sampler.py --self-test

A sample is `<epoch> <runnable> #lane=<n> [#free=<n>] <owner>=<n>...`, counting runnable
THREADS. `runnable` is `procs_running` from /proc/stat, the measure the W13
baseline used, and it is HOST-WIDE: another tenant's threads are in it (a
CARLA simulation shared newslab-118 on 2026-11-15). `#lane` is the threads of
processes this lane runs (`NROS_GATE_LANE` in their environment, exported by
gate-lane.sh), the number the pool can answer for. A gate test's count is the
threads whose environment names it (`NROS_GATE_TEST`, set by the gate
harness). `#free` is the tokens unclaimed in the lane's FIFO pool (FIONREAD),
which says whether a low runnable count is the pool starving the lane (none
free) or the work itself (tokens free).
"""

import importlib.util
import os
import signal
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def procs_running():
    with open("/proc/stat") as f:
        for line in f:
            if line.startswith("procs_running "):
                return int(line.split()[1])
    return 0


def fifo_tokens(path):
    """Tokens sitting unclaimed in a FIFO jobserver: the bytes in the pipe
    (FIONREAD). Opening a read end and closing it takes none. None when the
    pool is gone or this is not a FIFO."""
    import fcntl
    import struct
    import termios
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    except OSError:
        return None
    try:
        buf = fcntl.ioctl(fd, termios.FIONREAD, b"\0\0\0\0")
        return struct.unpack("i", buf)[0]
    except OSError:
        return None
    finally:
        os.close(fd)


def pool_fifo(pid):
    """The FIFO jobserver a process was handed (`--jobserver-auth=fifo:`)."""
    try:
        env = Path(f"/proc/{pid}/environ").read_bytes()
    except OSError:
        return None
    for kv in env.split(b"\0"):
        if kv.startswith(b"MAKEFLAGS=") and b"--jobserver-auth=fifo:" in kv:
            return kv.split(b"--jobserver-auth=fifo:", 1)[1].split(b" ", 1)[0].decode()
    return None


RUN_ID = os.environ.get("NROS_GATE_RUN", "").encode()


def owner_from_env(env, run_id):
    """(in_lane, gate test or None) from a process's environment block.

    With a run id, a process is the lane's only when it carries the SAME
    `NROS_GATE_RUN`: the lane NAME is shared by every worktree's lane. A gate
    test name outside this run is another lane's, never this one's gate."""
    lane, name, run = False, None, None
    for kv in env.split(b"\0"):
        if kv.startswith(b"NROS_GATE_TEST="):
            name = kv[len(b"NROS_GATE_TEST="):].decode(errors="replace")
        elif kv.startswith(b"NROS_GATE_LANE="):
            lane = True
        elif kv.startswith(b"NROS_GATE_RUN="):
            run = kv[len(b"NROS_GATE_RUN="):]
    if run_id:
        return (run == run_id, name if run == run_id else None)
    return (lane or name is not None, name)


def owner_of(pid, cache):
    """(in_lane, gate test or None) for a process, from its environment:
    `NROS_GATE_RUN`/`NROS_GATE_LANE` are exported by gate-lane.sh to
    everything the lane runs, `NROS_GATE_TEST` by the gate harness to
    everything one test runs."""
    if pid in cache:
        return cache[pid]
    try:
        env = Path(f"/proc/{pid}/environ").read_bytes()
    except OSError:
        env = b""
    cache[pid] = owner_from_env(env, RUN_ID)
    return cache[pid]


def comm_of(pid):
    try:
        return Path(f"/proc/{pid}/comm").read_text().strip().replace(" ", "_") or "?"
    except OSError:
        return "?"


def runnable_by_owner(cache, comms=None):
    """(lane, counts): runnable THREADS (what `procs_running` counts) that the
    lane owns, and per owner: a gate test, or `?<comm>` for one no gate test
    owns (`!<comm>` when it is not the lane's at all, e.g. another tenant).
    With `comms`, also each gate's runnable threads by command."""
    counts, lane = {}, 0
    fifo = cache.get("fifo")
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            tids = os.listdir(f"/proc/{d}/task")
        except OSError:
            continue
        n = 0
        for t in tids:
            try:
                st = Path(f"/proc/{d}/task/{t}/stat").read_text()
            except OSError:
                continue
            if st[st.rindex(")") + 2:].split(" ", 1)[0] == "R":
                n += 1
        if not n:
            continue
        in_lane, g = owner_of(int(d), cache)
        if in_lane:
            lane += n
            if fifo is None:
                # Read fresh, not from the owner cache: a pid's MAKEFLAGS names
                # the pool it was started under.
                fifo = pool_fifo(int(d))
                if fifo and fifo_tokens(fifo) is not None:
                    cache["fifo"] = fifo
                else:
                    fifo = None
        key = g or ("?" if in_lane else "!") + comm_of(d)
        counts[key] = counts.get(key, 0) + n
        if comms is not None and g:
            per = comms.setdefault(g, {})
            c = comm_of(d)
            per[c] = per.get(c, 0) + n
    free = fifo_tokens(fifo) if fifo else None
    if free is None and fifo:
        # That pool is gone (one lane step's pool ended and the next opened
        # its own): find the current one on the next sample.
        cache.pop("fifo", None)
    if free is not None:
        counts["#free"] = free
    return lane, counts


# Kernel wait channels by what a thread is waiting for. A substring rule, so a
# kernel that renames a helper still lands in the right class; `other` is
# reported, never dropped.
DISK_WCHANS = ("folio_wait", "balance_dirty_pages", "wait_on_buffer",
               "wait_on_page", "io_schedule", "rq_qos_wait", "blk_mq_get_tag",
               "jbd2", "wb_wait")


def wait_kind(state, wchan):
    """What one thread is doing, from its /proc state letter and wchan."""
    if state == "R":
        return "run"
    w = wchan or ""
    if "pipe" in w:
        return "pipe"
    if w in ("do_wait", "kernel_wait4", "do_waitid"):
        return "child"
    if "locks_lock" in w or "flock" in w or "fcntl_setlk" in w:
        return "lock"
    if any(d in w for d in DISK_WCHANS) or state == "D":
        return "disk"
    return "other"


WAIT_KINDS = ("run", "pipe", "child", "lock", "disk", "other")


def waits_by_gate(cache):
    """{gate test: {kind: threads}} for this lane's threads, one sample."""
    out = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        in_lane, g = owner_of(int(d), cache)
        if not in_lane:
            continue
        try:
            tids = os.listdir(f"/proc/{d}/task")
        except OSError:
            continue
        per = out.setdefault(g or "-", {})
        for t in tids:
            try:
                st = Path(f"/proc/{d}/task/{t}/stat").read_text()
                wchan = Path(f"/proc/{d}/task/{t}/wchan").read_text().strip()
            except OSError:
                continue
            k = wait_kind(st[st.rindex(")") + 2:].split(" ", 1)[0], wchan)
            per[k] = per.get(k, 0) + 1
    return out


def above_pool_line(now, counts, comms, pool):
    """`.over` lines: each gate test running more threads than the whole pool,
    with its top commands."""
    out = []
    for g, n in sorted(counts.items()):
        if g[0] in "#?!" or n <= pool:
            continue
        top = sorted(comms.get(g, {}).items(), key=lambda x: (-x[1], x[0]))[:6]
        out.append(f"{now} {g} {n} " + " ".join(f"{c}={k}" for c, k in top))
    return out


def summarize_over(lines):
    """gate -> (peak threads, {command: largest count seen}) from `.over`."""
    per = {}
    for line in lines:
        f = line.split()
        if len(f) < 3 or not f[0].isdigit() or not f[2].isdigit():
            continue
        peak, cs = per.get(f[1], (0, {}))
        for kv in f[3:]:
            c, _, k = kv.rpartition("=")
            if c and k.isdigit():
                cs[c] = max(cs.get(c, 0), int(k))
        per[f[1]] = (max(peak, int(f[2])), cs)
    return per


def sample(path, interval, pool=None):
    stop = []
    signal.signal(signal.SIGTERM, lambda *_: stop.append(1))
    cache = {}
    waits = os.environ.get("NROS_GATE_WAITS") == "1"
    wout = open(f"{path}.waits", "a") if waits else None
    oout = open(f"{path}.over", "a") if pool else None
    with open(path, "a") as out:
        while not stop:
            comms = {} if oout else None
            lane, counts = runnable_by_owner(cache, comms)
            now = str(int(time.time()))
            fields = [now, str(procs_running()), f"#lane={lane}"]
            fields += [f"{g}={n}" for g, n in sorted(counts.items())]
            out.write(" ".join(fields) + "\n")
            out.flush()
            if oout:
                for ln in above_pool_line(now, counts, comms, pool):
                    oout.write(ln + "\n")
                oout.flush()
            if wout:
                for g, per in sorted(waits_by_gate(cache).items()):
                    wout.write(f"{now} {g} " + " ".join(
                        f"{k}={per[k]}" for k in WAIT_KINDS if per.get(k)) + "\n")
                wout.flush()
            if len(cache) > 200000:
                cache.clear()
            for _ in range(int(interval * 10)):
                if stop:
                    break
                time.sleep(0.1)
    return 0


def pct(xs, p):
    if not xs:
        return 0
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))]


def weights():
    """Gate recipe -> declared cpu weight, from the derivation the nextest
    config is generated by (`check-gate-lists.py`)."""
    spec = importlib.util.spec_from_file_location(
        "check_gate_lists", ROOT / "scripts/check/check-gate-lists.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    out = {}
    for name, groups in mod.recipe_groups().items():
        cpus = [int(g[4:]) for g in groups if g.startswith("cpu:") and g[4:].isdigit()]
        out[name] = max(cpus) if cpus else 1
    return out


def recipe_of(test):
    # `<tier>::check::<recipe>[#unit]`
    rest = test.split("::", 1)[-1].removeprefix("check::")
    return rest.split("#", 1)[0]


def summarize(lines, tokens, weight_of):
    """(totals, over, peak): `peak` is the busiest sample's composition, as
    (owner, count) pairs, largest first; an owner is a gate test or `?<comm>`."""
    totals, lanes, frees, peaks, peak, peak_total = [], [], [], {}, [], -1
    for line in lines:
        f = line.split()
        if len(f) < 2 or not f[1].isdigit():
            continue
        totals.append(int(f[1]))
        here, lane = [], None
        for kv in f[2:]:
            g, _, n = kv.rpartition("=")
            if not (g and n.isdigit()):
                continue
            if g == "#lane":
                lane = int(n)
                continue
            if g == "#free":
                frees.append(int(n))
                continue
            here.append((g, int(n)))
            if g[0] not in "?!":
                peaks[g] = max(peaks.get(g, 0), int(n))
        if lane is not None:
            lanes.append(lane)
        if int(f[1]) > peak_total:
            peak_total, peak = int(f[1]), sorted(here, key=lambda x: -x[1])
    over = sorted(((n - weight_of(recipe_of(g)), g, n, weight_of(recipe_of(g)))
                   for g, n in peaks.items() if n > weight_of(recipe_of(g))), reverse=True)
    return totals, lanes, over, peak, frees


def summarize_waits(lines):
    """({gate: {kind: thread-samples}}, interval s): the per-gate wait table."""
    per, stamps = {}, []
    for line in lines:
        f = line.split()
        if len(f) < 3 or not f[0].isdigit():
            continue
        if not stamps or stamps[-1] != int(f[0]):
            stamps.append(int(f[0]))
        row = per.setdefault(f[1], {})
        for kv in f[2:]:
            k, _, n = kv.partition("=")
            if k in WAIT_KINDS and n.isdigit():
                row[k] = row.get(k, 0) + int(n)
    gaps = [b - a for a, b in zip(stamps, stamps[1:]) if b > a]
    return per, (pct(gaps, 50) if gaps else 0)


def report_waits(path, window=None):
    try:
        lines = Path(f"{path}.waits").read_text().splitlines()
    except OSError:
        return
    if window:
        lines = [ln for ln in lines if ln.split(" ", 1)[0].isdigit()
                 and window[0] <= int(ln.split(" ", 1)[0]) <= window[1]]
    per, step = summarize_waits(lines)
    if not per:
        return
    step = step or 1
    # Ordered by what a gate waited on that is not its own children (a wrapper
    # waiting for its child says nothing): tokens, locks, disk.
    rows = sorted(per.items(), key=lambda kv: -sum(kv[1].get(k, 0)
                                                   for k in ("pipe", "lock", "disk")))
    print(f"waits (NROS_GATE_WAITS): thread-seconds per gate, sampled every ~{step} s "
          "(run: on a CPU; pipe: a jobserver token or a child's output; child: a "
          "child to exit; lock: a file lock; disk: page or write-back I/O):")
    print(f"  {'gate':58s} " + " ".join(f"{k:>7s}" for k in WAIT_KINDS[:5]))
    for g, row in rows[:15]:
        print(f"  {g[:58]:58s} " + " ".join(f"{row.get(k, 0) * step:7d}" for k in WAIT_KINDS[:5]))


def report(path, tokens, window=None):
    try:
        lines = Path(path).read_text().splitlines()
        if window:
            lines = [ln for ln in lines if ln.split(" ", 1)[0].isdigit()
                     and window[0] <= int(ln.split(" ", 1)[0]) <= window[1]]
    except OSError:
        print("token pool: no load samples were taken")
        return 0
    w = weights()
    totals, lanes, over, peak, frees = summarize(lines, tokens, lambda r: w.get(r, 1))
    if not totals:
        print("token pool: no load samples were taken")
        return 0
    if lanes:
        print(f"token pool: this lane's runnable threads over {len(lanes)} samples: "
              f"p50 {pct(lanes, 50)}, p90 {pct(lanes, 90)}, max {max(lanes)}, "
              f"against a pool of {tokens}")
    if frees:
        empty = sum(1 for f in frees if f == 0)
        print(f"token pool: free tokens over {len(frees)} samples: p50 {pct(frees, 50)}, "
              f"p10 {pct(frees, 10)}, min {min(frees)}; empty in {empty} "
              f"({100 * empty // len(frees)} %). Free tokens with the lane's runnable count "
              "low: the work is not CPU-bound; none free with it low: tokens are held "
              "or lost")
    print(f"token pool: host runnable over {len(totals)} samples: p50 {pct(totals, 50)}, "
          f"p90 {pct(totals, 90)}, max {max(totals)} (procs_running: every tenant's "
          "threads, this lane's included)")
    if over:
        print(f"token pool: {len(over)} gate test(s) ran more processes than their weight "
              "(a child outside the pool, or a weight to raise; phase-489 W13):")
        for excess, g, n, wt in over[:15]:
            print(f"  {g}: peak {n} runnable, weight {wt}")
        if len(over) > 15:
            print(f"  ... and {len(over) - 15} more")
    if peak:
        print(f"token pool: the busiest sample ({max(totals)} runnable), by owner "
              "(a gate test; `?<command>`: the lane's, no gate test's; "
              "`!<command>`: not this lane's):")
        print("  " + ", ".join(f"{g} {n}" for g, n in peak[:12]))
    try:
        olines = Path(f"{path}.over").read_text().splitlines()
    except OSError:
        olines = []
    if window:
        olines = [ln for ln in olines if ln.split(" ", 1)[0].isdigit()
                  and window[0] <= int(ln.split(" ", 1)[0]) <= window[1]]
    above = summarize_over(olines)
    if above:
        print(f"token pool: {len(above)} gate test(s) ran more threads than the whole pool of "
              f"{tokens}, which it cannot have granted: some child ran outside the "
              "jobserver (top commands at those moments):")
        for g, (n, cs) in sorted(above.items(), key=lambda x: -x[1][0]):
            top = sorted(cs.items(), key=lambda x: (-x[1], x[0]))[:6]
            print(f"  {g}: peak {n}; " + ", ".join(f"{c} {k}" for c, k in top))
    report_waits(path, window)
    return 0


def self_test():
    bad = 0

    def case(name, ok):
        nonlocal bad
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
        bad += not ok

    lines = ["1 10 build::check::linux-check=8 fast::check::a=1",
             "2 30 build::check::linux-check=3 build::check::scaffold-builds#x=5",
             "junk"]
    w = {"linux-check": 4, "scaffold-builds": 1}
    lines.append("3 40 #lane=32 #free=5 ?cc1=30 !CarlaUE4=6 build::check::linux-check=2")
    totals, lanes, over, peak, frees = summarize(lines, 8, lambda r: w.get(r, 1))
    case("free tokens are their own column, never an owner",
         frees == [5] and all(g != "#free" for g, _ in peak))
    case("totals are the runnable column", totals == [10, 30, 40])
    case("the lane's own count is kept apart from the host's", lanes == [32])
    case("another tenant's process is never named as a gate",
         all(not g.startswith("!") for _, g, _, _ in over))
    case("the busiest sample is broken down by owner, largest first",
         peak == [("?cc1", 30), ("!CarlaUE4", 6), ("build::check::linux-check", 2)])
    case("a process no gate owns is never named as a gate over its weight",
         all(not g.startswith("?") for _, g, _, _ in over))
    case("a gate above its weight is named with its peak",
         (4, "build::check::linux-check", 8, 4) in over)
    case("a unit is weighed by its recipe", (4, "build::check::scaffold-builds#x", 5, 1) in over)
    # Negative control: a gate at its weight is not named.
    case("negative control: a gate within its weight is not named",
         all(g != "fast::check::a" for _, g, _, _ in over))
    case("percentiles", pct([1, 2, 3, 4, 100], 50) == 3 and pct([5], 90) == 5)
    # issue 2683: attribution by run id, not by the shared lane name.
    mine = b"NROS_GATE_LANE=dev\0NROS_GATE_RUN=r1\0NROS_GATE_TEST=build::check::x"
    other = b"NROS_GATE_LANE=dev\0NROS_GATE_RUN=r2\0NROS_GATE_TEST=fast::check::y"
    case("a process of this run is the lane's, with its gate",
         owner_from_env(mine, b"r1") == (True, "build::check::x"))
    case("another worktree's lane of the same name is not this lane's",
         owner_from_env(other, b"r1") == (False, None))
    case("without a run id the lane name decides, as before",
         owner_from_env(other, b"") == (True, "fast::check::y"))
    # issue 2683: what a thread waits on, by wchan.
    kinds = [wait_kind("R", "0"), wait_kind("S", "anon_pipe_read"), wait_kind("S", "do_wait"),
             wait_kind("S", "locks_lock_inode_wait"), wait_kind("D", "folio_wait_bit_common"),
             wait_kind("S", "balance_dirty_pages"), wait_kind("S", "futex_wait_queue"),
             wait_kind("S", "lock_mm_and_find_vma")]
    case("wait channels classify", kinds == ["run", "pipe", "child", "lock", "disk",
                                             "disk", "other", "other"])
    per, step = summarize_waits(["10 build::check::p run=2 lock=5",
                                 "15 build::check::p lock=3 disk=1",
                                 "15 - child=4", "junk"])
    case("the wait table sums a gate's samples and reads the interval",
         per["build::check::p"] == {"run": 2, "lock": 8, "disk": 1} and step == 5)
    # issue 2625: a gate above the whole pool, named with its commands.
    over_lines = above_pool_line("7", {"build::check::wf": 40, "build::check::ok": 32,
                                       "?cc1": 50, "#free": 0},
                                 {"build::check::wf": {"ld.lld": 30, "rustc": 10},
                                  "build::check::ok": {"rustc": 32}}, 32)
    case("a gate above the pool is recorded with its top commands",
         over_lines == ["7 build::check::wf 40 ld.lld=30 rustc=10"])
    case("negative control: a gate AT the pool, or no gate's process, is not recorded",
         not any("build::check::ok" in ln or "cc1" in ln for ln in over_lines))
    agg = summarize_over(over_lines + ["9 build::check::wf 36 ld.lld=34 ninja=2", "junk"])
    case("above-pool samples fold to the peak and each command's largest count",
         agg == {"build::check::wf": (40, {"ld.lld": 34, "rustc": 10, "ninja": 2})})
    if bad:
        print("run-load-sampler self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if len(argv) >= 2 and argv[0] == "sample":
        interval, pool, rest = 30.0, None, argv[2:]
        while rest:
            if rest[0] == "--interval" and len(rest) > 1:
                interval, rest = float(rest[1]), rest[2:]
            elif rest[0] == "--pool" and len(rest) > 1 and rest[1].isdigit():
                pool, rest = int(rest[1]), rest[2:]
            else:
                print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
                return 2
        return sample(argv[1], interval, pool)
    if len(argv) == 3 and argv[0] == "report" and argv[2].isdigit():
        return report(argv[1], int(argv[2]))
    if len(argv) == 5 and argv[0] == "report" and all(a.isdigit() for a in argv[2:]):
        return report(argv[1], int(argv[2]), (int(argv[3]), int(argv[4])))
    print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
