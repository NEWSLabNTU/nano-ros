#!/usr/bin/env python3
"""issue 1129 / phase-450 W1 — a fixture-resolver `Err` is converted in ONE place.

THE RULE
========

Every `build_*` resolver in `nros-tests` returns `TestResult<T>`. Converting its
`Err` into a test outcome is ONE decision — skip when the fixture was never
built, fail otherwise — and it belongs in
`nros_tests::fixtures::require::RequireFixture::require`.

WHY NOT A REGEX OVER THE MESSAGES
=================================

Because that is what was there, and it is what this gate replaces.
`check-skip-budget` could only key on the WORDING a call site chose, so it
grepped `not prebuilt` and saw 4 of 61 sites; 57 said `not built`. The phase doc
named the trap before the fix was written: *"the one that must not be fixed by
adding a second spelling to the matcher."* Widening the regex leaves a 62nd
wording free to reopen it.

Keying on the CALL instead has the property a wording never can: a site that
bypasses the helper is the thing that fails, whatever it says.

THE BASELINE IS A PER-FILE RATCHET, AND ONLY SHRINKS
===================================================

260 sites predate the helper. Blocking on all of them at once would mean this
rule lands as one unreviewable diff or not at all, so the unconverted sites are
counted PER FILE, each count may only fall, and a file that grows one is
refused. That is the same shape as the tree's other ratchets, for the same
reason: a rule nobody can adopt incrementally is a rule that does not land.

It counts rather than listing `file:line` because the first version listed
them, and converting a site SHIFTS every line under it — so the ratchet went
red on the very edit it exists to reward. A baseline that a correct change
invalidates is a baseline nobody can move.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "packages" / "testing" / "nros-tests"
BASELINE = ROOT / ".config" / "fixture-require-baseline.txt"

# A resolver is `pub fn build_*(..) -> TestResult`. Harvested, never listed, so
# a new resolver is covered the day it is written.
RESOLVER_DECL = re.compile(r"\bfn (build_[a-z0-9_]+)\s*\([^;{]*?\)\s*->\s*TestResult", re.S)
# The handlers that make the decision at the call site instead of delegating.
BYPASS = re.compile(r"\.(expect|unwrap_or_else|unwrap)\s*\(")
LOOKAHEAD = 4


def tracked_rs():
    """Every tracked `.rs` under `nros-tests`, from the INDEX not a walk.

    `rglob` was the first spelling here and `check-no-tracked-file-find` caught
    it — the same defect phase-450 W5 had already fixed in the unsafe census,
    reproduced in the gate written to finish the same phase. A walk also reads
    whatever `target/` happens to contain, so it is wrong twice: slow, and not
    a question about the repository.
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", str(TESTS.relative_to(ROOT)) + "/**/*.rs"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        print("check-fixture-require: `git ls-files` failed:", file=sys.stderr)
        print(out.stderr.strip()[:400], file=sys.stderr)
        return None
    return [ROOT / p for p in out.stdout.split("\0") if p]


def resolvers(files):
    out = set()
    for f in files:
        out.update(RESOLVER_DECL.findall(f.read_text(errors="replace")))
    return out


def statement_end(text, start):
    """Index just past the statement whose resolver call opened at `start`.

    Scoped to the STATEMENT, not to a fixed window of lines. A window is what
    this had first, and it reported `build_entry_poc()?` as a bypassing site
    because an unrelated `Command::new(bin).output().expect(...)` sat two lines
    below it — a gate whose reach is wider than its rule, which is the same
    defect in the other direction from the one this phase collects.

    Begins at depth 1: the caller matched the resolver's OPENING paren.
    """
    depth = 1
    i = start
    while i < len(text):
        c = text[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth < 0:
                return i
        elif c == ";" and depth == 0:
            return i
        i += 1
    return len(text)


def sites(files, names):
    """Call sites of a resolver whose `Err` is handled without the helper."""
    if not names:
        return []
    call = re.compile(r"\b(" + "|".join(sorted(map(re.escape, names))) + r")\s*\(")
    decl = re.compile(r"\bfn\s+$")
    found = []
    for f in files:
        text = f.read_text(errors="replace")
        pos = 0
        while True:
            m = call.search(text, pos)
            if not m:
                break
            # A DECLARATION is not a call site.
            if decl.search(text[: m.start()]):
                pos = m.end()
                continue
            end = statement_end(text, m.end())
            stmt = text[m.start() : end]
            if ".require(" not in stmt and BYPASS.search(stmt):
                found.append(f"{f.relative_to(ROOT)}:{text[: m.start()].count(chr(10)) + 1}")
            pos = max(end, m.end())
    return sorted(found)


def read_baseline():
    """file -> allowed count of unconverted sites."""
    out = {}
    try:
        text = BASELINE.read_text()
    except OSError:
        # MISSING means nothing is allowed, not everything. A ratchet whose
        # absent file means "allow all" has stopped ratcheting.
        return out
    for ln in text.split("\n"):
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        count, _, path = ln.partition(" ")
        out[path.strip()] = int(count)
    return out


def self_test():
    """Negative controls: the matcher must answer BOTH ways."""
    cases = [
        ("build_native_talker()\n    .expect(\"x\")", True),
        ("build_native_talker()\n    .unwrap_or_else(|e| panic!(\"{e}\"))", True),
        ("build_native_talker().require(\"native talker\")", False),
        ("build_native_talker()?", False),
        ("some_other_call().expect(\"x\")", False),
    ]
    names = {"build_native_talker"}
    call = re.compile(r"\b(" + "|".join(names) + r")\s*\(")
    bad = 0
    for text, want in cases:
        lines = text.split("\n")
        got = False
        for i, line in enumerate(lines):
            if not call.search(line):
                continue
            window = "\n".join(lines[i : i + LOOKAHEAD])
            if ".require(" in window:
                continue
            if BYPASS.search(window):
                got = True
        if got != want:
            print(f"  self-test FAIL: {text!r} -> {got}, want {want}")
            bad += 1
    print(f"check-fixture-require self-test: {'OK' if not bad else 'FAILED'} "
          f"({len(cases)} cases)")
    return bad


def main():
    if self_test():
        return 1
    files = tracked_rs()
    if files is None:
        return 1
    names = resolvers(files)
    current = sites(files, names)

    per_file = {}
    for s in current:
        per_file[s.rsplit(":", 1)[0]] = per_file.get(s.rsplit(":", 1)[0], 0) + 1
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(
            "# issue 1129 / phase-450 W1 — how many call sites in each file still\n"
            "# decide a fixture-resolver `Err` themselves instead of calling\n"
            "# `RequireFixture::require`. SHRINK ONLY, per file.\n"
            "#\n"
            "# Generated by `python3 scripts/check-fixture-require.py"
            " --write-baseline`.\n"
            "# Counts, not `file:line`: converting a site shifts every line under\n"
            "# it, so a line-keyed baseline went red on the edit it rewards.\n"
            + "".join(f"{n} {f}\n" for f, n in sorted(per_file.items()))
        )
        print(f"check-fixture-require: baseline written — {len(current)} site(s) "
              f"in {len(per_file)} file(s), {len(names)} resolver(s).")
        return 0

    base = read_baseline()
    grew = []
    for f, n in sorted(per_file.items()):
        allowed = base.get(f, 0)
        if n > allowed:
            grew.append((f, n, allowed))

    if grew:
        print("check-fixture-require: FAIL — "
              f"{len(grew)} file(s) decide a fixture-resolver `Err` themselves "
              "more often than the baseline allows:")
        for f, n, allowed in grew[:40]:
            print(f"  {f}: {n} site(s), baseline {allowed}")
        print()
        print("  A `build_*` resolver returns `TestResult`, and what its `Err`")
        print("  MEANS is one decision, not one per call site. Write")
        print('      let bin = build_x().require("what it is");')
        print("  `require` skips a `FixtureNotBuilt` and panics on anything else,")
        print("  which is the rule 260 sites used to each restate in prose.")
        return 1

    total_base = sum(base.values())
    print(f"check-fixture-require: OK — {len(names)} resolver(s), "
          f"{len(current)} unconverted site(s), baseline {total_base}"
          + (" (shrink it)" if len(current) < total_base else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
