#!/usr/bin/env python3
"""RFC-0079 — does a running image's thread table match its port's plan?

A `[board.priority_plan]` states where things SIT. Nothing has ever checked
that claim against a live process, and `reserved.foreign` in particular is a
declaration about threads nano-ros does not create — lwIP's `tcpip_thread`, a
work queue, whatever the RTOS or libc started. On POSIX that is answerable
cheaply: `/proc/<pid>/task/*/stat` carries each thread's scheduling policy and
real-time priority.

    python3 scripts/dev/priority-thread-probe.py <pid> [tier_key]

Classifies every thread against the plan and prints what it found. It REPORTS;
deciding what a surprise means is the reader's job, because the interesting
answers here are the threads nobody wrote down.

Linux only — this reads procfs. Other ports need their own enumerator, and
`reserved.foreign` stays an unverified claim there (RFC-0079 open question).
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "lib"))
from priority_plan import load_plans

# `man 5 proc`, /proc/[pid]/stat: 1-indexed field 41 is rt_priority and 42 is
# policy. Both sit AFTER comm, which may itself contain spaces and brackets —
# so the tail is split from the last ')' rather than by naive whitespace.
RT_PRIORITY_FIELD = 41
POLICY_FIELD = 42
POLICIES = {0: "OTHER", 1: "FIFO", 2: "RR", 3: "BATCH", 5: "IDLE", 6: "DEADLINE"}


def threads(pid):
    out = []
    for t in sorted(Path(f"/proc/{pid}/task").iterdir(), key=lambda p: int(p.name)):
        try:
            raw = (t / "stat").read_text(encoding="utf-8", errors="replace")
            name = (t / "comm").read_text(encoding="utf-8").strip()
        except OSError:
            continue  # a thread may exit between listing and reading
        tail = raw[raw.rfind(")") + 2:].split()
        # tail[0] is field 3, so field N is tail[N - 3].
        try:
            rtprio = int(tail[RT_PRIORITY_FIELD - 3])
            policy = int(tail[POLICY_FIELD - 3])
        except (IndexError, ValueError):
            continue
        out.append((int(t.name), name, policy, rtprio))
    return out


def classify(policy, rtprio, plan):
    """-> (bucket, note). Only a real-time policy can be IN a band at all."""
    if policy not in (1, 2):
        return "not-scheduled", "SCHED_OTHER — outside every band by construction"
    for nm, (lo, hi) in plan["reserved"].items():
        if lo <= rtprio <= hi:
            return f"reserved.{nm}", ""
    for nm, (lo, hi) in plan["pool"].items():
        if lo <= rtprio <= hi:
            return f"pool.{nm}", ""
    return "UNDECLARED", "real-time priority in no band this plan describes"


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    pid, tier_key = argv[1], (argv[2] if len(argv) > 2 else "posix")
    if not Path(f"/proc/{pid}/task").is_dir():
        print(f"no such live process: {pid}")
        return 2
    plan = load_plans().get(tier_key)
    if plan is None:
        print(f"{tier_key!r} declares no [board.priority_plan]")
        return 2
    if plan.get("derived"):
        print(f"{tier_key!r} has a DERIVED plan — resolve it per image first")
        return 2

    res = ", ".join(f"{k} {v[0]}..{v[1]}" for k, v in plan["reserved"].items())
    pool = ", ".join(f"{k} {v[0]}..{v[1]}" for k, v in plan["pool"].items())
    print(f"plan ({tier_key}): reserved {res} | pool {pool} | {plan['source']}\n")
    print(f"{'TID':>8}  {'THREAD':<18} {'POLICY':<8} {'RTPRIO':>6}  BUCKET")

    buckets = {}
    for tid, name, policy, rtprio in threads(pid):
        bucket, note = classify(policy, rtprio, plan)
        buckets[bucket] = buckets.get(bucket, 0) + 1
        pol = POLICIES.get(policy, str(policy))
        print(f"{tid:>8}  {name:<18} {pol:<8} {rtprio:>6}  {bucket}"
              + (f"   ({note})" if note else ""))

    print("\nsummary:")
    for b, n in sorted(buckets.items(), key=lambda kv: -kv[1]):
        print(f"  {n:3d}  {b}")
    if "UNDECLARED" in buckets:
        print("\n  UNDECLARED threads hold a real-time priority in no band the plan\n"
              "  describes. Either the plan is missing a `reserved.*` entry, or\n"
              "  something is placing a thread nobody accounted for.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
