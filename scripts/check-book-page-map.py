#!/usr/bin/env python3
"""Every book page has exactly one row in `book/page-map.toml` (phase-532 W0).

The map (RFC-0123 D2) is what moves pages between directories and books, and
what the user-surface gates read their subject from. A page it does not know
is a page no gate reads and no move carries, so:

* every `.md` under `book/src/` and `book-internals/src/` (except SUMMARY.md)
  is some row's current location;
* every page a SUMMARY.md links is some row's current location (this covers
  the generated, gitignored pages that are absent before `just book`);
* the map itself is well-formed (`book_pages.check`);
* both SUMMARY.md files are what the map renders (`book-move.py
  --check-summary`). SUMMARY's ORDER and MEMBERSHIP are generated: a page is
  added or moved by editing the map and running `book-move.py --summary`.
  Titles stay authored in SUMMARY (a page without one takes its `# ` heading),
  and the board generator's marked region is carried through, so neither
  needs the map.

phase-532 W8 folds this into `check-book-audience`, which adds the
SUMMARY-part and contributor-marker rules.

Exit 0 when clean; 1 with one line per problem otherwise.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "lib"))
import book_pages  # noqa: E402

LINK = re.compile(r"\]\(\s*(?:\./)?([^)#\s]+\.md)\s*\)")


def summary_pages(src: pathlib.Path) -> set[pathlib.Path]:
    summary = src / "SUMMARY.md"
    if not summary.exists():
        return set()
    return {(src / m).resolve() for m in LINK.findall(summary.read_text())}


def self_test() -> None:
    """Negative controls, run on every invocation: a map with each defect
    must be refused by `book_pages.check`."""
    P = book_pages.Page
    ok = [P(book="user", path="concepts/a.md", action="move", batch="b", old="x/a.md"),
          P(book="internals", path="rmw/b.md", action="new phase-532", batch="b")]
    assert book_pages.check(ok) == [], book_pages.check(ok)
    bad = {
        "not under a user part": [P(book="user", path="nowhere/a.md", action="move", batch="b", old="x/a.md")],
        "two rows": [ok[0], P(book="user", path="concepts/c.md", action="move", batch="b", old="x/a.md")],
        "target of both": [ok[0], P(book="user", path="concepts/a.md", action="move", batch="b", old="x/z.md")],
        "which no row owns": [P(book="user", path="concepts/q.md", action="merge-into concepts/q.md", batch="b", old="x/q.md")],
        "needs an `old` path": [P(book="user", path="concepts/n.md", action="move", batch="b")],
        "use `move`": [P(book="user", path="concepts/k.md", action="keep", batch="b", old="x/k.md")],
    }
    for want, rows in bad.items():
        got = book_pages.check(rows)
        assert any(want in e for e in got), f"self-test: expected {want!r}, got {got}"


def main() -> int:
    self_test()
    pages = book_pages.load()
    errs = book_pages.check(pages)

    known: set[pathlib.Path] = set()
    for p in pages:
        known.add(p.target.resolve())
        for f in p.formers:
            known.add((book_pages.BOOKS["user"] / f).resolve())

    files = book_pages.book_files()
    for src in book_pages.BOOKS.values():
        if not src.exists():
            continue
        on_disk = {f.resolve() for f in files
                   if f.name != "SUMMARY.md" and src.resolve() in f.resolve().parents}
        for f in sorted(on_disk - known):
            errs.append(f"{f.relative_to(book_pages.ROOT)}: no page-map row (add one to book/page-map.toml)")
        for f in sorted(summary_pages(src) - known):
            errs.append(f"{src.relative_to(book_pages.ROOT)}/SUMMARY.md links {f.relative_to(book_pages.ROOT)}: no page-map row")

    if not errs:
        r = subprocess.run([sys.executable, str(pathlib.Path(__file__).parent / "book-move.py"), "--check-summary"],
                           capture_output=True, text=True)
        errs += [line for line in r.stdout.splitlines() if line.strip()]
        if r.returncode and not r.stdout.strip():
            errs.append(f"book-move.py --check-summary failed: {r.stderr.strip()}")

    if errs:
        print(f"check-book-page-map: {len(errs)} problem(s):")
        for e in errs:
            print(f"  {e}")
        return 1
    n_old = sum(1 for p in pages if p.old)
    print(f"check-book-page-map: OK ({len(pages)} rows, {n_old} existing pages mapped)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
