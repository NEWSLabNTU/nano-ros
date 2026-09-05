#!/usr/bin/env python3
"""The warm job's cache must use the SAME key and path as the job it warms.

`gate.yml` has two cache-writing jobs and they only cooperate if they agree
exactly:

  * `check`      — reads the cache on pull_request / merge_group.
  * `warm-cache` — writes it on push to `main`, which is the ONLY scope the
                   other two can read from.

GitHub scopes a cache to the ref that created it; a run may restore from its own
ref or from the default branch, and nothing else. A merge_group run executes on
an ephemeral `gh-readonly-queue/...` ref, so the cache IT saves is unreadable
forever and `main` is its only usable scope. That is why the warm job exists.

If the two keys drift by one character, the warm job still succeeds, still
uploads a cache, and warms NOTHING — the consumer misses and rebuilds, exactly
as before, with no error anywhere. A silent no-op is the failure mode this
guards, and it is the same shape as an authored map that stops matching what it
names.

Run:  python3 scripts/check-gate-cache-keys-agree.py [--self-test]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GATE = os.path.join(ROOT, ".github", "workflows", "gate.yml")

# (consumer job, warmer job, what the pair caches)
PAIRS = [("check", "warm-cache", "the compile-tier CLI + resolver build")]


def jobs(text):
    """{job name: body text} for each top-level job in the workflow."""
    out, cur, buf = {}, None, []
    started = False
    for line in text.split("\n"):
        if line.startswith("jobs:"):
            started = True
            continue
        if not started:
            continue
        m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if m:
            if cur:
                out[cur] = "\n".join(buf)
            cur, buf = m.group(1), []
            continue
        if cur:
            buf.append(line)
    if cur:
        out[cur] = "\n".join(buf)
    return out


def cache_entries(body):
    """[(path_block, key)] for every actions/cache step in a job body."""
    out = []
    # `actions/cache`, `actions/cache/restore` and `actions/cache/save` all count:
    # the downstream jobs are RESTORE-only (a save on an ephemeral ref is a cache
    # nothing can ever read), and the pairing this gate checks is between the
    # reader's key and the writer's key regardless of which variant each uses.
    for m in re.finditer(
        r"uses:\s*actions/cache(?:/(?:restore|save))?@[^\n]*\n(.*?)(?=\n\s*- name:|\Z)",
        body,
        re.S,
    ):
        blk = m.group(1)
        key = re.search(r"^\s*key:\s*(.+)$", blk, re.M)
        paths = re.findall(r"^\s{10,}([^\s#][^\n]*)$", blk, re.M)
        pth = re.search(r"^\s*path:\s*(.+)$", blk, re.M)
        single = pth.group(1).strip() if pth and pth.group(1).strip() != "|" else None
        norm = [single] if single else [p.strip() for p in paths if not p.strip().startswith("key:")]
        if key:
            out.append((tuple(sorted(p for p in norm if p)), key.group(1).strip()))
    return out


def self_test():
    t = (
        "jobs:\n"
        "  a:\n"
        "    steps:\n"
        "      - uses: actions/cache/restore@v4\n"
        "        with:\n"
        "          path: /tmp/x\n"
        "          key: k-1\n"
        "  b:\n"
        "    steps:\n"
        "      - uses: actions/cache@v4\n"
        "        with:\n"
        "          path: /tmp/x\n"
        "          key: k-1\n"
    )
    js = jobs(t)
    assert set(js) == {"a", "b"}, js.keys()
    assert cache_entries(js["a"]) == cache_entries(js["b"]), (
        cache_entries(js["a"]),
        cache_entries(js["b"]),
    )
    assert cache_entries(js["a"])[0][1] == "k-1"
    sys.stdout.write("check-gate-cache-keys-agree self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    with open(GATE, encoding="utf8") as fh:
        js = jobs(fh.read())

    problems = []
    checked = 0
    for consumer, warmer, what in PAIRS:
        if consumer not in js or warmer not in js:
            problems.append(
                "gate.yml has no job %r or %r — this gate names a pair that does "
                "not exist, so it was asserting nothing."
                % (consumer, warmer)
            )
            continue
        ce = {k: p for p, k in cache_entries(js[consumer])}
        we = {k: p for p, k in cache_entries(js[warmer])}
        if not ce or not we:
            problems.append(
                "no actions/cache step found in %r or %r (found %d / %d) — the "
                "shape changed and this gate would pass vacuously."
                % (consumer, warmer, len(ce), len(we))
            )
            continue
        for key, paths in we.items():
            checked += 1
            if key not in ce:
                problems.append(
                    "`%s` warms key\n      %s\n    which `%s` never reads (%s).\n"
                    "    A warm job whose key does not match its consumer still "
                    "succeeds and still uploads — and warms NOTHING."
                    % (warmer, key, consumer, what)
                )
            elif ce[key] != paths:
                problems.append(
                    "`%s` and `%s` share key\n      %s\n    but cache different "
                    "paths:\n      consumer: %s\n      warmer:   %s"
                    % (consumer, warmer, key, list(ce[key]), list(paths))
                )

    if problems:
        sys.stderr.write("check-gate-cache-keys-agree: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        return 1

    sys.stdout.write(
        "check-gate-cache-keys-agree: OK — %d warmed key(s) match their consumer.\n" % checked
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
