#!/usr/bin/env python3
"""The warm job's cache must use the SAME key and path as the job it warms.

`gate.yml` has two cache-writing jobs and they only cooperate if they agree
exactly:

  * `check-*`    — the gate's sibling jobs (phase-489 W1 split the one
                   `check` job into them); they read on pull_request /
                   merge_group.
  * `warm-cache` — writes on push to `main`, which is the ONLY scope those
                   runs can read from.

GitHub scopes a cache to the ref that created it; a run may restore from its own
ref or from the default branch, and nothing else. A merge_group run executes on
an ephemeral `gh-readonly-queue/...` ref, so the cache IT saves is unreadable
forever and `main` is its only usable scope. That is why the warm job exists.

If the two keys drift by one character, the warm job still succeeds, still
uploads a cache, and warms NOTHING — the consumer misses and rebuilds, exactly
as before, with no error anywhere. A silent no-op is the failure mode this
guards, and it is the same shape as an authored map that stops matching what it
names.

Phase-489 W4 adds a second agreement, across WORKFLOWS: every job that runs
`just generate-bindings` (scheduled `gate` and `host-tests`, today) restores
and saves the bindings cache under the key, prefix and path list printed by
`scripts/ci/bindings-cache.sh key` — never a hand-written copy — restoring
BEFORE the verb and saving AFTER it. Both lanes run on `main`'s schedule, so
they share one cache scope; a job whose key or paths drifted would still go
green and simply never share, which is the same silent no-op as above.

Phase-489 W5 adds a third: the sccache directory the `check-*` jobs restore is
only worth restoring if the warm job FILLED it with the compiles those jobs
make. `warm-cache` used to warm the CLI build alone, so `check-fast` (~1240
compile requests, almost all in its own gates) restored a cache that answered 30
of them (3.7 %). The rule: `warm-cache` runs `just check fast` AFTER it
configures sccache and BEFORE it saves the directory, and every job names the
SAME `SCCACHE_CACHE_SIZE` (a smaller warmer silently evicts what a larger
consumer expects to find).

Run:  python3 scripts/check-gate-cache-keys-agree.py [--self-test]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GATE = os.path.join(ROOT, ".github", "workflows", "gate.yml")
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

# (consumer job-name PREFIX, warmer job, what the pair caches). A prefix, not a
# name: phase-489 W1 split the one `check` consumer into `check-*` siblings,
# and a hand list of them would be one more table to forget a sibling in. Every
# job matching the prefix is a consumer; at least one must exist.
PAIRS = [("check-", "warm-cache", "the compile-tier CLI + resolver build")]


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


def _path_values(blk):
    """The `path:` value(s) of one `with:` block — a single inline value, or
    every line of a `path: |` block scalar.

    INDENTATION-SCOPED, not "any indented line": a flat `\\s{10,}` scan used to
    sweep up `restore-keys: |` and ITS OWN block-scalar value as if they were
    paths, because both happen to sit at the same indent as `path:` itself.
    That was invisible as long as every restore step also carried a save step
    with the identical `restore-keys:` tail — restore/save split (a job with a
    restore step's `restore-keys:` but no matching save step) breaks that
    symmetry and turns the pollution into a real false mismatch. Scoping to
    lines STRICTLY DEEPER than `path:`'s own indent, stopping at the next line
    that is not, is what a YAML block scalar actually means.
    """
    lines = blk.split("\n")
    for i, line in enumerate(lines):
        m = re.match(r"^(\s*)path:\s*(.*)$", line)
        if not m:
            continue
        indent, val = len(m.group(1)), m.group(2).strip()
        if val and val not in ("|", "|-", ">", ">-"):
            return [val]
        out = []
        for line2 in lines[i + 1 :]:
            if not line2.strip():
                continue
            indent2 = len(line2) - len(line2.lstrip(" "))
            if indent2 <= indent:
                break
            out.append(line2.strip())
        return out
    return []


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
        norm = _path_values(blk)
        if key:
            out.append((tuple(sorted(p for p in norm if p)), key.group(1).strip()))
    return out


def restore_only_keys(body):
    """Keys a job RESTORES with `actions/cache/restore` (no save of its own)."""
    out = []
    for m in re.finditer(
        r"uses:\s*actions/cache/restore@[^\n]*\n(.*?)(?=\n\s*- name:|\Z)", body, re.S
    ):
        key = re.search(r"^\s*key:\s*(.+)$", m.group(1), re.M)
        if key:
            out.append(key.group(1).strip())
    return out


def produced_keys(body):
    """Keys a job WRITES: `actions/cache` (restores and saves) or `/save`."""
    out = set()
    for m in re.finditer(
        r"uses:\s*actions/cache(?:/save)?@[^\n]*\n(.*?)(?=\n\s*- name:|\Z)", body, re.S
    ):
        key = re.search(r"^\s*key:\s*(.+)$", m.group(1), re.M)
        if key:
            out.add(key.group(1).strip())
    return out


# phase-489 W4 — the bindings cache's ONE spelling, read from the key step.
BINDINGS_VERB = re.compile(r"^\s*just generate-bindings\s*$", re.M)
BINDINGS_KEY_RUN = "scripts/ci/bindings-cache.sh key >> \"$GITHUB_OUTPUT\""
BINDINGS_WANT = {
    "path": "${{ steps.bindings-key.outputs.paths }}",
    "key": "${{ steps.bindings-key.outputs.key }}",
    "restore-keys": "${{ steps.bindings-key.outputs.restore-prefix }}",
}


def _steps(body):
    """[(start offset, step text)] for each `- name:`/`- uses:` step of a job."""
    starts = [m.start() for m in re.finditer(r"^\s{6}- ", body, re.M)]
    return [(a, body[a:b]) for a, b in zip(starts, starts[1:] + [len(body)])]


def _with(step, field):
    m = re.search(r"^\s*%s:\s*(.+)$" % re.escape(field), step, re.M)
    return m.group(1).strip() if m else None


def bindings_problems(where, body):
    """Problems with one job's bindings cache, or [] if it runs no verb."""
    verb = BINDINGS_VERB.search(body)
    if not verb:
        return []
    steps = _steps(body)
    key_at = [a for a, t in steps if BINDINGS_KEY_RUN in t and "id: bindings-key" in t]
    restores = [(a, t) for a, t in steps if re.search(r"uses:\s*actions/cache/restore@", t)
                and _with(t, "key") == BINDINGS_WANT["key"]]
    saves = [(a, t) for a, t in steps if re.search(r"uses:\s*actions/cache/save@", t)
             and _with(t, "key") == BINDINGS_WANT["key"]]
    out = []
    if not key_at:
        out.append("%s runs `just generate-bindings` with no `id: bindings-key` step running "
                   "`%s`" % (where, BINDINGS_KEY_RUN))
    if not restores:
        out.append("%s runs `just generate-bindings` but restores no cache keyed "
                   "`%s` — the verb runs from cold (18-22 min)" % (where, BINDINGS_WANT["key"]))
    if not saves:
        out.append("%s restores the bindings cache but never saves `%s` — nothing "
                   "it regenerates reaches the next run" % (where, BINDINGS_WANT["key"]))
    for kind, found, fields in (("restore", restores, ("path", "restore-keys")),
                                ("save", saves, ("path",))):
        for a, t in found:
            for f in fields:
                got = _with(t, f)
                if got != BINDINGS_WANT[f]:
                    out.append("%s: the bindings %s step's `%s:` is %r, not %r — the list "
                               "is the key script's, never a copy" % (where, kind, f, got,
                                                                      BINDINGS_WANT[f]))
            if kind == "restore" and a > verb.start():
                out.append("%s restores the bindings cache AFTER the verb ran" % where)
            if kind == "save" and a < verb.start():
                out.append("%s saves the bindings cache BEFORE the verb ran" % where)
    if key_at and restores and min(key_at) > min(a for a, _ in restores):
        out.append("%s restores the bindings cache before its key step" % where)
    return out


def all_bindings_problems():
    """(jobs checked, problems) over every workflow file."""
    checked, out = 0, []
    for name in sorted(os.listdir(WORKFLOWS)):
        if not name.endswith((".yml", ".yaml")):
            continue
        with open(os.path.join(WORKFLOWS, name), encoding="utf8") as fh:
            for job, body in jobs(fh.read()).items():
                if BINDINGS_VERB.search(body):
                    checked += 1
                    out += bindings_problems("%s:%s" % (name, job), body)
    return checked, out


# phase-489 W5 — the warm job must fill the sccache dir with what `check-fast` compiles.
FAST_VERB = re.compile(r"^\s*(?:PATH=\S+\s+)?just check fast\s*$", re.M)


def sccache_warm_problems(warm_body, all_bodies):
    """Problems with how `warm-cache` fills the sccache directory, and with the
    cache size the jobs name. `all_bodies` is {job: body}."""
    out = []
    sizes = {}
    for job, body in all_bodies.items():
        for m in re.finditer(r"SCCACHE_CACHE_SIZE=(\d+[KMGT])\b", body):
            sizes.setdefault(m.group(1), set()).add(job)
    if len(sizes) > 1:
        out.append(
            "jobs disagree on SCCACHE_CACHE_SIZE: %s — the warm job and its consumers "
            "must name one size, or the smaller evicts what the larger expects"
            % "; ".join("%s in %s" % (v, sorted(j)) for v, j in sorted(sizes.items()))
        )
    cfg = re.search(r"^\s*- name: Configure sccache", warm_body, re.M)
    fast = FAST_VERB.search(warm_body)
    save = re.search(r"^\s*- name: Save sccache directory", warm_body, re.M)
    if not (cfg and save):
        out.append("`warm-cache` has no `Configure sccache` / `Save sccache directory` step")
    elif not fast:
        out.append(
            "`warm-cache` never runs `just check fast`, so the sccache entry it saves holds "
            "the CLI build and none of what `check-fast` compiles (3.7 %% hit rate, phase-489 W5)"
        )
    elif not (cfg.start() < fast.start() < save.start()):
        out.append(
            "`warm-cache` must run `just check fast` AFTER `Configure sccache` (or the compiles "
            "bypass the cache) and BEFORE `Save sccache directory` (or they are never saved)"
        )
    return out


def concurrency_problems(text):
    """The workflow-level `cancel-in-progress` must be false for `push` (and
    `merge_group`): a cancelled push run is a cancelled `warm-cache`, and the
    main-scope cache then never tracks main (phase-489 W5). The expression is a
    `&&` chain of `github.event_name != '<ev>'` terms, or a literal."""
    m = re.search(r"^concurrency:\n(?:[ \t]+.*\n|[ \t]*\n)*?[ \t]+cancel-in-progress:[ \t]*(.+)$", text, re.M)
    if not m:
        return ["gate.yml has no workflow-level `concurrency: cancel-in-progress:` — this rule asserted nothing"]
    expr = m.group(1).strip()
    expr = re.sub(r"^\$\{\{\s*|\s*\}\}$", "", expr)
    out = []
    for ev in ("push", "merge_group"):
        val = True
        for term in expr.split("&&"):
            term = term.strip()
            t = re.fullmatch(r"github\.event_name\s*(!=|==)\s*'([a-z_]+)'", term)
            if t:
                val = val and ((t.group(2) != ev) if t.group(1) == "!=" else (t.group(2) == ev))
            elif term == "false":
                val = False
            elif term != "true":
                return ["cancel-in-progress term %r is not understood — keep it a `&&` chain of event_name tests" % term]
        if val:
            out.append(
                "workflow `cancel-in-progress` is TRUE for a %r run: the next merge cancels the "
                "in-flight `warm-cache`, so main's cache stops tracking main (6 of 8 cancelled, "
                "phase-489 W5)" % ev
            )
    return out


def self_test():
    cw = "concurrency:\n  group: g\n  # c\n  cancel-in-progress: ${{ %s }}\n\nenv:\n  A: 1\n"
    assert concurrency_problems(cw % "github.event_name != 'merge_group' && github.event_name != 'push'") == []
    assert concurrency_problems(cw % "false") == []
    for bad in ("github.event_name != 'merge_group'", "true", "github.event_name != 'push'"):
        assert concurrency_problems(cw % bad), "concurrency rule missed: " + bad
    assert concurrency_problems("name: x\n"), "missing concurrency must be reported"

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
    # issue 1616: a restore-only key nobody writes is caught; a written one is not.
    assert restore_only_keys(js["a"]) == ["k-1"] and produced_keys(js["b"]) == {"k-1"}
    assert produced_keys(js["a"]) == set(), "a restore-only step writes nothing"

    # A restore step (with `restore-keys:`) and a save step (without) for the
    # SAME multi-line `path: |` + `key:` must parse to the SAME entry — this
    # is exactly the restore/save split shape, and `restore-keys:`'s own
    # block-scalar value must never be swept in as a path.
    t2 = (
        "jobs:\n"
        "  restore:\n"
        "    steps:\n"
        "      - uses: actions/cache/restore@v4\n"
        "        with:\n"
        "          path: |\n"
        "            packages/cli/target\n"
        "            packages/cli/nros-launch-resolve/target\n"
        "          key: nros-cli-x\n"
        "          restore-keys: |\n"
        "            nros-cli-\n"
        "  save:\n"
        "    steps:\n"
        "      - uses: actions/cache/save@v4\n"
        "        with:\n"
        "          path: |\n"
        "            packages/cli/target\n"
        "            packages/cli/nros-launch-resolve/target\n"
        "          key: nros-cli-x\n"
    )
    js2 = jobs(t2)
    ce_restore = cache_entries(js2["restore"])
    ce_save = cache_entries(js2["save"])
    assert ce_restore == ce_save, (ce_restore, ce_save)
    assert ce_restore[0][0] == (
        "packages/cli/nros-launch-resolve/target",
        "packages/cli/target",
    ), ce_restore

    # phase-489 W4 — the bindings rule: the real shape passes, and each
    # mutation it exists to catch is reported.
    good = (
        "jobs:\n"
        "  j:\n"
        "    steps:\n"
        "      - name: key\n"
        "        id: bindings-key\n"
        "        run: |\n"
        "          scripts/ci/bindings-cache.sh key >> \"$GITHUB_OUTPUT\"\n"
        "      - name: restore\n"
        "        id: bindings-cache\n"
        "        uses: actions/cache/restore@v4\n"
        "        with:\n"
        "          path: ${{ steps.bindings-key.outputs.paths }}\n"
        "          key: ${{ steps.bindings-key.outputs.key }}\n"
        "          restore-keys: ${{ steps.bindings-key.outputs.restore-prefix }}\n"
        "      - name: verb\n"
        "        run: |\n"
        "          just generate-bindings\n"
        "      - name: save\n"
        "        uses: actions/cache/save@v4\n"
        "        with:\n"
        "          path: ${{ steps.bindings-key.outputs.paths }}\n"
        "          key: ${{ steps.bindings-key.outputs.key }}\n"
    )
    assert bindings_problems("t", jobs(good)["j"]) == [], bindings_problems("t", jobs(good)["j"])
    mutations = {
        "hand-written paths": good.replace(
            "path: ${{ steps.bindings-key.outputs.paths }}\n          key: ${{ steps.bindings-key.outputs.key }}\n          restore",
            "path: examples/**/generated\n          key: ${{ steps.bindings-key.outputs.key }}\n          restore"),
        "no fallback prefix": good.replace(
            "restore-keys: ${{ steps.bindings-key.outputs.restore-prefix }}", "restore-keys: x-"),
        "no save": good[: good.index("      - name: save")],
        "no key step": good.replace("id: bindings-key", "id: other"),
        "restore after verb": good.replace("      - name: restore", "      - name: verb0\n"
                                           "        run: |\n          just generate-bindings\n"
                                           "      - name: restore", 1),
        "no restore": good.replace("actions/cache/restore@v4", "actions/checkout@v4"),
    }
    for what, text in mutations.items():
        assert bindings_problems("t", jobs(text)["j"]), "bindings rule missed: " + what

    # phase-489 W5 — the warm-job rule: the real shape passes, each mutation is caught.
    warm_good = (
        "    steps:\n"
        "      - name: Configure sccache\n"
        "        run: |\n"
        "          export SCCACHE_CACHE_SIZE=2G\n"
        "      - name: just check fast\n"
        "        run: |\n"
        "          source ./activate.sh\n"
        "          just check fast\n"
        "      - name: Save sccache directory\n"
        "        uses: actions/cache/save@v4\n"
    )
    consumer = "    steps:\n      - run: export SCCACHE_CACHE_SIZE=2G\n"
    assert sccache_warm_problems(warm_good, {"w": warm_good, "c": consumer}) == []
    for what, wbody, bodies in (
        ("no fast verb", warm_good.replace("just check fast\n      - name: Save", "echo hi\n      - name: Save"), None),
        ("fast after save", warm_good.replace(
            "      - name: just check fast\n        run: |\n          source ./activate.sh\n          just check fast\n", "")
            + "      - name: late\n        run: |\n          just check fast\n", None),
        ("fast before configure", "    steps:\n      - run: |\n          just check fast\n"
            + warm_good.split("    steps:\n", 1)[1].replace("just check fast\n", "true\n"), None),
        ("size drift", warm_good, {"w": warm_good, "c": consumer.replace("2G", "1G")}),
    ):
        got = sccache_warm_problems(wbody, bodies or {"w": wbody, "c": consumer})
        assert got, "warm-cache rule missed: " + what

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
    for prefix, warmer, what in PAIRS:
        consumers = sorted(j for j in js if j.startswith(prefix) and j != warmer)
        if not consumers or warmer not in js:
            problems.append(
                "gate.yml has no job matching %r*, or no %r — this gate names a pair "
                "that does not exist, so it was asserting nothing." % (prefix, warmer)
            )
            continue
        we = {k: p for p, k in cache_entries(js[warmer])}
        read = {}  # key -> {consumer: paths}
        for c in consumers:
            for p, k in cache_entries(js[c]):
                read.setdefault(k, {})[c] = p
        if not read or not we:
            problems.append(
                "no actions/cache step found in %s or %r (found %d / %d) — the "
                "shape changed and this gate would pass vacuously."
                % ("/".join(consumers), warmer, len(read), len(we))
            )
            continue
        # issue 1616 (W7): the CONSUMER direction too. A restore-only step whose
        # key neither the warmer nor the consumer itself ever writes restores
        # nothing, forever — and a one-way check (warmer -> consumer) let one
        # be added with the gate green.
        for c in consumers:
            written = produced_keys(js[warmer]) | produced_keys(js[c])
            for key in restore_only_keys(js[c]):
                checked += 1
                if key not in written:
                    problems.append(
                        "`%s` restores key\n      %s\n    which neither `%s` nor `%s` "
                        "ever saves (%s). A restore of a key nobody writes is a cache "
                        "miss on every run that reads as a cold cache."
                        % (c, key, warmer, c, what)
                    )
        for key, paths in we.items():
            checked += 1
            if key not in read:
                problems.append(
                    "`%s` warms key\n      %s\n    which no `%s*` job reads (%s).\n"
                    "    A warm job whose key does not match its consumer still "
                    "succeeds and still uploads — and warms NOTHING."
                    % (warmer, key, prefix, what)
                )
                continue
            for c, cpaths in sorted(read[key].items()):
                if cpaths != paths:
                    problems.append(
                        "`%s` and `%s` share key\n      %s\n    but cache different "
                        "paths:\n      consumer: %s\n      warmer:   %s"
                        % (c, warmer, key, list(cpaths), list(paths))
                    )

    with open(GATE, encoding="utf8") as fh:
        problems += concurrency_problems(fh.read())
    checked += 1

    if "warm-cache" in js:
        sprob = sccache_warm_problems(js["warm-cache"], js)
        problems += sprob
        checked += 1

    bjobs, bprob = all_bindings_problems()
    if bjobs == 0:
        problems.append(
            "no workflow job runs `just generate-bindings` — the bindings rule names a "
            "shape that no longer exists, so it was asserting nothing."
        )
    problems += bprob
    checked += bjobs

    if problems:
        sys.stderr.write("check-gate-cache-keys-agree: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        return 1

    sys.stdout.write(
        "check-gate-cache-keys-agree: OK — %d key pairing(s) checked; every warmed key is "
        "read by a consumer with the same paths, every restored key is written, and every "
        "`just generate-bindings` job restores and saves the bindings cache under the key "
        "script's own key and paths.\n" % checked
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
