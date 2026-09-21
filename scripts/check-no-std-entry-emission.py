#!/usr/bin/env python3
"""No `std::` path in Rust ENTRY code an nros producer emits.

Issue 1381. `nros::main!(panic = "own")` in
`packages/testing/nros-tests/bins/qemu-baremetal-main-e2e` — a leaf whose first
two attributes are `#![no_std]` / `#![no_main]` — expanded to nine `::std::`
paths, and a plain `cargo build --release` in that leaf answered with two
`error[E0433]: cannot find std in the crate root` pointing at the macro call.

## Why the existing guard was not one

Every one of those paths sat under
`#[cfg(not(any(target_os = "none", target_os = "nuttx")))]`, which reads as
"only on a hosted target". That is a true statement about the TARGET and the
wrong question. `#![no_std]` is a property of the CRATE and is orthogonal to the
target OS: a bare-metal leaf built with no `--target` compiles for the HOST,
takes the hosted arm, and still has no `std` in its crate root. There is no cfg
predicate for "this crate has std" — `feature = "std"` is a claim about the
`nros` dependency, not about the entry — so the check cannot be moved into the
emitted code. It has to be a decision the EMITTER makes.

`check-no-std-stdio` is this rule's sibling, and it says so itself: "A proc
macro always runs on the host and links libstd by definition. Its EMITTED
`::std::println!` lands in the caller's crate and is that crate's business,
checked there." 1381 is the hole in "checked there" — the caller's source is one
line, `nros::main!(…)`, so a source-scanning gate sees nothing. This gate scans
the producer instead. Deliberately a mirror and not an extension: that rule is
about libstd STDIO on Zephyr native_sim (issue 0589) and holds for authored
crate source; this one is about ANY `std` path and holds for emitted tokens.

## The rule

In a producer of Rust entry code, a `std::` / `::std::` path may appear only
where the producer has already established that the entry links `std` — which
in this tree means one function, named in `HOSTED_ONLY_EMITTERS` below, whose
sole caller passes `nros_orchestration_ir::board_entry_links_std(board)`.

For the proc-macro the scan is EMISSION-SCOPED: only text inside a `quote!` /
`quote_spanned!` block counts. `main_macro.rs` reads `Cargo.toml`, `system.toml`
and the environment, so it names `std::fs` / `std::env` dozens of times in its
own body; that code runs on the host and is not the hazard.

For a template pack every `std::` counts, because a template is nothing but
emitted text.

## Exemptions

None by comment. A site that is REAL but not yet fixed goes in `KNOWN_OPEN`
with a TRACKED ISSUE ID — the discipline CLAUDE.md records for the rmw-parity
map, and for the same reason: an exemption without an id is indistinguishable
from a decision nobody made. A `KNOWN_OPEN` row whose file no longer has a
finding FAILS the gate, so the row cannot outlive the defect it describes.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Any `std` path: `std::x`, `::std::x`. NOT `nros_std::`, `my_std::` — the
# lookbehind refuses an identifier character before the token, and `::std` is
# matched as a whole so the `::` prefix is not mistaken for a longer path's
# separator (`nros::std_shim::` has `s` behind it, `foo::std::` does not and is
# a genuine `std` root reached through a rooted path — which is why the
# lookbehind rejects `:` too).
STD_PATH_RE = re.compile(r"(?<![A-Za-z0-9_:])(?:::)?std::")

# The proc-macro emits through `quote!` / `quote_spanned!`. Everything outside
# such a block is host-side code the macro RUNS, not code it WRITES.
QUOTE_OPEN_RE = re.compile(r"\bquote(?:_spanned)?!\s*[{(\[]")

# A top-level `fn name(` / `pub fn name(` in a Rust file, at column 0.
TOP_FN_RE = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+)*fn\s+(\w+)")

# Functions whose ENTIRE emitted body is hosted-only by construction: the
# caller decides, from the board, whether to call them at all.
#
#   `hosted_std_scaffold_ts(links_std)` returns an EMPTY token stream when
#   `links_std` is false, and its one caller passes
#   `board_entry_links_std(deploy).unwrap_or(true)`. Issue 1381.
HOSTED_ONLY_EMITTERS = {
    "hosted_std_scaffold_ts": "issue 1381 — empty token stream unless the board's entry links std",
}

# Real, open, and owned by other work. Path (repo-relative) → issue id.
KNOWN_OPEN = {
    "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/rust/entry.rs.jinja": 1409,
}

# What counts as a producer of Rust ENTRY code.
PRODUCER_ROOTS = [
    # The canonical compile-time emitter.
    ("packages/core/nros-macros/src", "quote"),
    # The CLI's mirror of it (issue 0302), templates only — the surrounding
    # Rust is host-side renderer code, same as the proc-macro's.
    ("packages/cli/nros-cli-core/src/codegen/entry/packs", "template"),
]


def _tracked_files(root: Path):
    """Tracked paths under `root` — the git index for an in-repo root, a walk
    otherwise.

    Same split, and the same reason, as `check-no-std-stdio`: `rglob` over a
    built tree descends every `target/` before any filter can prune it, while
    the index answers in milliseconds. The walk exists for `--self-test`, whose
    temp trees are untracked by construction and tiny.
    """
    try:
        rel = root.relative_to(REPO)
    except ValueError:
        return sorted(p for p in root.rglob("*") if p.is_file()) if root.is_dir() else []
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "--", str(rel)],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split("\0")
    return [REPO / r for r in out if r]


def strip_line_comment(line):
    """The code part of a Rust/Jinja line.

    Only `//`, and only when it is not inside a string literal — crude, but the
    corpus is emitted code, where a `//` after an open quote is a URL or a path
    and never introduces a `std::`.
    """
    in_str = False
    esc = False
    for i, ch in enumerate(line):
        if esc:
            esc = False
            continue
        if ch == "\\":
            esc = True
            continue
        if ch == '"':
            in_str = not in_str
            continue
        if not in_str and ch == "/" and line[i : i + 2] == "//":
            return line[:i]
    return line


def quote_spans(text):
    """Line numbers (0-based) inside a `quote!` / `quote_spanned!` block.

    Brace-counted from the macro's opening delimiter. String literals and line
    comments are skipped so a `{` in either does not shift the depth.
    """
    inside = set()
    lines = text.splitlines()
    depth = 0
    for i, line in enumerate(lines):
        code = strip_line_comment(line)
        j = 0
        started_here = False
        while j < len(code):
            if depth == 0:
                m = QUOTE_OPEN_RE.search(code, j)
                if not m:
                    break
                depth = 1
                started_here = True
                j = m.end()
                continue
            ch = code[j]
            if ch == '"':
                j += 1
                while j < len(code):
                    if code[j] == "\\":
                        j += 2
                        continue
                    if code[j] == '"':
                        break
                    j += 1
            elif ch in "{([":
                depth += 1
            elif ch in "})]":
                depth -= 1
                if depth == 0:
                    inside.add(i)
            j += 1
        if depth > 0 or started_here:
            inside.add(i)
    return inside


def enclosing_fn(lines, i):
    """The name of the top-level `fn` containing line `i`, or None."""
    for j in range(i, -1, -1):
        m = TOP_FN_RE.match(lines[j])
        if m:
            return m.group(1)
    return None


def scan_rust_producer(path):
    """[(lineno, line)] — `std::` inside a `quote!` block, outside an allowed fn."""
    text = path.read_text(errors="replace")
    lines = text.splitlines()
    emitted = quote_spans(text)
    hits = []
    for i, line in enumerate(lines):
        if i not in emitted:
            continue
        code = strip_line_comment(line)
        if not STD_PATH_RE.search(code):
            continue
        if enclosing_fn(lines, i) in HOSTED_ONLY_EMITTERS:
            continue
        hits.append((i + 1, line.strip()))
    return hits


def scan_template(path):
    """[(lineno, line)] — every `std::` in a template pack file."""
    hits = []
    for i, line in enumerate(path.read_text(errors="replace").splitlines()):
        code = strip_line_comment(line)
        # A jinja comment line is prose about the template, not template output.
        if code.lstrip().startswith(("{#", "#")):
            continue
        if STD_PATH_RE.search(code):
            hits.append((i + 1, line.strip()))
    return hits


def check(repo=REPO, roots=None):
    """(findings, known_open_hit, stale_known_open, files_scanned)."""
    findings = []
    known_hit = {}
    scanned = 0
    for rel, kind in roots or PRODUCER_ROOTS:
        root = repo / rel
        if not root.exists():
            continue
        for path in _tracked_files(root):
            if kind == "quote" and path.suffix != ".rs":
                continue
            if kind == "template" and path.suffix not in (".jinja", ".j2"):
                continue
            scanned += 1
            hits = scan_rust_producer(path) if kind == "quote" else scan_template(path)
            if not hits:
                continue
            try:
                shown = str(path.relative_to(repo))
            except ValueError:
                shown = str(path)
            if shown in KNOWN_OPEN:
                known_hit[shown] = len(hits)
                continue
            for lineno, line in hits:
                findings.append((shown, lineno, line))
    stale = [p for p in KNOWN_OPEN if p not in known_hit and (repo / p).is_file()]
    return findings, known_hit, stale, scanned


SELF_TESTS = [
    (
        "a ::std:: path inside quote! is a finding",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn emit() -> TokenStream {\n"
                "    quote! {\n"
                '        ::std::process::exit(1);\n'
                "    }\n"
                "}\n"
            )
        },
        1,
    ),
    (
        "the bare `std::` spelling counts too",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn emit() -> TokenStream {\n    quote! { std::env::var(\"X\") }\n}\n"
            )
        },
        1,
    ),
    (
        "host-side std:: OUTSIDE a quote! is the macro's own code, not emitted",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn read() -> String {\n"
                "    std::fs::read_to_string(\"Cargo.toml\").unwrap()\n"
                "}\n"
            )
        },
        0,
    ),
    (
        "the hosted-only emitter is allowed to name std",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn hosted_std_scaffold_ts(links_std: bool) -> TokenStream {\n"
                "    if !links_std {\n"
                "        return quote! {};\n"
                "    }\n"
                "    quote! {\n"
                "        fn main() { ::std::process::exit(1); }\n"
                "    }\n"
                "}\n"
            )
        },
        0,
    ),
    (
        "a second function may NOT borrow the allowed one's licence",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn hosted_std_scaffold_ts(links_std: bool) -> TokenStream {\n"
                "    quote! { ::std::process::exit(1); }\n"
                "}\n"
                "\n"
                "fn something_else() -> TokenStream {\n"
                "    quote! { ::std::process::exit(2); }\n"
                "}\n"
            )
        },
        1,
    ),
    (
        "a commented-out emission is prose",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn emit() -> TokenStream {\n"
                "    quote! {\n"
                "        // was ::std::println! before issue 1381\n"
                "        ::core::result::Result::Ok(())\n"
                "    }\n"
                "}\n"
            )
        },
        0,
    ),
    (
        "a lookalike crate is not std",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn emit() -> TokenStream {\n"
                "    quote! { ::nros_std_shim::exit(1); my_std::exit(2); }\n"
                "}\n"
            )
        },
        0,
    ),
    (
        "an entry template naming std is a finding",
        {
            "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/rust/e.rs.jinja": (
                "fn main() {\n    ::std::process::exit(1);\n}\n"
            )
        },
        1,
    ),
    (
        "a jinja comment about std is prose",
        {
            "packages/cli/nros-cli-core/src/codegen/entry/packs/entry/rust/e.rs.jinja": (
                "{# this used to say ::std::process::exit #}\n"
            )
        },
        0,
    ),
    (
        "a multi-line quote! block keeps its scope to the end",
        {
            "packages/core/nros-macros/src/m.rs": (
                "fn emit() -> TokenStream {\n"
                "    quote! {\n"
                "        fn a() {}\n"
                "        fn b() {\n"
                "            ::std::process::exit(1);\n"
                "        }\n"
                "    }\n"
                "}\n"
            )
        },
        1,
    ),
]


def self_test():
    import tempfile

    failures = 0
    for name, files, expected in SELF_TESTS:
        with tempfile.TemporaryDirectory() as td:
            repo = Path(td)
            for rel, content in files.items():
                p = repo / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(content)
            findings, _, _, _ = check(repo=repo)
            got = len(findings)
            if got != expected:
                failures += 1
                print(f"  FAIL  {name}: expected {expected}, got {got}")
                for f in findings:
                    print(f"          {f[0]}:{f[1]}: {f[2]}")
            else:
                print(f"  ok    {name}")
    if failures:
        print(f"\ncheck-no-std-entry-emission --self-test: {failures} case(s) FAILED")
        return 1
    print(f"\ncheck-no-std-entry-emission --self-test: {len(SELF_TESTS)} case(s) OK")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    findings, known_hit, stale, scanned = check()

    if stale:
        print(
            "check-no-std-entry-emission: a KNOWN_OPEN site has no finding any more:",
            file=sys.stderr,
        )
        for p in stale:
            print(f"  {p}  (issue {KNOWN_OPEN[p]})", file=sys.stderr)
        print(
            "\n  Either it was fixed — then delete its row from KNOWN_OPEN in\n"
            "  scripts/check-no-std-entry-emission.py and resolve the issue — or this\n"
            "  gate has stopped looking at it, which is worse. An exemption that\n"
            "  outlives its defect is how a gate quietly narrows.\n",
            file=sys.stderr,
        )
        return 1

    if not findings:
        for p, n in sorted(known_hit.items()):
            print(
                f"check-no-std-entry-emission: KNOWN-OPEN {p}: {n} site(s), issue {KNOWN_OPEN[p]}"
            )
        print(
            f"check-no-std-entry-emission: OK ({scanned} producer file(s), "
            f"{len(known_hit)} known-open)"
        )
        return 0

    print(
        "check-no-std-entry-emission: emitted Rust entry code names a `std` path:",
        file=sys.stderr,
    )
    for path, lineno, line in findings:
        print(f"  {path}:{lineno}: {line}", file=sys.stderr)
    print(
        "\n"
        "  Issue 1381 — an entry crate for a `board-run` / `zephyr-staticlib` board\n"
        "  carries `#![no_std]`, and `#![no_std]` is orthogonal to the target OS: a\n"
        "  `cargo build` that names no `--target` compiles that crate for the HOST,\n"
        "  where `#[cfg(not(target_os = \"none\"))]` lets the hosted arm through and\n"
        "  there is still no `std` in the crate root. The leaf then cannot be built\n"
        "  by hand at all — which is how 1381 was found.\n"
        "\n"
        "  There is no `core`/`alloc` spelling for a wall clock, the process\n"
        "  environment or an exit status. The fix is not to spell them differently,\n"
        "  it is NOT TO EMIT THEM: ask\n"
        "\n"
        "      nros_orchestration_ir::board_entry_links_std(<board key>)\n"
        "\n"
        "  and emit the hosted scaffold only when it answers `Some(true)` (`None` —\n"
        "  an out-of-tree board — means assume hosted). In the proc-macro that is\n"
        "  `main_macro::hosted_std_scaffold_ts`; put new hosted tokens THERE rather\n"
        "  than opening a second site.\n"
        "\n"
        "  A site that is real but cannot be fixed yet goes in KNOWN_OPEN with a\n"
        "  tracked issue id, never in a comment.\n",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
