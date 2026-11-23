#!/usr/bin/env python3
"""Gate: the book never teaches a setting name the knob registry retired.

Every knob, statement and env row in the registry lists the spellings it
retired (`retired = [{ spelling = "<road>:<name>" }]`, RFC-0140). A build
refuses most of them, so a reader who copies one from the book gets an error
instead of a setting. The book went stale this way after the phase-581
renames: `buffer_size = 256` under `[image.<id>.knobs.xrce]`, `batch` for
`tx_batch`, `buffer_size` in the logging table, and three `ZENOH_TLS_*`
variables listed as current. `check-book-identifiers` checks lower-case code
names and `check-config-names` checks the registry itself; neither reads the
book's setting names, so nothing caught them.

What it checks, on every page under book/src except the release pages in
`migrating/` (which name old spellings on purpose) and parked chapters:

  * an `env:`, `run:`, `kconfig:` or `cmake:` retired spelling, as a whole
    word anywhere on the page;
  * a `toml:` retired spelling written out with its dots (`zenoh.wire.x`);
  * a key in a ```toml block whose table header puts it at a retired
    spelling: `buffer_size = 256` under `[image.<id>.knobs.xrce]` is
    `xrce.buffer_size`.

A paragraph (or table row) that says "retired" may name an old spelling: that
is how a page tells the reader what changed.

NOT checked: a bare key in prose (`batch`, `buffer_size`). Without its table
header it could be any owner's, and the same word is often live under another
(`log.buffer_bytes` sits beside a retired `xrce.buffer_size`), so matching it
would fail pages that are right. Two of the stale names above were of this
kind and were found by reading.

The retired set is read from the same registry `check-config-names` reads
(`scripts/lib/knob_registry.py`), so a new retirement is checked here with no
edit to this file.

Run:  python3 scripts/check-book-retired-names.py [--selftest]
Gate: just check book-retired-names
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import knob_registry as kr  # noqa: E402

SKIP_PREFIX = "book/src/migrating/"
# The same parked-chapter rule `check-book-board-names.py` reads from SUMMARY.md.
PARKED = re.compile(r"\[[^\]]*\((?:[^)]*,\s*)?(?:dormant|retired)\)\]\(\./([^)]+)\)", re.I)
TABLE = re.compile(r"^\s*\[\s*([A-Za-z0-9_.<>\-\"]+)\s*\]\s*(?:#.*)?$")
KEY = re.compile(r"^\s*([a-z][a-z0-9_]*)\s*=")
SAYS_RETIRED = re.compile(r"\bretired\b", re.I)


def _rows():
    """Every registry row: the merged registry's, then tracked package files'."""
    reg = kr.registry()
    for part in ("knob", "statement", "env", "kconfig"):
        yield from reg.get(part, []) or []
    known = {Path(p["file"]).resolve() for p in reg.get("packages", [])}
    listed = subprocess.run(["git", "-C", str(ROOT), "ls-files", "*nros-knobs.toml"],
                            capture_output=True, text=True, check=True).stdout.split()
    for rel in listed:
        path = (ROOT / rel).resolve()
        if path in known:
            continue
        doc = kr.read(path)
        errors: list[str] = []
        knobs, _, envs, _ = kr._parse_file(doc, kr._rel(path), {}, doc.get("owner"), errors)
        if errors:
            raise SystemExit(f"check-book-retired-names: {kr._rel(path)}: " + "; ".join(errors))
        yield from knobs
        yield from envs


def retired_spellings() -> dict[str, str]:
    """{name: road} of every retired spelling."""
    out: dict[str, str] = {}
    for row in _rows():
        for e in row.get("retired_entries", []):
            out.setdefault(e["name"], e["road"])
        # A knob retired as a whole row keeps its key only to refuse it.
        if row.get("retired") and row.get("key") and row.get("id"):
            out.setdefault(row["key"], "toml")
            out.setdefault("NROS_" + row["id"], "env")
            out.setdefault("CONFIG_NROS_" + row["id"], "kconfig")
    return out


def table_prefix(header: str) -> str | None:
    """The knob prefix a toml table header sets, or None for a non-knob table.

    `[image.<id>.knobs.zenoh.wire]` and `[knobs.zenoh.wire]` give `zenoh.wire`.
    """
    parts = header.split(".")
    if "knobs" not in parts:
        return None
    rest = parts[parts.index("knobs") + 1:]
    return ".".join(rest) if rest else None


def paragraphs(lines: list[str]):
    """Yield (first line number, lines) of each blank-line paragraph. A table
    row is its own paragraph: one row naming a retired spelling does not
    excuse its neighbours."""
    block: list[tuple[int, str]] = []
    for i, line in enumerate(lines, 1):
        if not line.strip():
            if block:
                yield block
            block = []
        elif line.lstrip().startswith("|"):
            if block:
                yield block
            yield [(i, line)]
            block = []
        else:
            block.append((i, line))
    if block:
        yield block


def findings(text: str, retired: dict[str, str]) -> list[tuple[int, str, str]]:
    """[(line, spelling, how)] of every retired spelling the page teaches."""
    words = {n for n, road in retired.items() if road != "toml"}
    dotted = {n for n, road in retired.items() if road == "toml"}
    word_rx = re.compile(r"(?<![A-Za-z0-9_])(" + "|".join(map(re.escape, sorted(words, key=len, reverse=True)))
                         + r")(?![A-Za-z0-9_])") if words else None
    dot_rx = re.compile(r"(?<![A-Za-z0-9_.])(" + "|".join(map(re.escape, sorted(dotted, key=len, reverse=True)))
                        + r")(?![A-Za-z0-9_.])") if dotted else None

    lines = text.splitlines()
    hits: dict[int, list[tuple[str, str]]] = {}

    # Keys inside ```toml blocks, resolved through their table header.
    in_toml = False
    prefix = None
    for i, line in enumerate(lines, 1):
        fence = line.strip()
        if fence.startswith("```"):
            in_toml = not in_toml and fence[3:].strip().startswith("toml")
            prefix = None
            continue
        if not in_toml:
            continue
        m = TABLE.match(line)
        if m:
            prefix = table_prefix(m.group(1))
            continue
        m = KEY.match(line)
        if m and prefix and f"{prefix}.{m.group(1)}" in dotted:
            hits.setdefault(i, []).append((f"{prefix}.{m.group(1)}", "toml key under its table"))

    for i, line in enumerate(lines, 1):
        for rx, how in ((word_rx, "name"), (dot_rx, "dotted key")):
            if rx:
                for m in rx.finditer(line):
                    hits.setdefault(i, []).append((m.group(1), how))

    out = []
    for block in paragraphs(lines):
        if any(SAYS_RETIRED.search(line) for _, line in block):
            continue
        for n, _ in block:
            out += [(n, s, how) for s, how in hits.get(n, [])]
    return sorted(set(out))


def selftest(verbose: bool = False) -> int:
    retired = {"xrce.buffer_size": "toml", "zenoh.wire.frag_max_size": "toml",
               "ZENOH_TLS_ROOT_CA_CERTIFICATE": "run", "CONFIG_NROS_OLD": "kconfig"}
    cases = [
        ("a retired key under its table fails",
         "```toml\n[image.posix.knobs.xrce]\nbuffer_size = 256\n```\n", True),
        ("the current key passes", "```toml\n[image.posix.knobs.xrce]\nbuffer_bytes = 256\n```\n", False),
        ("a board table resolves too", "```toml\n[knobs.zenoh.wire]\nfrag_max_size = 1\n```\n", True),
        ("the same word under another table passes",
         "```toml\n[image.posix.knobs.log]\nbuffer_size = 256\n```\n", False),
        ("a retired variable fails", "Set `ZENOH_TLS_ROOT_CA_CERTIFICATE` to the file.\n", True),
        ("a retired Kconfig symbol fails", "```\nCONFIG_NROS_OLD=y\n```\n", True),
        ("a dotted key in prose fails", "Raise `zenoh.wire.frag_max_size`.\n", True),
        ("a paragraph saying retired passes",
         "`ZENOH_TLS_ROOT_CA_CERTIFICATE` is retired;\nuse the session file.\n", False),
        ("a table row saying retired passes",
         "| `NEW` | (`ZENOH_TLS_ROOT_CA_CERTIFICATE` is retired.) |\n", False),
        ("one retired row does not excuse the next",
         "| a | `CONFIG_NROS_OLD` is retired |\n| b | `ZENOH_TLS_ROOT_CA_CERTIFICATE` |\n", True),
        ("a longer name is not its prefix", "`ZENOH_TLS_ROOT_CA_CERTIFICATE_PATH`\n", False),
    ]
    failed = 0
    for name, page, want in cases:
        got = bool(findings(page, retired))
        if got != want:
            failed += 1
            print(f"check-book-retired-names: SELFTEST FAILED: {name}", file=sys.stderr)
        elif verbose:
            print(f"  ok: {name}")
    if verbose and not failed:
        print("check-book-retired-names: selftest OK")
    return 1 if failed else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1

    retired = retired_spellings()
    if not retired:
        print("check-book-retired-names: the registry yielded NO retired spelling", file=sys.stderr)
        return 1

    summary = (ROOT / "book/src/SUMMARY.md").read_text()
    parked = {f"book/src/{p}" for p in PARKED.findall(summary)}
    pages = subprocess.run(["git", "-C", str(ROOT), "ls-files", "book/src/**/*.md", "book/src/*.md"],
                           capture_output=True, text=True, check=True).stdout.split()
    bad = 0
    for rel in sorted(set(pages)):
        if rel.startswith(SKIP_PREFIX) or rel in parked:
            continue
        for line, spelling, how in findings((ROOT / rel).read_text(), retired):
            bad += 1
            print(f"{rel}:{line}: `{spelling}` is retired ({how}); write its current name, "
                  "or say on the same paragraph that it is retired")
    if bad:
        print(f"check-book-retired-names: {bad} retired spelling(s) in the book "
              "(`nros knob show <key>` prints a knob's current name)", file=sys.stderr)
        return 1
    print(f"check-book-retired-names: {len(retired)} retired spellings, none taught by the book")
    return 0


if __name__ == "__main__":
    sys.exit(main())
