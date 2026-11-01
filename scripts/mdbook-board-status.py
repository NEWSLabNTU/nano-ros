#!/usr/bin/env python3
"""mdBook preprocessor: the rendered board pages read today's support level.

RFC-0114 D10 says a manual row whose `last_run` is older than
`manual_stale_days` reads *Builds* in every generated view. The committed
markdown cannot do that: a tracked file that changes on a day nobody edited
anything would turn `check-board-registry` red for every pull request from day
121. So the committed pages carry the date the degradation starts, and this
preprocessor applies the clock when the book is BUILT: it re-renders every
board page `scripts/gen-board-support.py` produces under `book/src/` with
today's date, through the same renderer, and replaces that chapter's content in
the book mdBook hands it. A page whose level has not changed renders identically.

book/book.toml names it; mdBook runs it from the book root. Protocol: with
`supports <renderer>` it exits 0 (every renderer); otherwise it reads
`[context, book]` as JSON on stdin and writes the book back on stdout. mdBook
0.4 calls the chapter list `sections` and 0.5 calls it `items`; both are walked.

  NROS_BOARD_STATUS_TODAY=YYYY-MM-DD   evaluate on that date instead of today
  python3 scripts/mdbook-board-status.py --self-test
"""

from __future__ import annotations

import datetime
import importlib.util
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOOK_SRC = "book/src/"


def load_generator():
    spec = importlib.util.spec_from_file_location(
        "gen_board_support", ROOT / "scripts/gen-board-support.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def today() -> datetime.date:
    forced = os.environ.get("NROS_BOARD_STATUS_TODAY")
    return datetime.date.fromisoformat(forced) if forced else datetime.date.today()


def live_pages(gen, root: Path, on: datetime.date) -> dict[str, str]:
    """{chapter path relative to book/src: content on `on`} for every board page."""
    reg = gen.Registry(root)
    reg.today = on
    return {p[len(BOOK_SRC):]: text for p, text in gen.outputs(reg).items()
            if p.startswith(BOOK_SRC) and p.endswith(".md")}


def rewrite(node, pages: dict[str, str], changed: list[str]) -> None:
    """Replace the content of every chapter whose path is a board page."""
    if isinstance(node, list):
        for item in node:
            rewrite(item, pages, changed)
        return
    if not isinstance(node, dict):
        return
    chapter = node.get("Chapter")
    if isinstance(chapter, dict):
        path = chapter.get("path") or chapter.get("source_path")
        if path in pages:
            if chapter.get("content") != pages[path]:
                changed.append(path)
            chapter["content"] = pages[path]
        rewrite(chapter.get("sub_items", []), pages, changed)
        return
    for key in ("sections", "items"):
        if key in node:
            rewrite(node[key], pages, changed)


def run(stdin: str, gen, root: Path, on: datetime.date) -> tuple[str, list[str]]:
    _context, book = json.loads(stdin)
    changed: list[str] = []
    rewrite(book, live_pages(gen, root, on), changed)
    return json.dumps(book), changed


def self_test() -> None:
    """Negative controls on the real registry: the FVP row (last booted
    2026-09-06, 120 stale days) must render as Boots before its stale date and
    as Builds after it, and a book with no board page must pass through."""
    gen = load_generator()
    page = "reference/boards/fvp.md"
    committed = (ROOT / BOOK_SRC / page).read_text()

    def book(shape: str) -> str:
        chapter = {"Chapter": {"name": "x", "content": committed, "path": page,
                               "sub_items": []}}
        return json.dumps([{"root": "."}, {shape: [chapter]}])

    for shape in ("sections", "items"):
        out, changed = run(book(shape), gen, ROOT, datetime.date(2026, 11, 1))
        assert not changed, f"{shape}: a fresh row must render as committed"
        out, changed = run(book(shape), gen, ROOT, datetime.date(2027, 2, 1))
        assert changed == [page], f"{shape}: a stale FVP row was not re-rendered"
        content = json.loads(out)[shape][0]["Chapter"]["content"]
        assert "Builds (last booted 2026-09-06)" in content, content
        assert "Boots, manual" not in content, "the stale word survived"
    other = json.dumps([{}, {"items": [{"Chapter": {"name": "y", "content": "z",
                                                    "path": "intro.md"}}]}])
    out, changed = run(other, gen, ROOT, datetime.date(2027, 2, 1))
    assert not changed and json.loads(out)["items"][0]["Chapter"]["content"] == "z"
    print("mdbook-board-status: self-test OK")


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        return 0
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        self_test()
        return 0
    on = today()
    out, changed = run(sys.stdin.read(), load_generator(), ROOT, on)
    for path in changed:
        print(f"mdbook-board-status: {path} re-rendered for {on}", file=sys.stderr)
    sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
