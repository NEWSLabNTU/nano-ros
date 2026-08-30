#!/usr/bin/env python3
"""`nros-platform-posix` is named for a standard — keep it holding to one.

WHY THIS EXISTS

phase-401 measured the host stack and found the two layers say different words
for good reason:

    nros-platform-posix   POSIX-clean   → the name is a claim about the STANDARD
    nros-board-linux      Linux-only    → `sched_setaffinity`, absent on apple

The platform crate's name is the load-bearing one. It is what lets the tree say
"the platform layer names software-stack facts, the board layer names what we
support" — and that sentence is only true while the crate stays portable.

Nothing was enforcing it. The audit found the crate scrupulously clean today
(its one `__linux__` selects `MSG_NOSIGNAL` behind a portable `#else`;
`pthread_setname_np` is gated on `_GNU_SOURCE` rather than the OS) — which is
precisely the state worth protecting, because the next `epoll` added here would
be invisible: it would compile and pass CI on the only host anyone runs.

WHAT IT CHECKS

Linux-only constructs in `nros-platform-posix`'s C sources, unless the line is
guarded. A guard is `#ifdef __linux__` / `#if defined(__linux__)` / `_GNU_SOURCE`
within the enclosing few lines, or the symbol appearing only inside a comment.

WHAT IT DOES NOT CHECK, AND WHY

* **The board crate.** `nros-board-linux` is Linux-only ON PURPOSE and says so
  in its name. Checking it would be checking that a thing is what it says.
* **macOS portability.** The crate does NOT build on macOS, and that is macOS's
  incomplete POSIX rather than this crate's Linux-ness: `timer_create`,
  `sem_timedwait` and working unnamed `sem_init` are all absent there, while
  every BSD has them. Enforcing "builds on macOS" would demand a port
  (dispatch sources, named semaphores) whose paths no CI lane could run — the
  exact shape phase-260 rejected when it dropped macOS.
* **Whether \\*BSD actually works.** Nothing in the crate blocks it — measured:
  no `/proc`, `epoll`, `prctl`, `accept4`, `pipe2`, `gettid` or bare `_NP` — but
  "nothing blocks it" is not "it runs", and there is no BSD runner. This gate
  keeps that door open; it does not claim anyone has walked through it.

Usage:  check-posix-platform-purity.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATE = os.path.join(ROOT, "packages", "platform", "nros-platform-posix", "src")

# Linux-only, or Linux-specific enough that a POSIX-named crate should not reach
# for them unguarded. Each is a word-boundary match.
FORBIDDEN = {
    "epoll_create": "epoll is Linux-only; POSIX has poll/select",
    "epoll_create1": "epoll is Linux-only; POSIX has poll/select",
    "epoll_ctl": "epoll is Linux-only; POSIX has poll/select",
    "epoll_wait": "epoll is Linux-only; POSIX has poll/select",
    "eventfd": "eventfd is a Linux syscall; POSIX has pipe",
    "signalfd": "signalfd is a Linux syscall",
    "timerfd_create": "timerfd is Linux-only; POSIX has timer_create",
    "inotify_init": "inotify is Linux-only",
    "memfd_create": "memfd_create is Linux-only",
    "prctl": "prctl is Linux-only",
    "gettid": "gettid is Linux-only",
    "accept4": "accept4 is Linux-only (also on some BSDs, not portable)",
    "pipe2": "pipe2 is Linux-only (also on some BSDs, not portable)",
    "sched_setaffinity": "affinity is Linux-only — it belongs in nros-board-linux",
    "sched_getaffinity": "affinity is Linux-only — it belongs in nros-board-linux",
    "cpu_set_t": "affinity is Linux-only — it belongs in nros-board-linux",
    "CPU_SET": "affinity is Linux-only — it belongs in nros-board-linux",
    "sched_setattr": "sched_setattr/SCHED_DEADLINE are Linux-only",
    "SCHED_DEADLINE": "sched_setattr/SCHED_DEADLINE are Linux-only",
    "SCHED_BATCH": "SCHED_BATCH is Linux-only",
    "SCHED_IDLE": "SCHED_IDLE is Linux-only",
}

# A line is excused when a guard opens within this many lines above it.
GUARD_WINDOW = 12
GUARD = re.compile(r"__linux__|_GNU_SOURCE|__ANDROID__")


def strip_comments(text):
    """Blank out /* */ and // comments, preserving line structure.

    Line structure matters: the report names line numbers, and a naive strip
    would shift every one of them.
    """
    out = []
    in_block = False
    for line in text.split("\n"):
        res, i = [], 0
        while i < len(line):
            if in_block:
                end = line.find("*/", i)
                if end == -1:
                    i = len(line)
                else:
                    in_block = False
                    i = end + 2
            else:
                start = line.find("/*", i)
                slash = line.find("//", i)
                if slash != -1 and (start == -1 or slash < start):
                    res.append(line[i:slash])
                    break
                if start == -1:
                    res.append(line[i:])
                    break
                res.append(line[i:start])
                in_block = True
                i = start + 2
        out.append("".join(res))
    return out


def offending(lines):
    """[(lineno, symbol, why)] for unguarded uses."""
    hits = []
    for n, line in enumerate(lines, 1):
        for sym, why in FORBIDDEN.items():
            if not re.search(rf"\b{re.escape(sym)}\b", line):
                continue
            window = lines[max(0, n - 1 - GUARD_WINDOW) : n]
            if any(GUARD.search(w) for w in window):
                continue
            hits.append((n, sym, why))
    return hits


def check():
    problems = []
    if not os.path.isdir(CRATE):
        raise SystemExit(f"check-posix-platform-purity: no {CRATE}")
    for name in sorted(os.listdir(CRATE)):
        if not name.endswith((".c", ".h")):
            continue
        path = os.path.join(CRATE, name)
        with open(path, encoding="utf8", errors="replace") as fh:
            lines = strip_comments(fh.read())
        for lineno, sym, why in offending(lines):
            problems.append((os.path.relpath(path, ROOT), lineno, sym, why))
    return problems


def self_test():
    """Prove the check can fail. Runs on EVERY invocation — a negative control
    nobody runs decays into a comment (`check-gate-selftests`)."""
    cases = [
        # (source, expected number of hits)
        ("int f(void) { return epoll_create1(0); }", 1),
        # Guarded: the whole point of the window.
        ("#ifdef __linux__\nint f(void){return eventfd(0,0);}\n#endif", 0),
        # A comment mentioning it is not a use — this file's own docs do that.
        ("/* TODO: forward via signalfd/eventfd */\nint f(void){return 0;}", 0),
        # `_GNU_SOURCE` counts as a guard, as `pthread_setname_np` uses today.
        ("#define _GNU_SOURCE\nint f(void){return gettid();}", 0),
        # Clean POSIX stays clean.
        ("#include <sched.h>\nint f(void){return sched_yield();}", 0),
        # The affinity family, which belongs one layer up.
        ("void f(void){ cpu_set_t s; CPU_SET(0,&s); }", 2),
    ]
    fails = []
    for i, (src, want) in enumerate(cases):
        got = len(offending(strip_comments(src)))
        if got != want:
            fails.append(f"case {i}: expected {want} hit(s), got {got}")
    if fails:
        for f in fails:
            print(f"check-posix-platform-purity self-test: FAIL {f}", file=sys.stderr)
        raise SystemExit(1)


def main():
    problems = check()
    if problems:
        print(
            "check-posix-platform-purity: Linux-only constructs in a crate named "
            "for the POSIX standard:\n",
            file=sys.stderr,
        )
        for rel, lineno, sym, why in problems:
            print(f"  {rel}:{lineno}  `{sym}` — {why}", file=sys.stderr)
        print(
            "\n  This crate's NAME is a claim about the standard, and the tree "
            "relies on it:\n"
            "  the platform layer names software-stack facts, the board layer "
            "names what we\n"
            "  support (CLAUDE.md “Naming”). Linux-only code belongs in "
            "`nros-board-linux`.\n"
            "  If it genuinely has to live here, guard it with `#ifdef __linux__` "
            "and give\n"
            "  the other branch something real to do.",
            file=sys.stderr,
        )
        return 1
    print(
        "check-posix-platform-purity OK — nros-platform-posix uses no unguarded "
        "Linux-only construct."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
