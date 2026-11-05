"""The Book charter's checks (RFC-0124 Principles and Topic levels, phase-537 W0).

Every hand-written user page carries one declaration line after its title:

    <!-- type: how-to · level: 2 -->

`type` is one of the four Diátaxis types (RFC-0124 D2) and `level` one of the
four topic levels. An optional `topic: <slug>` names the topic, so two pages of
one type for one topic can be found (Principle 7).

`findings(...)` returns one line per departure from a gated principle or the
level rule. phase-537 W0 only REPORTS them (`check-book-audience` prints the
count and never fails on them); W13 turns each into a refusal. The rules that
already refuse elsewhere (internal ids, `nros dev`, typed board facts) are not
repeated here.

Pure functions over (path, text) pairs plus the registry rows the caller
loaded, so the self-test needs no tree.
"""

from __future__ import annotations

import re
from collections import defaultdict

TYPES = ("tutorial", "how-to", "explanation", "reference")
LEVELS = (1, 2, 3, 4)

DECL = re.compile(r"^<!--\s*type:\s*([\w-]+)\s*·\s*level:\s*(\d)(?:\s*·\s*topic:\s*([\w./-]+))?\s*-->\s*$")
ANY_DECL = re.compile(r"^<!--\s*type:")

# RFC-0124 D9: words per type (None = no limit).
BUDGET = {"how-to": (300, 1500), "tutorial": (1500, 3000), "explanation": (800, 2500), "reference": (0, None)}
FLOOR = 300  # D9: a page under 300 words is merged into its parent

# RFC-0124 D1: the type each new section holds. Old part directories have no
# expectation until phase-537 W1 moves their pages.
SECTION_TYPES = {
    "tutorial": {"tutorial"},
    "starters": {"how-to"},
    "guide": {"how-to"},
    "background": {"explanation"},
    "reference": {"reference"},
    "extending": {"how-to", "reference", "explanation"},
    "start": {"how-to", "explanation"},
    "migrating": {"how-to", "reference"},
}

# The "needs first" line a page states its prerequisites on (the W0 templates).
NEEDS = re.compile(r"^\s*(?:>\s*)*\*\*Needs first:\*\*(.*)$", re.I)
LINK = re.compile(r"\]\(([^)#\s]+\.md)(?:#[^)]*)?\)")

FENCE = re.compile(r"^\s*(?:>\s*)*(```|~~~)\s*([\w+-]*)(.*)$")
SHELL_LANGS = {"bash", "sh", "shell", "zsh", "fish"}
COMMENT = re.compile(r"<!--.*?-->", re.S)


def declaration(text: str) -> tuple[str, int, str | None] | None:
    """The page's (type, level, topic), or None when it declares none."""
    for line in text.splitlines()[:6]:
        m = DECL.match(line.strip())
        if m:
            return m.group(1), int(m.group(2)), m.group(3)
    return None


def prose_words(text: str) -> int:
    """Words outside fences and HTML comments: what a reader reads as prose."""
    out, fence = [], None
    for line in text.splitlines():
        m = FENCE.match(line)
        if m:
            fence = None if fence == m.group(1) else (fence or m.group(1))
            continue
        if fence is None:
            out.append(line)
    return len(COMMENT.sub(" ", "\n".join(out)).split())


def unprobed_steps(text: str) -> list[int]:
    """Lines of shell fences with no `probe=` marker (Principle 14)."""
    out, fence = [], None
    for n, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if not m:
            continue
        if fence is not None:
            if m.group(1) == fence and not m.group(2):
                fence = None
            continue
        fence = m.group(1)
        if m.group(2).lower() in SHELL_LANGS and "probe=" not in m.group(3):
            out.append(n)
    return out


def resolve(rel: str, link: str) -> str:
    """`link` from page `rel` (both under book/src/) as a book/src/ path."""
    parts = rel.split("/")[:-1]
    for seg in link.split("/"):
        if seg == "..":
            parts = parts[:-1]
        elif seg and seg != ".":
            parts.append(seg)
    return "/".join(parts)


def page_findings(rel: str, text: str) -> list[str]:
    """The checks that read one page. `rel` is relative to book/src/."""
    d = declaration(text)
    if d is None:
        bad = any(ANY_DECL.match(l.strip()) for l in text.splitlines()[:6])
        return [f"{rel}: {'a malformed' if bad else 'no'} type and level declaration (RFC-0124 D2)"]
    typ, level, _ = d
    out = []
    if typ not in TYPES:
        out.append(f"{rel}: type {typ!r} is not one of {', '.join(TYPES)} (D2)")
        return out
    if level not in LEVELS:
        out.append(f"{rel}: level {level} is not 1-4 (Topic levels)")
    section = rel.split("/", 1)[0]
    if section in SECTION_TYPES and typ not in SECTION_TYPES[section]:
        out.append(f"{rel}: a {typ} page in {section}/, which holds {'/'.join(sorted(SECTION_TYPES[section]))} (Principle 7)")
    words = prose_words(text)
    lo, hi = BUDGET[typ]
    if words < FLOOR:
        out.append(f"{rel}: {words} words, under the {FLOOR}-word floor — merge into its parent (Principle 15)")
    elif hi is not None and words > hi:
        out.append(f"{rel}: {words} words, over the {typ} budget of {hi} (Principle 15)")
    elif words < lo:
        out.append(f"{rel}: {words} words, under the {typ} budget of {lo} (Principle 15)")
    lines = unprobed_steps(text)
    if lines:
        shown = ", ".join(str(n) for n in lines[:5]) + (" …" if len(lines) > 5 else "")
        out.append(f"{rel}: {len(lines)} shell step(s) with no probe fence (line {shown}) (Principle 14)")
    return out


def level_findings(pages: dict[str, str]) -> list[str]:
    """"Needs first" links that point at a higher level (Topic levels)."""
    levels = {rel: d[1] for rel, text in pages.items() if (d := declaration(text))}
    out = []
    for rel, text in pages.items():
        mine = levels.get(rel)
        if mine is None:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            m = NEEDS.match(line)
            if not m:
                continue
            for link in LINK.findall(m.group(1)):
                target = resolve(rel, link)
                theirs = levels.get(target)
                if theirs is not None and theirs > mine:
                    out.append(f"{rel}:{n}: level {mine} page needs {target}, a level {theirs} page (Topic levels)")
    return out


def topic_findings(pages: dict[str, str]) -> list[str]:
    """Two pages of one type for one declared topic (Principle 7)."""
    seen: dict[tuple[str, str], list[str]] = defaultdict(list)
    for rel, text in sorted(pages.items()):
        d = declaration(text)
        if d and d[2]:
            seen[(d[2], d[0])].append(rel)
    return [f"{', '.join(rels)}: {len(rels)} {typ} pages for topic {topic!r} (Principle 7)"
            for (topic, typ), rels in seen.items() if len(rels) > 1]


def admitted(row: dict, status: str) -> bool:
    """RFC-0115 D4 as RFC-0124 Principle 3 states it: a Verified row, or an
    emulator twin of a named board."""
    return status.startswith("Verified") or bool(row.get("emulator") and row.get("hardware"))


def board_findings(rows: list[tuple[dict, str]]) -> list[str]:
    """Board pages whose every row is unadmitted (Principle 3). `rows` pairs
    each registry row with its status word; a row names its page by
    `book_run` / `book_flash`."""
    pages: dict[str, list[bool]] = defaultdict(list)
    names: dict[str, list[str]] = defaultdict(list)
    for row, status in rows:
        for key in ("book_run", "book_flash"):
            if row.get(key):
                page = row[key].split("#", 1)[0]
                pages[page].append(admitted(row, status))
                names[page].append(f"{row.get('crate', '?')}: {status}")
    return [f"{page}: no row admits it to the user book ({'; '.join(sorted(set(names[page])))}) (Principle 3)"
            for page, oks in sorted(pages.items()) if not any(oks)]


def findings(pages: dict[str, str], rows: list[tuple[dict, str]]) -> list[str]:
    out = []
    for rel, text in sorted(pages.items()):
        out += page_findings(rel, text)
    return out + level_findings(pages) + topic_findings(pages) + board_findings(rows)


def self_test() -> None:
    page = "# T\n\n<!-- type: how-to · level: 2 -->\n\n" + "word " * 400 + "\n```bash\nnros build\n```\n" \
           "```bash probe=10\nnros run\n```\n```text\nout\n```\n"
    assert declaration(page) == ("how-to", 2, None)
    assert declaration("# T\n<!-- type: tutorial · level: 1 · topic: rmw -->\n") == ("tutorial", 1, "rmw")
    assert prose_words(page) == 402, prose_words(page)  # "#", "T" and 400
    assert unprobed_steps(page) == [6], unprobed_steps(page)
    f = page_findings("guide/x.md", page)
    assert len(f) == 1 and "no probe fence (line 6)" in f[0], f
    assert "no type" in page_findings("a.md", "# T\n")[0]
    assert "malformed" in page_findings("a.md", "# T\n<!-- type: how-to -->\n")[0]
    assert "Principle 7" in page_findings("tutorial/x.md", page)[0]
    short = "# T\n<!-- type: how-to · level: 1 -->\nfew words\n"
    assert "floor" in page_findings("a.md", short)[0]
    assert "budget of 1500" in page_findings("a.md", "# T\n<!-- type: how-to · level: 1 -->\n" + "w " * 1600)[0]
    assert resolve("tutorial/a.md", "../guide/b.md") == "guide/b.md"
    pages = {"tutorial/a.md": "# A\n<!-- type: tutorial · level: 1 · topic: t -->\n**Needs first:** [b](../guide/b.md)\n",
             "guide/b.md": "# B\n<!-- type: how-to · level: 3 -->\n",
             "guide/c.md": "# C\n<!-- type: tutorial · level: 1 · topic: t -->\n"}
    lf = level_findings(pages)
    assert len(lf) == 1 and "tutorial/a.md:3: level 1 page needs guide/b.md, a level 3" in lf[0], lf
    assert len(topic_findings(pages)) == 1
    rows = [({"crate": "a", "book_run": "t/a.md#run"}, "Verified on host (merge)"),
            ({"crate": "b", "book_run": "t/b.md#run"}, "Boots, manual (2026-11-03)"),
            ({"crate": "c", "book_run": "t/c.md#r", "emulator": "wokwi:pico", "hardware": "pico"}, "Boots, manual (x)"),
            ({"crate": "d", "book_flash": "t/a.md#f"}, "Builds")]
    bf = board_findings(rows)
    assert len(bf) == 1 and bf[0].startswith("t/b.md:"), bf


if __name__ == "__main__":
    self_test()
    print("book_charter self-test: OK")
