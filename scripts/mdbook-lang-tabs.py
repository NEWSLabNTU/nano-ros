#!/usr/bin/env python3
"""mdBook preprocessor: Rust | C | C++ language tabs (RFC-0124 D9, phase-537 W2).

A page writes one step in several languages as a tab group (the syntax and its
parser are `scripts/lib/book_tabs.py`):

    <!-- tabs -->
    #### Rust
    …markdown…
    #### C
    …markdown…
    <!-- /tabs -->

This turns each group into tabs. The output is HTML wrappers around the tabs'
own markdown, with blank lines between, so a fence inside a tab stays a fence
and mdBook still highlights it. The tabs are radio buttons styled by CSS, so
they work with no script; a small inline script, once per page, makes one
choice switch every group on the page and remembers it across pages in
localStorage (inside try/catch: a page with storage blocked still works). The
colours are mdBook's theme variables, so every theme applies. Printed, every
tab shows under its label.

book/book.toml and book-internals/book.toml name it; mdBook runs it from the
book root. Protocol as `mdbook-board-status.py`: `supports <renderer>` exits 0;
otherwise `[context, book]` JSON on stdin, the book on stdout. A malformed group
fails the build naming the chapter and line.

  python3 scripts/mdbook-lang-tabs.py --self-test
"""

from __future__ import annotations

import hashlib
import html
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import book_tabs  # noqa: E402

STORAGE_KEY = "nros-book-lang"

# Hidden panels: a rule per language, written so that a browser without
# `:has()` drops it and shows every panel instead of none.
_HIDE = "\n".join(
    f'.nros-tabs:has(> .nros-tabs-bar input[value="{lang}"]:not(:checked)) > .nros-tab-panel[data-lang="{lang}"] {{ display: none; }}'
    for lang in book_tabs.LABELS.values())

STYLE = """<style>
/* One group is one card: a frame around the tab strip and the panel, so the
   tabs read as the panel's own labels. The selected tab takes the page colour
   and an accent line, and opens into the panel; a code block inside sits flush
   against the frame instead of drawing a second border. Colours derive from
   mdBook's --fg/--bg/--links, so every theme applies. */
.nros-tabs { --nros-tabs-edge: color-mix(in srgb, var(--fg) 25%%, var(--bg));
  margin: 1em 0; border: 1px solid var(--nros-tabs-edge); border-radius: 6px; overflow: hidden; }
.nros-tabs-bar { display: flex; flex-wrap: wrap;
  background: color-mix(in srgb, var(--fg) 7%%, var(--bg));
  border-bottom: 1px solid var(--nros-tabs-edge); }
.nros-tab { position: relative; padding: 0.4em 1.1em; margin-bottom: -1px; cursor: pointer;
  border-right: 1px solid transparent; border-top: 2px solid transparent; color: var(--fg); }
.nros-tab:hover { background: color-mix(in srgb, var(--fg) 12%%, var(--bg)); }
.nros-tab:has(input:checked) { background: var(--bg); color: var(--links); font-weight: 600;
  border-top-color: var(--links); border-right-color: var(--nros-tabs-edge);
  border-bottom: 1px solid var(--bg); }
.nros-tab:first-child:has(input:checked) { border-left: none; }
.nros-tab:not(:first-child):has(input:checked) { border-left: 1px solid var(--nros-tabs-edge); }
.nros-tab:has(input:focus-visible) { outline: 2px solid var(--links); outline-offset: -2px; }
.nros-tab input { position: absolute; opacity: 0; width: 1px; height: 1px; margin: 0; }
.nros-tab-panel > * { margin-left: 1rem; margin-right: 1rem; }
.nros-tab-panel > :first-child { margin-top: 0.8em; }
.nros-tab-panel > :last-child { margin-bottom: 0.8em; }
.nros-tab-panel > pre { margin-left: 0; margin-right: 0; }
.nros-tab-panel > pre:first-child { margin-top: 0; }
.nros-tab-panel > pre:last-child { margin-bottom: 0; }
.nros-tab-panel > pre > code { border: none; border-radius: 0; }
.nros-tab-panel > pre:not(:first-child) > code { border-top: 1px solid var(--nros-tabs-edge); }
.nros-tab-panel > pre:not(:last-child) > code { border-bottom: 1px solid var(--nros-tabs-edge); }
%s
@media print {
  .nros-tabs { border: none; }
  .nros-tabs-bar { display: none; }
  .nros-tab-panel { display: block !important; }
  .nros-tab-panel::before { content: attr(aria-label); font-weight: 600; }
}
</style>""" % _HIDE

SCRIPT = """<script>
(function () {
  if (window.nrosLangTabs) return;
  window.nrosLangTabs = true;
  var KEY = "%s";
  function radios() { return document.querySelectorAll(".nros-tabs-bar input[type=radio]"); }
  function pick(lang) {
    radios().forEach(function (r) { if (r.value === lang) r.checked = true; });
  }
  function restore() {
    var saved = null;
    try { saved = window.localStorage.getItem(KEY); } catch (e) {}
    if (saved) pick(saved);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", restore);
  else restore();
  document.addEventListener("change", function (ev) {
    var r = ev.target;
    if (!r.matches || !r.matches(".nros-tabs-bar input[type=radio]")) return;
    var before = r.getBoundingClientRect().top;
    pick(r.value);
    window.scrollBy(0, r.getBoundingClientRect().top - before);
    try { window.localStorage.setItem(KEY, r.value); } catch (e) {}
  });
})();
</script>""" % STORAGE_KEY


def render_group(g: book_tabs.Group, name: str) -> str:
    bar = [f'<div class="nros-tabs-bar" role="radiogroup" aria-label="Language">']
    for i, t in enumerate(g.tabs):
        checked = " checked" if i == 0 else ""
        bar.append(f'<label class="nros-tab"><input type="radio" name="{name}" '
                   f'value="{t.lang}"{checked}><span>{html.escape(t.label)}</span></label>')
    bar.append("</div>")
    out = ['<div class="nros-tabs">', *bar, ""]
    for t in g.tabs:
        out.append(f'<div class="nros-tab-panel" data-lang="{t.lang}" role="group" '
                   f'aria-label="{html.escape(t.label)}">')
        out.append("")
        out.append("\n".join(t.body).strip("\n"))
        out.append("")
        out.append("</div>")
        out.append("")
    out.append("</div>")
    return "\n".join(out)


def render(content: str, path: str) -> str:
    """The chapter with every group rendered; unchanged when it has none."""
    segs = book_tabs.segments(content)
    if len(segs) == 1:
        return content
    page_id = hashlib.sha1(path.encode()).hexdigest()[:8]
    out, n = [], 0
    for seg in segs:
        if isinstance(seg, book_tabs.Group):
            n += 1
            out.append("\n" + render_group(seg, f"nros-tabs-{page_id}-{n}") + "\n")
        else:
            out.append(seg)
    trailer = "\n" if content.endswith("\n") else ""
    return "\n".join(out) + "\n\n" + STYLE + "\n\n" + SCRIPT + "\n" + trailer


def rewrite(node, errors: list[str], changed: list[str]) -> None:
    if isinstance(node, list):
        for item in node:
            rewrite(item, errors, changed)
        return
    if not isinstance(node, dict):
        return
    chapter = node.get("Chapter")
    if isinstance(chapter, dict):
        path = chapter.get("source_path") or chapter.get("path") or chapter.get("name", "?")
        content = chapter.get("content") or ""
        try:
            new = render(content, path)
        except book_tabs.TabError as e:
            errors.append(f"{path}: {e}")
        else:
            if new != content:
                changed.append(path)
                chapter["content"] = new
        rewrite(chapter.get("sub_items", []), errors, changed)
        return
    for key in ("sections", "items"):
        if key in node:
            rewrite(node[key], errors, changed)


def run(stdin: str) -> tuple[str, list[str], list[str]]:
    _context, book = json.loads(stdin)
    errors: list[str] = []
    changed: list[str] = []
    rewrite(book, errors, changed)
    return json.dumps(book), errors, changed


def self_test() -> None:
    book_tabs.self_test()
    page = ("# P\n\n<!-- tabs -->\n\n#### Rust\n\nSay it in Rust:\n\n```rust\nlet x = 1;\n```\n\n"
            "#### C\n\n```c\nint x = 1;\n```\n\n<!-- /tabs -->\n\nAfter.\n")
    out = render(page, "guide/p.md")
    assert "<!-- tabs -->" not in out and "#### Rust" not in out, out
    # the fences survive as fences, on lines of their own, with blank lines around
    assert "\n\n```rust\nlet x = 1;\n```\n\n</div>" in out, out
    assert "\n\nSay it in Rust:\n\n```rust" in out, out
    assert out.count('type="radio"') == 2 and out.count(" checked>") == 1
    assert 'value="rust" checked' in out and 'value="c">' in out
    assert out.count("<script>") == 1 and out.count("<style>") == 1
    assert out.index("After.") < out.index("<script>")
    # two pages never share a radio-group name (print.html holds every page)
    assert render(page, "a.md").split('name="')[1][:20] != render(page, "b.md").split('name="')[1][:20]
    plain = "# P\n\nno tabs\n"
    assert render(plain, "x.md") == plain
    for shape in ("sections", "items"):
        book = json.dumps([{"root": "."}, {shape: [
            {"Chapter": {"name": "p", "content": page, "path": "p.md", "sub_items": [
                {"Chapter": {"name": "q", "content": plain, "path": "q.md", "sub_items": []}}]}},
            "Separator"]}])
        text, errors, changed = run(book)
        assert not errors and changed == ["p.md"], (errors, changed)
        doc = json.loads(text)[shape][0]["Chapter"]
        assert "nros-tabs" in doc["content"] and doc["sub_items"][0]["Chapter"]["content"] == plain
    bad = json.dumps([{}, {"items": [{"Chapter": {"name": "b", "content": "<!-- tabs -->\n#### Go\n",
                                                  "path": "b.md"}}]}])
    _, errors, _ = run(bad)
    assert len(errors) == 1 and errors[0].startswith("b.md: line 2:"), errors
    print("mdbook-lang-tabs: self-test OK")


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        return 0
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        self_test()
        return 0
    out, errors, changed = run(sys.stdin.read())
    if errors:
        for e in errors:
            print(f"mdbook-lang-tabs: {e}", file=sys.stderr)
        return 1
    for path in changed:
        print(f"mdbook-lang-tabs: {path}: tabs rendered", file=sys.stderr)
    sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
