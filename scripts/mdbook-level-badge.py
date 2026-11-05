#!/usr/bin/env python3
"""mdBook preprocessor: a page's type and level as a badge under its title
(RFC-0124 D2 and Topic levels, phase-537 W3).

Every hand-written user page declares its type and level on the line after
its title (`scripts/lib/book_charter.py` reads it):

    # Configure zenoh

    <!-- type: how-to · level: 2 -->

The comment is invisible in the rendered page. This turns it into a small
badge, "How-to · Level 2: Building real applications", so a reader sees what
kind of page it is and what it assumes before reading on. The level names are
the four of RFC-0124's Topic levels (`book_charter.LEVEL_NAMES`). A page with
no declaration, which is every generated page, gets no badge. A malformed
declaration is left as it is: `check-book-audience` reports it.

The colours are mdBook's theme variables, so every theme applies. The style
is one inline block per page that has a badge.

book/book.toml names it; mdBook runs it from the book root. Protocol as
`mdbook-lang-tabs.py`: `supports <renderer>` exits 0; otherwise `[context,
book]` JSON on stdin, the book on stdout.

  python3 scripts/mdbook-level-badge.py --self-test
"""

from __future__ import annotations

import html
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import book_charter  # noqa: E402

TYPE_LABELS = {"tutorial": "Tutorial", "how-to": "How-to", "explanation": "Explanation",
               "reference": "Reference"}

STYLE = """<style>
.nros-level-badge { display: inline-block; margin: 0 0 1em; padding: 0.15em 0.7em;
  border: 1px solid var(--table-border-color); border-radius: 999px;
  background: var(--table-alternate-bg); color: var(--fg); font-size: 0.85em; }
.nros-level-badge .nros-level-type { color: var(--links); font-weight: 600; }
</style>"""


def badge(typ: str, level: int) -> str | None:
    """The badge's HTML, or None when the type or level is not a known one."""
    label, name = TYPE_LABELS.get(typ), book_charter.LEVEL_NAMES.get(level)
    if label is None or name is None:
        return None
    text = f"Level {level}: {name}"
    return (f'<div class="nros-level-badge" data-type="{typ}" data-level="{level}">'
            f'<span class="nros-level-type">{html.escape(label)}</span> · '
            f'<span class="nros-level-name">{html.escape(text)}</span></div>')


def render(content: str) -> str:
    """The chapter with its declaration rendered as a badge; unchanged when it
    declares none."""
    lines = content.split("\n")
    fence = False
    for i, line in enumerate(lines[:6]):
        if line.lstrip().startswith(("```", "~~~")):
            fence = not fence
            continue
        m = None if fence else book_charter.DECL.match(line.strip())
        if not m:
            continue
        html_badge = badge(m.group(1), int(m.group(2)))
        if html_badge is None:
            return content
        lines[i] = "\n" + html_badge + "\n"
        trailer = "\n" if content.endswith("\n") else ""
        return "\n".join(lines).rstrip("\n") + "\n\n" + STYLE + trailer
    return content


def rewrite(node, changed: list[str]) -> None:
    if isinstance(node, list):
        for item in node:
            rewrite(item, changed)
        return
    if not isinstance(node, dict):
        return
    chapter = node.get("Chapter")
    if isinstance(chapter, dict):
        content = chapter.get("content") or ""
        new = render(content)
        if new != content:
            changed.append(chapter.get("source_path") or chapter.get("path") or chapter.get("name", "?"))
            chapter["content"] = new
        rewrite(chapter.get("sub_items", []), changed)
        return
    for key in ("sections", "items"):
        if key in node:
            rewrite(node[key], changed)


def run(stdin: str) -> tuple[str, list[str]]:
    _context, book = json.loads(stdin)
    changed: list[str] = []
    rewrite(book, changed)
    return json.dumps(book), changed


def self_test() -> None:
    assert sorted(TYPE_LABELS) == sorted(book_charter.TYPES)
    assert sorted(book_charter.LEVEL_NAMES) == list(book_charter.LEVELS)
    page = "# Configure zenoh\n\n<!-- type: how-to · level: 2 -->\n\nSteps.\n"
    out = render(page)
    assert "<!-- type:" not in out, out
    assert ">How-to</span> · <span class=\"nros-level-name\">Level 2: Building real applications<" in out, out
    # the badge sits where the declaration was, as its own HTML block, under the title
    assert out.startswith("# Configure zenoh\n\n\n<div class=\"nros-level-badge\""), out
    assert out.index("nros-level-badge\"") < out.index("Steps.") < out.index("<style>")
    assert out.count("<style>") == 1 and out.endswith("</style>\n")
    topic = "# T\n<!-- type: tutorial · level: 1 · topic: rmw -->\nx\n"
    assert "Tutorial</span> · <span class=\"nros-level-name\">Level 1: First contact<" in render(topic)
    assert "Level 4: Extending nano-ros" in render("# T\n<!-- type: explanation · level: 4 -->\n")
    assert "Level 3: Production and tuning" in render("# T\n<!-- type: reference · level: 3 -->\n")
    # no declaration (a generated page), a malformed one, or an unknown level: unchanged
    for same in ("# Gen\n\n<!-- GENERATED by x -->\n| a |\n", "# T\n<!-- type: how-to -->\n",
                 "# T\n<!-- type: how-to · level: 7 -->\n", "# T\n<!-- type: recipe · level: 1 -->\n",
                 "# T\n" + "\n" * 6 + "<!-- type: how-to · level: 1 -->\n"):
        assert render(same) == same, same
    # a declaration quoted in a page's body is not the page's own
    body = "# T\n\nA page declares:\n\n```markdown\n<!-- type: how-to · level: 2 -->\n```\n"
    assert render(body) == body
    for shape in ("sections", "items"):
        book = json.dumps([{"root": "."}, {shape: [
            {"Chapter": {"name": "p", "content": page, "path": "p.md", "sub_items": [
                {"Chapter": {"name": "q", "content": "# Q\n", "path": "q.md", "sub_items": []}}]}},
            "Separator"]}])
        text, changed = run(book)
        assert changed == ["p.md"], changed
        doc = json.loads(text)[shape][0]["Chapter"]
        assert "nros-level-badge" in doc["content"] and doc["sub_items"][0]["Chapter"]["content"] == "# Q\n"
    print("mdbook-level-badge: self-test OK")


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        return 0
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        self_test()
        return 0
    out, changed = run(sys.stdin.read())
    print(f"mdbook-level-badge: {len(changed)} page(s) badged", file=sys.stderr)
    sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
