#!/usr/bin/env python3
"""mdBook preprocessor: a file the reader writes is shown as the file, under
its path.

A tutorial step that creates a file used to be a shell heredoc,
`cat > manifest/west.yml <<'EOF' … EOF`, so the file's content read as part of
a command. Instead the page shows the file as its own block, highlighted in
its own language, and names it in the fence's info string:

    ```yaml probe=410 file=manifest/west.yml
    manifest:
      …
    ```

`append=PATH` marks text added to the end of an existing file, and
`edit=PATH` a change to part of one: a ```diff block whose `-` lines become
its `+` lines, with a few unchanged lines around them (a space first) to
show where. The probe
(`scripts/probe/extract-book-steps.py`) writes the block's body to the path;
this renders the path as a caption above the block ("manifest/west.yml", or
"Add to the end of prj.conf", "In system.toml, change the marked lines"). mdBook ignores the extra info-string words, so
the block still highlights.

The caption joins the block into one card: `book/theme/nros.css` styles
`.nros-file-caption` and the code block after it, alongside the Terminal and
Output labels it puts on command and output blocks.

book/book.toml names it; mdBook runs it from the book root. Protocol as
`mdbook-lang-tabs.py`: `supports <renderer>` exits 0; otherwise `[context,
book]` JSON on stdin, the book on stdout.

  python3 scripts/mdbook-file-blocks.py --self-test
"""

from __future__ import annotations

import html
import json
import re
import sys

FENCE = re.compile(r"^(\s*)(`{3,}|~{3,})(.*)$")
FILE_TOKEN = re.compile(r"(?:^|\s)(file|append|edit)=(\S+)(?:\s|$)")


def caption(indent: str, mode: str, path: str) -> str:
    code = f"<code>{html.escape(path)}</code>"
    label = {"file": code, "append": f"Add to the end of {code}",
             "edit": f"In {code}, change the marked lines"}[mode]
    return f'{indent}<div class="nros-file-caption" data-mode="{mode}">{label}</div>'


def render(content: str) -> str:
    """The chapter with a caption above every `file=` / `append=` block."""
    out: list[str] = []
    fence: str | None = None
    for line in content.split("\n"):
        m = FENCE.match(line)
        if fence is None and m:
            fence = m.group(2)
            tok = FILE_TOKEN.search(m.group(3))
            if tok:
                # A blank line before and after the HTML block keeps the fence
                # a fence (CommonMark ends an HTML block at a blank line).
                if out and out[-1].strip():
                    out.append("")
                out.append(caption(m.group(1), tok.group(1), tok.group(2)))
                out.append("")
        elif fence is not None and m and m.group(2).startswith(fence[0]) \
                and len(m.group(2)) >= len(fence) and not m.group(3).strip():
            fence = None
        out.append(line)
    return "\n".join(out)


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


def self_test() -> None:
    page = ("Write it:\n```yaml probe=4 file=manifest/west.yml\nmanifest: {}\n```\n\n"
            "Then:\n\n```bash\ncat > x <<'EOF'\n```yaml file=not/a/fence.yml\nEOF\n```\n"
            "  ```text append=prj.conf\n  CONFIG_X=y\n  ```\n")
    out = render(page)
    assert ('Write it:\n\n<div class="nros-file-caption" data-mode="file"><code>manifest/west.yml'
            '</code></div>\n\n```yaml probe=4') in out, out
    # a `file=` line inside another block is that block's text, not a fence
    assert out.count("nros-file-caption") == 2, out
    assert ('  <div class="nros-file-caption" data-mode="append">Add to the end of '
            '<code>prj.conf</code></div>\n\n  ```text append') in out, out
    assert "In <code>b/system.toml</code>, change the marked lines" in render(
        "```diff edit=b/system.toml\n-a\n+b\n```\n")
    assert render("plain\n```bash\nls\n```\n") == "plain\n```bash\nls\n```\n"
    print("mdbook-file-blocks: self-test OK")


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        sys.exit(0)
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        self_test()
        return
    _context, book = json.loads(sys.stdin.read())
    rewrite(book, [])
    sys.stdout.write(json.dumps(book))


if __name__ == "__main__":
    main()
