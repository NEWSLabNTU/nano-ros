#!/usr/bin/env python3
"""issue 1236 / phase-450 W5 — the tree's `unsafe` has a DIRECTION, and nothing recorded it.

Issue 1221 put `#![forbid(unsafe_code)]` on the ten shipped crates measured at
literal zero. `forbid` is the right instrument for that population and a very
poor one for the rest: it is a PROPERTY, not a budget, so it says nothing about
the crates that legitimately carry unsafe — which is where all of it is.

Measured 2026-09-08 (issue 1236): **5,047** occurrences across the **62** crates
that are not at zero. None of that is a defect. The arena exists to avoid
`alloc`, which is the point of `nros-node`; the C and C++ API crates exist to be
an ABI. What was missing is direction — a `String::from_utf8_unchecked` added to
`nros-rmw-zenoh` next month is a normal-looking diff that nothing objects to,
and there was no record of whether the tree's unsafe grows, shrinks, or moves
between crates.

So: a census and a ratchet. Not a proposal to reduce the count.

## Two things 1236 required, both of which this repo has been bitten by

**The count is about CODE, not spelling.** A `grep` for the token is satisfied
by a rename and counts comments and doc examples. This strips comments and
string/char literals first, then counts by SYNTACTIC KIND — `unsafe {}` blocks,
`unsafe fn`, `unsafe impl`, `unsafe extern`, `unsafe trait` — reported
separately. 1221's own table had to say "occurrences of the token" precisely
because the two differ.

The limits of that, stated rather than implied: this is a lexical scan, not a
Rust parser. It strips `//`, `/* */` (nested), `"…"`, `r#"…"#` and `'…'`, and
then matches `unsafe` only where the next token is one of the five above or `{`.
It will not be confused by the token in prose or a string, and it does not
attempt to resolve `cfg`. A crate whose source it cannot read is a reported
FAILURE, never an absence — the reach must equal the rule (issue 0196's shape,
which four gates in this tree have failed).

**The crate list is ENUMERATED, never authored.** It comes from
`cargo metadata --no-deps`, so a crate added tomorrow is in the census the day
it lands. An authored list drifts the moment someone adds a crate, which is the
failure this gate would otherwise reproduce.

## The ratchet

`.config/unsafe-census-baseline.txt`, one row per crate per kind, and it may
only SHRINK. An increase in any kind for any crate fails; so does a decrease
the baseline does not yet record — it must be recorded in the change that made
it (rerun with `--write-baseline`), or the slack lets the crate regrow to its
old count unobserved (phase-472 W9, `scripts/lib/ratchet.py`). A crate absent from the
baseline may not carry unsafe at all — that is what makes a NEW crate's unsafe
visible rather than grandfathered.

Run:  python3 scripts/check-unsafe-census.py [--self-test] [--show] [--write-baseline]
"""
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE = REPO / ".config" / "unsafe-census-baseline.txt"
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from ratchet import Move, fell_instructions, judge  # noqa: E402  phase-472 W9

KINDS = ("block", "fn", "impl", "extern", "trait")


# One pass, and it never builds a stripped copy of the file. The alternatives
# were measured over this tree, all reporting the identical 73 crates / 4,411
# sites: a character loop that returns a cleaned string took 14.5s; adding an
# `"unsafe" not in src` early-out took 9.7s; this takes 0.3s. The cost was never
# the walk — it was rebuilding every file as a Python string.
#
# The alternation ORDER is the correctness of it: a `//` inside a string is not
# a comment and a quote inside a comment does not open a string, so whichever
# construct STARTS first must win, which is what a single alternation gives.
# Block comments nest in Rust, so they are counted rather than matched.
SCAN = re.compile(
    r"""(?P<line>//[^\n]*)
      | (?P<open>/\*) | (?P<close>\*/)
      | (?P<raw>r\#*")
      | (?P<str>")
      | (?P<chr>'(?:\\.|[^\\'])')
      | (?P<unsafe>\bunsafe\s+(?:fn|impl|extern|trait)\b|\bunsafe\s*\{)
    """,
    re.X,
)
KIND_OF = re.compile(r"\bunsafe\s+(fn|impl|extern|trait)\b")


def count_text(src: str) -> dict:
    counts = {k: 0 for k in KINDS}
    if "unsafe" not in src:
        return counts
    depth = 0          # block-comment nesting
    i, n = 0, len(src)
    while i < n:
        m = SCAN.search(src, i)
        if not m:
            break
        i = m.end()
        if depth:
            if m.lastgroup == "open":
                depth += 1
            elif m.lastgroup == "close":
                depth -= 1
            continue
        g = m.lastgroup
        if g == "open":
            depth = 1
        elif g in ("line", "chr", "close"):
            continue
        elif g == "raw":
            end = src.find('"' + "#" * (len(m.group("raw")) - 2), i)
            i = n if end == -1 else end + len(m.group("raw")) - 1
        elif g == "str":
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    break
                i += 1
        elif g == "unsafe":
            k = KIND_OF.match(m.group("unsafe"))
            counts[k.group(1) if k else "block"] += 1
    return counts


def workspace_crates():
    """(name, src_dir) for every TRACKED crate — enumerated, never authored.

    From `git ls-files`, not `cargo metadata`. That is deliberate and it is the
    difference between this gate and a narrower one: `cargo metadata --no-deps`
    reports only workspace MEMBERS, and this tree excludes ~130 paths from the
    root workspace (issue 1217), several of which are real crates carrying real
    unsafe — `nros-platform-mps2-an385`, `nros-board-esp32-qemu` and the rest of
    issue 1309's set are reached by no lane at all. A census built on
    `cargo metadata` would inherit exactly that blind spot and report a smaller,
    cleaner number than the truth.

    Measured while writing this: members-only saw 37 crates; tracked manifests
    see the full set. The reach has to equal the rule (issue 0196).
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "packages/*/Cargo.toml", "packages/*/*/Cargo.toml",
         "packages/*/*/*/Cargo.toml"],
        cwd=REPO, capture_output=True, text=True,
    )
    if out.returncode != 0:
        print("check-unsafe-census: `git ls-files` failed:", file=sys.stderr)
        print(out.stderr.strip()[:600], file=sys.stderr)
        return None
    crates = []
    for rel in (p for p in out.stdout.split("\0") if p):
        manifest = REPO / rel
        try:
            text = manifest.read_text(encoding="utf-8")
        except OSError:
            continue
        m = re.search(r'^\s*name\s*=\s*"([^"]+)"', text, re.M)
        if not m:
            continue          # a `[workspace]`-only manifest declares no crate
        crates.append((m.group(1), manifest.parent / "src"))
    # A generated tree can produce two manifests with one name; count each path
    # once and let the name collide loudly rather than silently summing.
    return sorted(set(crates))


def tracked_rs_by_dir():
    """{src_dir: [files]} for every tracked `.rs`, from the INDEX.

    Not `Path.rglob`: `check-no-tracked-file-find` forbids a filesystem walk to
    locate git-tracked files, and the measurement behind that rule is 7m36s
    against 0.8s for the same paths — `find` stats every directory it considers
    pruning. This gate broke that rule on its first run and the gate caught it.
    """
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.rs"], cwd=REPO, capture_output=True, text=True,
    )
    if out.returncode != 0:
        return None
    by_dir = {}
    for rel in (p for p in out.stdout.split("\0") if p):
        by_dir.setdefault(str((REPO / rel).parent), []).append(REPO / rel)
    return by_dir


def changed_crates(crates):
    """Crate names whose sources THIS BRANCH changed, or None if unknowable.

    Measured on PR #947: a shrink-only ratchet over a counter that every crate
    can move cannot be judged against an ABSOLUTE total, because the total is
    not a property of the pull request. A merge
    group builds `main` + the PR, so a concurrently-merging PR that adds one
    `unsafe` block ejects THIS one — and the queue's ALLGREEN strategy ejects
    everything behind it too. #947 was ejected exactly that way
    (`nros-rmw-zenoh: unsafe block 109 -> 110`) by a commit it does not
    contain, after four re-baselines in one day that were each obsolete before
    CI finished.

    The rule the gate states is "a NEW unsafe site should be a DECISION SOMEONE
    MADE". The person who made it is the one whose change touches the crate, so
    that is what this measures. Growth in a crate the branch never touched is
    reported and recorded, not blamed.

    Returns None when the comparison point cannot be established (no
    `origin/main`, a shallow clone with no merge base). Then every crate is
    enforced: failing CLOSED is the safe direction for a ratchet, and the
    alternative — treating "I could not tell" as "nothing changed" — is how a
    ratchet silently stops ratcheting.
    """
    base = subprocess.run(
        ["git", "merge-base", "origin/main", "HEAD"],
        cwd=REPO, capture_output=True, text=True,
    )
    if base.returncode != 0 or not base.stdout.strip():
        return None
    diff = subprocess.run(
        ["git", "diff", "--name-only", base.stdout.strip(), "--"],
        cwd=REPO, capture_output=True, text=True,
    )
    if diff.returncode != 0:
        return None
    touched = [ln.strip() for ln in diff.stdout.split("\n") if ln.strip()]
    names = set()
    for name, src in crates:
        prefix = str(src.relative_to(REPO)) if src.is_absolute() else str(src)
        for path in touched:
            if path == prefix or path.startswith(prefix + "/"):
                names.add(name)
                break
    return names


def census():
    """{crate: {kind: n}} — a crate whose source cannot be read is an ERROR."""
    crates = workspace_crates()
    if crates is None:
        return None, ["crate enumeration failed"]
    tracked = tracked_rs_by_dir()
    if tracked is None:
        return None, ["`git ls-files *.rs` failed"]
    result, errors = {}, []
    for name, src in crates:
        if not src.is_dir():
            continue  # no Rust sources (metadata-only package); nothing to count
        # every tracked `.rs` at or under this crate's `src/`
        prefix = str(src)
        files = [f for d, fs in tracked.items()
                 if d == prefix or d.startswith(prefix + "/")
                 for f in fs]
        totals = {k: 0 for k in KINDS}
        for f in sorted(files):
            try:
                text = f.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError) as e:
                errors.append(f"{name}: cannot read {f.relative_to(REPO)}: {e}")
                continue
            for k, v in count_text(text).items():
                totals[k] += v
        if any(totals.values()):
            result[name] = totals
    return result, errors


def read_baseline():
    rows = {}
    if not BASELINE.is_file():
        return rows
    for line in BASELINE.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != len(KINDS) + 1:
            continue
        rows[parts[0]] = dict(zip(KINDS, (int(x) for x in parts[1:])))
    return rows


def write_baseline(now):
    width = max((len(n) for n in now), default=10) + 2
    lines = [
        "# issue 1236 / phase-450 W5 — the per-crate `unsafe` census. SHRINK ONLY.",
        "#",
        "# Generated by `python3 scripts/check-unsafe-census.py --write-baseline`.",
        "# Never hand-edit: the point is that the numbers are MEASURED, and a",
        "# hand-edited row is a number nobody measured.",
        "#",
        "# Columns: crate  " + "  ".join(KINDS),
        "#",
        "# These counts are not defects. `nros-node`'s arena exists to avoid",
        "# `alloc`; the C and C++ API crates exist to BE an ABI. The ratchet",
        "# records DIRECTION, which is what `forbid(unsafe_code)` cannot do for a",
        "# crate that legitimately carries unsafe (issue 1221).",
        "",
    ]
    for name in sorted(now):
        row = now[name]
        lines.append(name.ljust(width) + "  ".join(str(row[k]).rjust(len(k)) for k in KINDS))
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _row_text(name, row):
    return name + "  " + "  ".join(str(row.get(k, 0)) for k in KINDS)


def verdict(now, base, mine):
    """(failure lines, inherited-note lines) for a census against its baseline.

    `mine` is the set of crates this branch touched (None: unknowable, so every
    crate is judged — failing CLOSED). BOTH directions are judged on the shared
    ratchet (`scripts/lib/ratchet.py`): a count that grew fails, and so does one
    that FELL while the baseline still records the old value — otherwise a crate
    at 3 against a row of 5 can regrow two sites with no gate objecting
    (phase-472 W9). A fall is judged in the same scope as a rise: a crate this
    branch never touched cannot have moved because of it.
    """
    flat = lambda d: {(c, k): v[k] for c, v in d.items() for k in KINDS}  # noqa: E731
    rose, fell = judge(flat(now), flat(base))
    blamed = (lambda c: True) if mine is None else (lambda c: c in mine)
    fails, inherited = [], []
    for m in rose:
        crate, kind = m.key
        if m.was == 0 and crate not in base:
            line = f"  {crate}: NEW crate carrying unsafe ({kind}={m.now})"
        else:
            line = f"  {crate}: unsafe {kind} {m.was} -> {m.now}"
        (fails if blamed(crate) else inherited).append(line)
    fell_mine = [m for m in fell if blamed(m.key[0])]
    if fell_mine:
        # One edit per CRATE ROW, which is the unit the baseline file holds.
        crates = sorted({m.key[0] for m in fell_mine})
        # `spell(crate, 1)` is the recorded row, `spell(crate, 0)` the row as
        # it must become (None: the crate carries no unsafe now — delete it).
        rows = [Move(c, 1, 0) for c in crates]
        spell = lambda c, n: (_row_text(c, base[c]) if n else  # noqa: E731
                              (_row_text(c, now[c]) if c in now else None))
        fails.extend(fell_instructions(
            rows, str(BASELINE.relative_to(REPO)), spell,
            "python3 scripts/check-unsafe-census.py --write-baseline"))
    return fails, inherited


def self_test():
    """A planted `unsafe` the census does not report is what makes this decoration."""
    failures = 0
    cases = [
        ("unsafe { *p }", {"block": 1}),
        ("unsafe fn f() {}", {"fn": 1}),
        ("unsafe impl Send for X {}", {"impl": 1}),
        ("unsafe extern \"C\" { fn g(); }", {"extern": 1}),
        ("unsafe trait T {}", {"trait": 1}),
        ("unsafe   {\n}", {"block": 1}),
        # NOT counted: the token in prose, in a string, or in a raw string.
        ("// unsafe { }\nlet x = 1;", {}),
        ("/* unsafe fn f() {} */", {}),
        ("let s = \"unsafe { }\";", {}),
        ("let s = r#\"unsafe fn f(){}\"#;", {}),
        ("//! `unsafe impl` in a doc comment", {}),
        # a lifetime must not eat the rest of the file as a char literal
        ("fn f<'a>(x: &'a str) -> &'a str { x }\nunsafe { }", {"block": 1}),
        # nested block comment
        ("/* a /* b unsafe fn */ c */ unsafe impl X {}", {"impl": 1}),
    ]
    for src, want in cases:
        got = {k: v for k, v in count_text(src).items() if v}
        if got != want:
            print(f"  self-test FAIL: {src!r} -> {got}, want {want}", file=sys.stderr)
            failures += 1
    # The ratchet, both directions, through `verdict` — the function `main`
    # runs. The fall cases are the phase-472 W9 hole: before, a crate at 3
    # against a row of 5 printed OK and could regrow to 5.
    z = {k: 0 for k in KINDS}
    row = lambda **kw: dict(z, **kw)  # noqa: E731
    ratchet = [
        ("unchanged passes", {"a": row(block=5)}, {"a": row(block=5)}, {"a"}, False),
        ("growth in a touched crate fails", {"a": row(block=6)}, {"a": row(block=5)}, {"a"}, True),
        ("a new crate carrying unsafe fails", {"b": row(fn=1)}, {}, {"b"}, True),
        ("an unrecorded FALL in a touched crate fails",
         {"a": row(block=3)}, {"a": row(block=5)}, {"a"}, True),
        ("a crate that lost ALL its unsafe but keeps a row fails",
         {}, {"a": row(block=5)}, {"a"}, True),
        ("with no merge base every crate is judged",
         {"a": row(block=3)}, {"a": row(block=5)}, None, True),
        ("an untouched crate's move is inherited, not blamed",
         {"a": row(block=6)}, {"a": row(block=5)}, set(), False),
    ]
    for desc, now_, base_, mine_, want in ratchet:
        got = bool(verdict(now_, base_, mine_)[0])
        if got != want:
            print(f"  self-test FAIL: ratchet: {desc}: failed={got}, want {want}",
                  file=sys.stderr)
            failures += 1
    cases = cases + ratchet
    if failures:
        print(f"check-unsafe-census self-test: {failures} case(s) FAILED", file=sys.stderr)
        return 1
    print(f"check-unsafe-census self-test: OK ({len(cases)} cases)")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test() != 0:
        return 1

    now, errors = census()
    if errors:
        print("check-unsafe-census: could not measure every crate:", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        print(
            "\n  A crate this cannot read is a FAILURE, not an absence — a census\n"
            "  whose reach is narrower than its rule is the shape issue 0196\n"
            "  describes and phase-450 collects.", file=sys.stderr,
        )
        return 1

    if "--write-baseline" in sys.argv:
        write_baseline(now)
        total = sum(sum(r.values()) for r in now.values())
        print(f"check-unsafe-census: baseline written — {len(now)} crate(s), {total} site(s).")
        return 0

    base = read_baseline()
    if "--show" in sys.argv:
        for name in sorted(now):
            print(f"  {name:44} " + "  ".join(f"{k}={now[name][k]}" for k in KINDS))

    mine = changed_crates(workspace_crates() or [])
    scope = ("every crate (no merge base with origin/main — failing closed)"
             if mine is None else f"{len(mine)} crate(s) this branch touched")
    fails, inherited = verdict(now, base, mine)

    # Growth in a crate this branch never touched is INHERITED — see
    # `changed_crates`. It is still recorded, and the baseline still has to move,
    # but it is not this branch's decision and must not fail it.
    if inherited:
        print(
            f"check-unsafe-census: {len(inherited)} row(s) moved in "
            "crates this branch did not touch — inherited from `main`, not blamed here:"
        )
        for line in inherited:
            print("  " + line)
        print("  Record them with --write-baseline; the decision was made in the "
              "commit that made it.")

    if fails:
        print("check-unsafe-census: FAIL\n", file=sys.stderr)
        for line in fails:
            print(line, file=sys.stderr)
        print(
            "\n  The baseline records DIRECTION. This is not 'reduce the count' — the\n"
            "  counts are legitimate (issue 1221). It is that a NEW unsafe site\n"
            "  should be a decision someone made, not a diff nobody objected to,\n"
            "  and that a REMOVED one is locked in by the change that removed it.\n"
            "  If a growth is intended, say so in the commit and re-run with\n"
            "      python3 scripts/check-unsafe-census.py --write-baseline",
            file=sys.stderr,
        )
        return 1

    total = sum(sum(r.values()) for r in now.values())
    print(f"check-unsafe-census: OK — {len(now)} crate(s), {total} unsafe site(s), "
          f"every row at its recorded count in {scope}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
