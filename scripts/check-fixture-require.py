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
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "packages" / "testing" / "nros-tests"
BASELINE = ROOT / ".config" / "fixture-require-baseline.txt"
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from ratchet import fell_instructions, judge  # noqa: E402  phase-472 W9

# A resolver is `pub fn build_*(..) -> TestResult`. Harvested, never listed, so
# a new resolver is covered the day it is written.
RESOLVER_DECL = re.compile(r"\bfn (build_[a-z0-9_]+)\s*\([^;{]*?\)\s*->\s*TestResult", re.S)
# The handlers that make the decision at the call site instead of delegating.
BYPASS = re.compile(r"\.(expect|unwrap_or_else|unwrap)\s*\(")


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


# Issue 1544 — the SYNTACTIC bypasses. A resolver's `Err` decided by a `match`,
# an `if let`/`while let`, or a `let … else` never names a method at all, so
# BYPASS above cannot see it; 18 `match build_x() { Ok(p) => p, Err(e) =>
# panic!(..) }` sites sat in the tree while the gate read green. Each is keyed on
# the call being the scrutinee DIRECTLY — `match build_x().require(..)` is not
# a bypass, and neither is a `match` on something the resolver's value fed.
MATCH_BEFORE = re.compile(r"\bmatch\s*$")
IF_LET_BEFORE = re.compile(r"\b(?:if|while)\s+let\b[^;{}]*=\s*$")
LET_BEFORE = re.compile(r"\blet\b[^;{}]*=\s*$")
BLOCK_AFTER = re.compile(r"\s*\{")
ELSE_AFTER = re.compile(r"\s*else\s*\{")


def call_close(text, open_paren):
    """Index just past the `)` matching the `(` at `open_paren`."""
    depth = 0
    for i in range(open_paren, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def syntactic_bypass(text, call_start, call_end):
    """Is the call the scrutinee of a match / if-let / let-else?"""
    s = max(text.rfind(";", 0, call_start), text.rfind("{", 0, call_start),
            text.rfind("}", 0, call_start)) + 1
    before = text[s:call_start]
    # issue 1615 (W6): a PATH-QUALIFIED call (`match nros_tests::fixtures::f(..)`)
    # is the same scrutinee — the qualifier sat between `match` and the name.
    before = re.sub(r"(?:(?:::)?[A-Za-z_][A-Za-z0-9_]*\s*::\s*)+$", "", before)
    after = text[call_end:call_end + 80]
    if MATCH_BEFORE.search(before) and BLOCK_AFTER.match(after):
        return "match"
    if IF_LET_BEFORE.search(before) and BLOCK_AFTER.match(after):
        return "if-let"
    if LET_BEFORE.search(before) and not IF_LET_BEFORE.search(before) and ELSE_AFTER.match(after):
        return "let-else"
    return None


def _rel(f):
    try:
        return f.relative_to(ROOT)
    except ValueError:
        return f


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
            ce = call_close(text, m.end() - 1)
            if syntactic_bypass(text, m.start(), ce) or (
                ".require(" not in stmt and BYPASS.search(stmt)
            ):
                found.append(f"{_rel(f)}:{text[: m.start()].count(chr(10)) + 1}")
            pos = max(end, m.end())
    return sorted(found)


# Issue 1620 — the SECOND way to decide absence without the helper, and the one
# this gate could not see: never calling a resolver at all. A site that probes
# the artifact itself —
#
#     if !bin.is_file() { skip!("… not built …") }
#
# — has no `build_*` call to key on, so `sites()` above reports nothing, while
# the skip fires in EVERY run, a gated one included, where an absent in-lane
# fixture is a broken promise (issue 0584 part 2). The census for 1620 found
# six such sites (`ros_env::e2e_setup*` ×3, `borrowed_e2e`, the two
# `zephyr_leaf_staleness` probes) skipping where the shared funnel would have
# failed. Keyed on the SHAPE — an existence probe as the guard of a block whose
# effect is a `skip!` — never on the skip's wording, for the reason the header
# gives. Not every such site is a fixture (a probe for an SDK checkout is a
# capability), so it is a ratchet like the other half: a new one is refused,
# and an existing one either converts to `require_prebuilt_artifact(..)
# .require(..)` or stays at its recorded count.
EXISTENCE_GUARD = re.compile(
    r"\bif\s+!\s*[^{};]*?\.(?:is_file|exists|is_dir)\s*\(\s*\)\s*\{"
)
SKIP_CALL = re.compile(r"\bskip(?:_class)?!\s*\(")
EXISTENCE_BASELINE = ROOT / ".config" / "fixture-existence-skip-baseline.txt"


def existence_skip_sites(files):
    """`if !<path>.is_file()/exists()/is_dir() { … skip!(…) … }` call sites."""
    found = []
    for f in files:
        text = f.read_text(errors="replace")
        for m in EXISTENCE_GUARD.finditer(text):
            # Comment lines are prose about the shape, not the shape.
            line_start = text.rfind("\n", 0, m.start()) + 1
            if text[line_start : m.start()].lstrip().startswith("//"):
                continue
            body_end = statement_end(text, m.end())
            if SKIP_CALL.search(text[m.end() : body_end]):
                found.append(f"{_rel(f)}:{text[: m.start()].count(chr(10)) + 1}")
    return sorted(found)


def per_file_counts(found):
    out = {}
    for s in found:
        k = s.rsplit(":", 1)[0]
        out[k] = out.get(k, 0) + 1
    return out


def read_baseline(path=None):
    """file -> allowed count of unconverted sites."""
    path = path or BASELINE
    out = {}
    try:
        text = path.read_text()
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


def verdict(per_file, base, baseline=None):
    """(rose moves, fall lines) — the per-file ratchet, both directions.

    A file whose count FELL must be recorded in the same change (phase-472 W9):
    before, this printed "(shrink it)" and passed, so the file could regrow to
    its recorded count unobserved.
    """
    baseline = baseline or BASELINE
    rose, fell = judge(per_file, base)
    lines = fell_instructions(
        fell, str(baseline.relative_to(ROOT)),
        lambda f, n: f"{n} {f}" if n else None,
        "python3 scripts/check-fixture-require.py --write-baseline") if fell else []
    return rose, lines


def self_test():
    """Negative controls: the matcher must answer BOTH ways.

    Through `sites()` itself, on real files — the first version of this ran a
    private line-window copy of the matcher (issue 1544), so a change to the
    statement-scoped scan the gate actually runs was never under test.
    """
    cases = [
        ("build_native_talker()\n    .expect(\"x\")", True),
        ("build_native_talker()\n    .unwrap_or_else(|e| panic!(\"{e}\"))", True),
        ("build_native_talker().require(\"native talker\")", False),
        ("build_native_talker()?", False),
        ("some_other_call().expect(\"x\")", False),
        # issue 1544 — the syntactic bypasses.
        ("match build_native_talker() {\n    Ok(p) => p,\n    Err(e) => panic!(\"{e}\"),\n}", True),
        ("if let Ok(p) = build_native_talker() {\n    run(p);\n}", True),
        ("let Ok(p) = build_native_talker() else {\n    panic!(\"x\")\n}", True),
        ("match build_native_talker().require(\"t\").extension() {\n    _ => {}\n}", False),
        ("let p = build_native_talker().require(\"t\");\nlet Some(x) = p.parent() else {\n    panic!()\n}", False),
        ("if let Some(x) = probe() {\n    let p = build_native_talker().require(\"t\");\n}", False),
        # A statement-scoped scan must not reach past its own `;`.
        ("let p = build_native_talker()?;\nCommand::new(p).output().expect(\"x\")", False),
    ]
    names = {"build_native_talker"}
    bad = 0
    with tempfile.TemporaryDirectory() as td:
        for k, (text, want) in enumerate(cases):
            f = Path(td) / f"case{k}.rs"
            f.write_text("fn t() {\n    " + text + ";\n}\n")
            got = bool(sites([f], names))
            if got != want:
                print(f"  self-test FAIL: {text!r} -> {got}, want {want}")
                bad += 1
    # Issue 1620 — the existence-probe half, both directions, through the
    # function `main` runs.
    existence_cases = [
        ('if !bin.is_file() {\n    skip!("fixture not built");\n}', True),
        ('if !dir.join(".compile-ok").exists() {\n    nros_tests::skip!("x");\n}', True),
        ('if !root.is_dir() {\n    nros_tests::skip_class!(lane, "x");\n}', True),
        # a probe whose block does not skip is not a decision about absence
        ('if !conf.is_file() {\n    panic!("missing");\n}', False),
        # a skip guarded by something that is not an existence probe
        ('if !nros_tests::process::require_cmake() {\n    skip!("cmake");\n}', False),
        # the converted form
        ('let b = require_prebuilt_artifact(&p, "r").require("w");', False),
        # a comment describing the shape
        ('// if !bin.is_file() { skip!(..) } was the old shape\nlet x = 1;', False),
    ]
    with tempfile.TemporaryDirectory() as td:
        for k, (text, want) in enumerate(existence_cases):
            f = Path(td) / f"exist{k}.rs"
            f.write_text("fn t() {\n    " + text + "\n}\n")
            got = bool(existence_skip_sites([f]))
            if got != want:
                print(f"  self-test FAIL (existence): {text!r} -> {got}, want {want}")
                bad += 1
    cases = cases + existence_cases
    # The ratchet, through `verdict` — the function `main` runs.
    for desc, cur, base, want_rose, want_fell in (
        ("at its count", {"a": 2}, {"a": 2}, False, False),
        ("a rise", {"a": 3}, {"a": 2}, True, False),
        ("an unrecorded fall", {"a": 1}, {"a": 2}, False, True),
        ("a converted file still listed", {}, {"a": 2}, False, True),
    ):
        rose, fell_lines = verdict(cur, base)
        if bool(rose) != want_rose or bool(fell_lines) != want_fell:
            print(f"  self-test FAIL: ratchet: {desc}")
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
    per_file = per_file_counts(current)
    # Issue 1620 — the half that calls no resolver at all.
    existence = existence_skip_sites(files)
    existence_per_file = per_file_counts(existence)

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
        EXISTENCE_BASELINE.write_text(
            "# issue 1620 — how many sites in each file guard a `skip!` with their\n"
            "# OWN existence probe (`if !p.is_file() { skip!(..) }`) instead of\n"
            "# asking the shared absence funnel\n"
            "# (`require_prebuilt_artifact(..).require(..)` or a `build_*`\n"
            "# resolver). Such a skip fires in a gated run too, where an absent\n"
            "# in-lane fixture must FAIL (issue 0584). Some are genuine capability\n"
            "# probes (an SDK checkout, a host tool); the count only SHRINKS.\n"
            "#\n"
            "# Generated by `python3 scripts/check-fixture-require.py"
            " --write-baseline`.\n"
            + "".join(f"{n} {f}\n" for f, n in sorted(existence_per_file.items()))
        )
        print(f"check-fixture-require: baselines written — {len(current)} resolver "
              f"site(s) in {len(per_file)} file(s), {len(names)} resolver(s); "
              f"{len(existence)} existence-probe skip(s) in "
              f"{len(existence_per_file)} file(s).")
        return 0

    rc = 0
    rose, fell_lines = verdict(per_file, read_baseline())
    grew = [(m.key, m.now, m.was) for m in rose]
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
        rc = 1
    if fell_lines:
        print("check-fixture-require: FAIL — the baseline records more unconverted "
              "sites than the tree has:")
        print("\n".join(fell_lines))
        rc = 1

    e_rose, e_fell_lines = verdict(
        existence_per_file, read_baseline(EXISTENCE_BASELINE), EXISTENCE_BASELINE
    )
    if e_rose:
        print("check-fixture-require: FAIL — "
              f"{len(e_rose)} file(s) guard a `skip!` with their own existence "
              "probe more often than the baseline allows:")
        rose_keys = {m.key for m in e_rose}
        for site in existence:
            if site.rsplit(":", 1)[0] in rose_keys:
                print(f"  {site}")
        print()
        print("  `if !p.is_file() { skip!(..) }` skips in EVERY run — including a")
        print("  gated one, where an absent in-lane fixture is a broken promise and")
        print("  must fail (issue 0584). Ask the funnel that knows the lane:")
        print('      let p = nros_tests::fixtures::require_prebuilt_artifact(&p, "<build cmd>")')
        print('          .require("what it is");')
        print("  or the row's `build_*` resolver. A probe for something that is NOT")
        print("  a build-stage artifact (an SDK checkout, a host tool) is a capability")
        print("  skip — name the tool, and use a probe that is not a path existence test.")
        rc = 1
    if e_fell_lines:
        print("check-fixture-require: FAIL — the existence-probe baseline records "
              "more sites than the tree has:")
        print("\n".join(e_fell_lines))
        rc = 1
    if rc:
        return rc

    print(f"check-fixture-require: OK — {len(names)} resolver(s), "
          f"{len(current)} unconverted resolver site(s), {len(existence)} "
          "existence-probe skip(s), every file at its recorded count")
    return 0


if __name__ == "__main__":
    sys.exit(main())
