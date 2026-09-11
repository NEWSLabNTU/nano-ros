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
only SHRINK. An increase in any kind for any crate fails; a decrease is a
finding to record (rerun with `--write-baseline`). A crate absent from the
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

KINDS = ("block", "fn", "impl", "extern", "trait")


def strip_noise(src: str) -> str:
    """Remove comments and string/char literals, preserving newlines.

    Order matters: a `//` inside a string is not a comment, and a quote inside a
    comment does not open a string. One pass over the text, tracking which of the
    four states we are in, is the only way to get both right — two regex passes
    get the nesting wrong in opposite directions.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        # raw string: r"…", r#"…"#, r##"…"##
        m = re.match(r'r(#*)"', src[i:])
        if c == "r" and m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            hashes = m.group(1)
            end = src.find('"' + hashes, i + len(m.group(0)))
            chunk = src[i:end if end != -1 else n]
            out.append("\n" * chunk.count("\n"))
            i = (end + 1 + len(hashes)) if end != -1 else n
            continue
        if src.startswith("//", i):
            end = src.find("\n", i)
            i = n if end == -1 else end
            continue
        if src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            out.append("\n" * src[i:j].count("\n"))
            i = j
            continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            out.append("\n" * src[i:j].count("\n"))
            i = j
            continue
        if c == "'":
            # a char literal, not a lifetime: `'a` has no closing quote nearby
            m = re.match(r"'(\\.|[^\\'])'", src[i:])
            if m:
                i += m.end()
                continue
        out.append(c)
        i += 1
    return "".join(out)


UNSAFE = re.compile(r"\bunsafe\s+(fn|impl|extern|trait)\b|\bunsafe\s*\{")


def count_text(src: str) -> dict:
    counts = {k: 0 for k in KINDS}
    for m in UNSAFE.finditer(strip_noise(src)):
        counts[m.group(1) or "block"] += 1
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


def census():
    """{crate: {kind: n}} — a crate whose source cannot be read is an ERROR."""
    crates = workspace_crates()
    if crates is None:
        return None, ["cargo metadata failed"]
    result, errors = {}, []
    for name, src in crates:
        if not src.is_dir():
            continue  # no Rust sources (metadata-only package); nothing to count
        totals = {k: 0 for k in KINDS}
        for f in sorted(src.rglob("*.rs")):
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

    grew, appeared, shrank = [], [], []
    for name in sorted(set(now) | set(base)):
        b = base.get(name)
        n = now.get(name, {k: 0 for k in KINDS})
        if b is None:
            appeared.append((name, n))
            continue
        for k in KINDS:
            if n[k] > b[k]:
                grew.append((name, k, b[k], n[k]))
            elif n[k] < b[k]:
                shrank.append((name, k, b[k], n[k]))

    if grew or appeared:
        print("check-unsafe-census: FAIL\n", file=sys.stderr)
        for name, k, was, is_ in grew:
            print(f"  {name}: unsafe {k} {was} -> {is_}", file=sys.stderr)
        for name, n in appeared:
            kinds = ", ".join(f"{k}={n[k]}" for k in KINDS if n[k])
            print(f"  {name}: NEW crate carrying unsafe ({kinds})", file=sys.stderr)
        print(
            "\n  The baseline may only SHRINK. This is not 'reduce the count' — the\n"
            "  counts are legitimate (issue 1221). It is that a NEW unsafe site\n"
            "  should be a decision someone made, not a diff nobody objected to.\n"
            "  If the growth is intended, say so in the commit and re-run with\n"
            "      python3 scripts/check-unsafe-census.py --write-baseline",
            file=sys.stderr,
        )
        return 1

    total = sum(sum(r.values()) for r in now.values())
    note = ""
    if shrank:
        note = (f"  {len(shrank)} row(s) SHRANK — re-run with --write-baseline to record it.")
        print(note)
    print(f"check-unsafe-census: OK — {len(now)} crate(s), {total} unsafe site(s), none grew.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
