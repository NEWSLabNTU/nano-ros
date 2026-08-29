#!/usr/bin/env python3
"""Issue 0827 — the infrastructure-queryable counts have ONE definition each,
and it matches the number of servers actually created.

A service server IS a zenoh queryable, so these counts are a term in every
service-buffer pool the RMW sizes. They had SEVEN spellings and no definition:
the count of creation statements, two doc comments, `nros-zpico-build`'s
default-picking comment, and two RMW runtime messages, plus CLAUDE.md. Two had
drifted — both said lifecycle was 6, which is where the widely-quoted "twelve
slots before the application declares anything" came from. It is eleven.

A constant alone would not have caught that: it is still a hand-typed literal,
and a seventh parameter service would leave it stale exactly as the prose was.
So this ties the constant to the CREATION SITES, which is the thing that
actually decides the number.

Python rather than shell, deliberately: `check-gate-selftests`'s call detector
requires parentheses, which a bash function call never has, so a shell gate can
only ever classify as flag-only and land in a baseline that may only shrink.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

SPIN = "packages/core/nros-node/src/executor/spin.rs"
PARAMS = "packages/core/nros-node/src/parameter_services.rs"
LIFECYCLE = "packages/core/nros-node/src/lifecycle_services.rs"

# (label, creation fn, constant, file that must define it)
GROUPS = [
    ("ROS parameter services", "create_param_srv", "PARAM_SERVICE_QUERYABLES", PARAMS),
    ("REP-2002 lifecycle services", "create_lc_srv", "LIFECYCLE_SERVICE_QUERYABLES", LIFECYCLE),
]

# An RMW backend must NOT restate these: it does not depend on `nros-node` and
# can see neither the constants nor whether their features are compiled in. A
# number stated where it cannot be derived is a number that drifts — which is
# how both wrong spellings got there.
RESTATE = re.compile(r"(param|parameter).{0,40}services\s+(use|consume)?\s*\(?\d+\)?", re.I)


def sites(text, creator):
    return len(re.findall(rf"^\s+let [a-z0-9_]+ = {creator}::<", text, re.M))


def declared(text, name):
    m = re.search(rf"^pub const {name}: usize = (\d+);", text, re.M)
    return int(m.group(1)) if m else None


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf8") as fh:
        return fh.read()


MIRROR = re.compile(r"^const (PARAM_SERVICE_QUERYABLES|LIFECYCLE_SERVICE_QUERYABLES): usize = (\d+);", re.M)


def rmw_rust_files(root, rmw_dir):
    """Tracked `.rs` under `rmw_dir`, via the git index rather than a walk.

    `check-no-tracked-file-find` measured 7m36s -> 0.8s for the same paths, and
    it is right: this gate reads every RMW source on the fast line.
    """
    out = subprocess.run(
        ["git", "-C", root, "ls-files", f"{rmw_dir}/*.rs"],
        capture_output=True, text=True,
    )
    if out.returncode == 0:
        return [p for p in out.stdout.split() if "/target/" not in p]
    found = []
    # walk-ok: the self-test builds a synthetic tree that is not a git
    # repository, so there is no index to query. Never reached on the real tree.
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, rmw_dir)):
        dirnames[:] = [d for d in dirnames if d not in ("target", "build")]
        for fn in filenames:
            if fn.endswith(".rs"):
                found.append(os.path.relpath(os.path.join(dirpath, fn), root))
    return found


def check(root, rmw_dir="packages/rmw"):
    """Return a list of problem strings (empty == pass)."""
    problems = []
    try:
        spin = read(root, SPIN)
    except OSError as e:
        return [f"missing {SPIN}: {e}"]

    for label, creator, const, const_rel in GROUPS:
        n = sites(spin, creator)
        try:
            want = declared(read(root, const_rel), const)
        except OSError as e:
            problems.append(f"{const}: cannot read {const_rel}: {e}")
            continue
        if want is None:
            problems.append(
                f"{const} not found in {const_rel} — the count must have exactly "
                f"one definition, beside the code that creates them."
            )
        elif n == 0:
            # Never agree with a constant because the pattern stopped matching:
            # that is a blind gate reporting success.
            problems.append(
                f"no `{creator}::<...>` sites found in {SPIN} — the creation shape "
                f"changed and this gate is now blind. Fix the pattern; do not delete the check."
            )
        elif n != want:
            problems.append(
                f"{label}: {n} `{creator}` site(s) in {SPIN}, but {const} = {want}. "
                f"A service server is a queryable — update {const_rel} so every pool "
                f"sized from it moves too."
            )

    # phase-392 W5.d — `nros-zpico-build` MIRRORS both counts, and cannot do
    # otherwise: it is a build-script helper, so it can neither depend on
    # `nros-node` to read the constants nor see that crate's features. A mirror
    # is acceptable only while something holds it to the definition — that is
    # the whole difference between this and the seven prose spellings it
    # replaced, of which two had drifted.
    definitions = {}
    for _, _, const, const_rel in GROUPS:
        try:
            definitions[const] = declared(read(root, const_rel), const)
        except OSError:
            definitions[const] = None

    for rel in rmw_rust_files(root, rmw_dir):
        full = os.path.join(root, rel)
        try:
            with open(full, encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        for i, line in enumerate(text.split("\n"), 1):
            if RESTATE.search(line):
                problems.append(
                    f"{rel}:{i} states an infrastructure-queryable count. "
                    f"Name the knob and the cause; the counts live beside "
                    f"the code that creates them."
                )
        for m in MIRROR.finditer(text):
            const, value = m.group(1), int(m.group(2))
            want = definitions.get(const)
            if want is None:
                problems.append(
                    f"{rel} mirrors {const}, but the definition could not be read."
                )
            elif value != want:
                problems.append(
                    f"{rel} mirrors {const} = {value}, definition is {want}. "
                    f"A build-script helper cannot read the constant, so the "
                    f"mirror is held here instead (phase-392 W5.d)."
                )
    return problems


def _write(root, n_param, n_lc, c_param, c_lc, rmw_line):
    for rel in (SPIN, PARAMS, LIFECYCLE, "packages/rmw/zenoh/x/src/service.rs"):
        os.makedirs(os.path.join(root, os.path.dirname(rel)), exist_ok=True)
    body = "".join(f"        let h{i} = create_param_srv::<T>(\n" for i in range(n_param))
    body += "".join(f"        let l{i} = create_lc_srv::<T>(\n" for i in range(n_lc))
    open(os.path.join(root, SPIN), "w").write(body)
    open(os.path.join(root, PARAMS), "w").write(
        f"pub const PARAM_SERVICE_QUERYABLES: usize = {c_param};\n")
    open(os.path.join(root, LIFECYCLE), "w").write(
        f"pub const LIFECYCLE_SERVICE_QUERYABLES: usize = {c_lc};\n")
    open(os.path.join(root, "packages/rmw/zenoh/x/src/service.rs"), "w").write(rmw_line + "\n")


def self_test():
    """Every probe asserts a failure this gate must catch, plus the clean case,
    so a gate that stopped matching anything cannot report success."""
    cases = [
        ((6, 5, 6, 5, "// nothing"), 0, "counts agree"),
        ((6, 5, 6, 6, "// nothing"), 1, "lifecycle constant drifted to 6 (the historical error)"),
        ((7, 5, 6, 5, "// nothing"), 1, "a 7th parameter service was added"),
        ((0, 5, 6, 5, "// nothing"), 1, "creation-site pattern stopped matching"),
        ((6, 5, 6, 5, "// parameter services use 6"), 1, "an RMW restated a count"),
        ((6, 5, 6, 5, "const PARAM_SERVICE_QUERYABLES: usize = 6;"), 0,
         "a build-script mirror that agrees"),
        ((6, 5, 6, 5, "const PARAM_SERVICE_QUERYABLES: usize = 7;"), 1,
         "a build-script mirror that drifted"),
    ]
    failures = 0
    tmp = tempfile.mkdtemp()
    try:
        for args, want, label in cases:
            root = os.path.join(tmp, "t")
            shutil.rmtree(root, ignore_errors=True)
            _write(root, *args)
            got = 1 if check(root) else 0
            if got != want:
                sys.stderr.write(f"  self-test FAIL: {label} — got {got}, want {want}\n")
                failures += 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        sys.stderr.write(f"check-infra-queryable-counts self-test: FAILED ({failures})\n")
        sys.exit(1)
    print("check-infra-queryable-counts self-test: OK")


def main():
    # On the NORMAL path, not behind a flag: a negative control nobody runs
    # decays into a comment (`check-gate-selftests`).
    self_test()
    if "--self-test" in sys.argv:
        return
    problems = check(ROOT)
    if problems:
        sys.stderr.write("check-infra-queryable-counts: %d problem(s) — issue 0827:\n" % len(problems))
        for p in problems:
            sys.stderr.write(f"  - {p}\n")
        sys.exit(1)
    for label, creator, const, _ in GROUPS:
        n = sites(read(ROOT, SPIN), creator)
        print(f"  ok    {label}: {n} creation site(s) == {const}")
    print("infra-queryables: counts agree with their creation sites.")


if __name__ == "__main__":
    main()
