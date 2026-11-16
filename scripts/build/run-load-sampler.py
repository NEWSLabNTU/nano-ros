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

Usage:
  run-load-sampler.py sample <file> [--interval S]   append samples until killed
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


def owner_of(pid, cache):
    """(in_lane, gate test or None) for a process, from its environment:
    `NROS_GATE_LANE` is exported by gate-lane.sh to everything the lane runs,
    `NROS_GATE_TEST` by the gate harness to everything one test runs."""
    if pid in cache:
        return cache[pid]
    lane, name = False, None
    try:
        env = Path(f"/proc/{pid}/environ").read_bytes()
    except OSError:
        env = b""
    for kv in env.split(b"\0"):
        if kv.startswith(b"NROS_GATE_TEST="):
            name = kv[len(b"NROS_GATE_TEST="):].decode(errors="replace")
        elif kv.startswith(b"NROS_GATE_LANE="):
            lane = True
    cache[pid] = (lane or name is not None, name)
    return cache[pid]


def comm_of(pid):
    try:
        return Path(f"/proc/{pid}/comm").read_text().strip().replace(" ", "_") or "?"
    except OSError:
        return "?"


def runnable_by_owner(cache):
    """(lane, counts): runnable THREADS (what `procs_running` counts) that the
    lane owns, and per owner: a gate test, or `?<comm>` for one no gate test
    owns (`!<comm>` when it is not the lane's at all, e.g. another tenant)."""
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
    free = fifo_tokens(fifo) if fifo else None
    if free is None and fifo:
        # That pool is gone (one lane step's pool ended and the next opened
        # its own): find the current one on the next sample.
        cache.pop("fifo", None)
    if free is not None:
        counts["#free"] = free
    return lane, counts


def sample(path, interval):
    stop = []
    signal.signal(signal.SIGTERM, lambda *_: stop.append(1))
    cache = {}
    with open(path, "a") as out:
        while not stop:
            lane, counts = runnable_by_owner(cache)
            fields = [str(int(time.time())), str(procs_running()), f"#lane={lane}"]
            fields += [f"{g}={n}" for g, n in sorted(counts.items())]
            out.write(" ".join(fields) + "\n")
            out.flush()
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
    if bad:
        print("run-load-sampler self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if len(argv) >= 2 and argv[0] == "sample":
        interval = 30.0
        if len(argv) == 4 and argv[2] == "--interval":
            interval = float(argv[3])
        return sample(argv[1], interval)
    if len(argv) == 3 and argv[0] == "report" and argv[2].isdigit():
        return report(argv[1], int(argv[2]))
    if len(argv) == 5 and argv[0] == "report" and all(a.isdigit() for a in argv[2:]):
        return report(argv[1], int(argv[2]), (int(argv[3]), int(argv[4])))
    print(__doc__.split("Usage:")[1].split("\n\n")[0], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
