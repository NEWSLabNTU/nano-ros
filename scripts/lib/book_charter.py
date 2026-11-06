"""The Book charter's checks (RFC-0124 Principles and Topic levels, phase-537 W0).

Every hand-written user page carries one declaration line after its title:

    <!-- type: how-to · level: 2 -->

`type` is one of the four Diátaxis types (RFC-0124 D2) and `level` one of the
four topic levels. An optional `topic: <slug>` names the topic, so two pages of
one type for one topic can be found (Principle 7).

`findings(...)` returns one `Finding` per departure from a gated principle or
the level rule. phase-537 W0 reported them; since W13 `check-book-audience`
REFUSES each one, against a ratchet baseline (`ratchet`, below, and
`.config/book-charter-baseline.txt`) that lists the findings left in pages
other open work owns. The rules that already refuse elsewhere (internal ids,
`nros dev`, typed board facts) are not repeated here.

A finding's `key` (`<subject> <rule>`) names it without a line number, so an
edit above it does not move it. A rule that counts (shell steps with no probe
fence) carries the count beside the key, and the baseline holds the count it
may not rise above.

Pure functions over (path, text) pairs plus the registry rows the caller
loaded, so the self-test needs no tree.
"""

from __future__ import annotations

import re
from collections import defaultdict
from dataclasses import dataclass

import book_tabs

TYPES = ("tutorial", "how-to", "explanation", "reference")
LEVELS = (1, 2, 3, 4)
# RFC-0124 Topic levels: the name of each level, as the rendered badge shows it
# (scripts/mdbook-level-badge.py).
LEVEL_NAMES = {1: "First contact", 2: "Building real applications", 3: "Production and tuning",
               4: "Extending nano-ros"}

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


@dataclass(frozen=True)
class Finding:
    """One departure from the charter. `subject` is a book/src/ page (or
    `topic:<slug>`), `rule` a word that names the rule (with its argument
    after a colon, such as the linked page), `count` the number of instances
    for a rule that counts them. `text` is what a person reads."""
    subject: str
    rule: str
    text: str
    count: int | None = None

    @property
    def key(self) -> str:
        return f"{self.subject} {self.rule}"

    def row(self) -> str:
        """The finding as a baseline row, without its reason."""
        return self.key if self.count is None else f"{self.key}={self.count}"

    def __str__(self) -> str:
        return self.text


def declaration(text: str) -> tuple[str, int, str | None] | None:
    """The page's (type, level, topic), or None when it declares none."""
    for line in text.splitlines()[:6]:
        m = DECL.match(line.strip())
        if m:
            return m.group(1), int(m.group(2)), m.group(3)
    return None


def prose_words(text: str) -> int:
    """Words outside fences and HTML comments: what a reader reads as prose.

    A language tab group (RFC-0124 D9) counts once, as its longest tab: a
    reader reads one language. Its markers and tab headings are not prose.
    A malformed group counts as plain text; check-book-audience refuses it."""
    try:
        segs = book_tabs.segments(text)
    except book_tabs.TabError:
        return _plain_words(text)
    return sum(max(_plain_words("\n".join(t.body)) for t in s.tabs)
               if isinstance(s, book_tabs.Group) else _plain_words(s) for s in segs)


def _plain_words(text: str) -> int:
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


def page_findings(rel: str, text: str) -> list[Finding]:
    """The checks that read one page. `rel` is relative to book/src/."""
    d = declaration(text)
    if d is None:
        bad = any(ANY_DECL.match(l.strip()) for l in text.splitlines()[:6])
        return [Finding(rel, "declaration",
                        f"{rel}: {'a malformed' if bad else 'no'} type and level declaration (RFC-0124 D2)")]
    typ, level, _ = d
    out = []
    if typ not in TYPES:
        return [Finding(rel, "declaration", f"{rel}: type {typ!r} is not one of {', '.join(TYPES)} (D2)")]
    if level not in LEVELS:
        out.append(Finding(rel, "level", f"{rel}: level {level} is not 1-4 (Topic levels)"))
    section = rel.split("/", 1)[0]
    if section in SECTION_TYPES and typ not in SECTION_TYPES[section]:
        out.append(Finding(rel, "section-type",
                           f"{rel}: a {typ} page in {section}/, which holds "
                           f"{'/'.join(sorted(SECTION_TYPES[section]))} (Principle 7)"))
    words = prose_words(text)
    lo, hi = BUDGET[typ]
    if words < FLOOR:
        out.append(Finding(rel, "floor", f"{rel}: {words} words, under the {FLOOR}-word floor — merge into its "
                                         "parent (Principle 15)"))
    elif hi is not None and words > hi:
        out.append(Finding(rel, "over-budget", f"{rel}: {words} words, over the {typ} budget of {hi} (Principle 15)"))
    elif words < lo:
        out.append(Finding(rel, "under-budget", f"{rel}: {words} words, under the {typ} budget of {lo} (Principle 15)"))
    lines = unprobed_steps(text)
    if lines:
        shown = ", ".join(str(n) for n in lines[:5]) + (" …" if len(lines) > 5 else "")
        out.append(Finding(rel, "unprobed-steps",
                           f"{rel}: {len(lines)} shell step(s) with no probe fence (line {shown}) (Principle 14)",
                           len(lines)))
    return out


def level_findings(pages: dict[str, str]) -> list[Finding]:
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
                    out.append(Finding(rel, f"needs-first:{target}",
                                       f"{rel}:{n}: level {mine} page needs {target}, a level {theirs} page "
                                       "(Topic levels)"))
    return out


def topic_findings(pages: dict[str, str]) -> list[Finding]:
    """Two pages of one type for one declared topic (Principle 7)."""
    seen: dict[tuple[str, str], list[str]] = defaultdict(list)
    for rel, text in sorted(pages.items()):
        d = declaration(text)
        if d and d[2]:
            seen[(d[2], d[0])].append(rel)
    return [Finding(f"topic:{topic}", f"duplicate-{typ}",
                    f"{', '.join(rels)}: {len(rels)} {typ} pages for topic {topic!r} (Principle 7)")
            for (topic, typ), rels in seen.items() if len(rels) > 1]


def admitted(row: dict, status: str) -> bool:
    """RFC-0115 D4 as RFC-0124 Principle 3 states it: a Verified row, or an
    emulator twin of a named board."""
    return status.startswith("Verified") or bool(row.get("emulator") and row.get("hardware"))


def board_findings(rows: list[tuple[dict, str]]) -> list[Finding]:
    """Board pages whose every row is unadmitted (Principle 3). `rows` pairs
    each registry row with its status word; a row names its page by
    `book_run` / `book_flash`, and its User guide platform page by
    `book_platform` (phase-537 W9), so a platform page that takes over from
    an admitted row's starter is admitted by that row."""
    pages: dict[str, list[bool]] = defaultdict(list)
    names: dict[str, list[str]] = defaultdict(list)
    for row, status in rows:
        for key in ("book_run", "book_flash", "book_platform"):
            if row.get(key):
                page = row[key].split("#", 1)[0]
                pages[page].append(admitted(row, status))
                names[page].append(f"{row.get('crate', '?')}: {status}")
    return [Finding(page, "unadmitted-board",
                    f"{page}: no row admits it to the user book ({'; '.join(sorted(set(names[page])))}) (Principle 3)")
            for page, oks in sorted(pages.items()) if not any(oks)]


# phase-537 W9 (RFC-0124 D8) — the User guide's platform pages are grouped by
# their row's generated status word, in this order (RFC-0124 Appendix A). A
# twin of a named `hardware` board is its own group whatever its word says,
# as its starter is (D12).
PLATFORM_GROUPS = ("Verified on host", "Verified in emulator", "Verified on hardware",
                   "Twin and hardware")
PLATFORMS_HEADING = "Platforms"
SUMMARY_ENTRY = re.compile(r"^(\s*)-\s*\[(.+?)\]\((?:\./)?([^)#]*)\)\s*$")


def platform_group(row: dict, status: str) -> str | None:
    """The platform group an admitted row's page sits in, or None for a row
    RFC-0124 D8 does not admit (its generated reference page only)."""
    if row.get("emulator") and row.get("hardware"):
        return "Twin and hardware"
    for where in ("on host", "in emulator", "on hardware"):
        if status.startswith(f"Verified {where}"):
            return f"Verified {where}"
    return None


def platform_pages(rows: list[tuple[dict, str]]) -> tuple[dict[str, str], list[Finding]]:
    """({page: group} for every row's `book_platform`, findings). A page whose
    rows name two groups, or a row the book does not admit, is a finding: the
    page splits by status (D8), or the row keeps its reference page only."""
    groups: dict[str, set[str]] = defaultdict(set)
    out: list[Finding] = []
    for row, status in rows:
        page = (row.get("book_platform") or "").split("#", 1)[0]
        if not page:
            continue
        g = platform_group(row, status)
        if g is None:
            out.append(Finding(page, f"platform-unadmitted:{row.get('crate', '?')}",
                               f"{page}: `{row.get('crate', '?')}` ({status}) names it as its platform page, "
                               "but the row is not admitted to the user book (Principle 3, D8)"))
            continue
        groups[page].add(g)
    placed = {}
    for page, gs in sorted(groups.items()):
        if len(gs) > 1:
            out.append(Finding(page, "platform-split",
                               f"{page}: its rows sit in {len(gs)} status groups ({', '.join(sorted(gs))}); "
                               "split the page by status (D8)"))
        placed[page] = min(gs, key=PLATFORM_GROUPS.index)
    return placed, out


def summary_platforms(summary: str) -> dict[str, str | None]:
    """{page: status group, or None when directly under Platforms} for every
    page SUMMARY.md lists in the User guide's Platforms group."""
    out: dict[str, str | None] = {}
    base = None  # indent of the "Platforms" entry while inside it
    group = None
    for line in summary.splitlines():
        m = SUMMARY_ENTRY.match(line)
        if not m:
            if line.startswith("#"):
                base = None
            continue
        indent, title, target = len(m.group(1)), m.group(2), m.group(3)
        if base is None:
            if title == PLATFORMS_HEADING and not target:
                base, group = indent, None
            continue
        if indent <= base:
            base = None
            if title == PLATFORMS_HEADING and not target:
                base, group = indent, None
            continue
        if not target:
            group = title
            continue
        if target.startswith("guide/platforms/"):
            out[target] = group if indent > base + 2 and group else None
    return out


def platform_findings(summary: str, rows: list[tuple[dict, str]]) -> list[Finding]:
    """A platform page whose SUMMARY group is not its row's generated status
    group, or a page placed in a status group no row names (D8)."""
    want, out = platform_pages(rows)
    have = summary_platforms(summary)
    for page, g in sorted(want.items()):
        if page not in have:
            out.append(Finding(page, "platform-group",
                               f"{page}: its row places it in Platforms › {g}, and SUMMARY.md does not list it "
                               "there (D8)"))
        elif have[page] != g:
            out.append(Finding(page, "platform-group",
                               f"{page}: SUMMARY.md places it in Platforms › {have[page] or '(no group)'}, its "
                               f"row's status word places it in {g} (D8)"))
    for page, g in sorted(have.items()):
        if g is not None and page not in want:
            out.append(Finding(page, "platform-typed",
                               f"{page}: SUMMARY.md places it in Platforms › {g}, and no registry row names it "
                               "as its platform page (`book_platform`): a status group is generated, never "
                               "typed (D8)"))
        elif g is not None and g not in PLATFORM_GROUPS:
            out.append(Finding(page, "platform-bad-group",
                               f"{page}: Platforms › {g} is not a status group ({', '.join(PLATFORM_GROUPS)}) (D8)"))
    return out


def findings(pages: dict[str, str], rows: list[tuple[dict, str]], summary: str | None = None) -> list[Finding]:
    out = []
    for rel, text in sorted(pages.items()):
        out += page_findings(rel, text)
    out += level_findings(pages) + topic_findings(pages) + board_findings(rows)
    return out + (platform_findings(summary, rows) if summary is not None else [])


# phase-537 W13 — the charter refuses, against a ratchet baseline. A row is
#     <subject> <rule>[=<count>]  # <reason or owner>
# with the key `Finding.row()` prints. The baseline only shrinks: a finding not
# in it fails, a row that no longer fires fails until it is deleted, and a
# counted row fails both when the count rises and when it falls (lower it).
BASELINE_ROW = re.compile(r"^(\S+) (\S+?)(?:=(\d+))?\s+#\s*(\S.*)$")


def read_baseline(text: str) -> tuple[dict[str, tuple[int | None, str]], list[str]]:
    """({key: (count, reason)}, problems) from a baseline file's text. A row
    with no reason, a malformed row or a repeated key is a problem."""
    rows: dict[str, tuple[int | None, str]] = {}
    bad = []
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = BASELINE_ROW.match(line.strip())
        if not m:
            bad.append(f"line {n}: not `<page> <rule>[=<count>]  # <reason>`: {line.strip()[:100]}")
            continue
        key = f"{m.group(1)} {m.group(2)}"
        if key in rows:
            bad.append(f"line {n}: `{key}` is listed twice")
            continue
        rows[key] = (int(m.group(3)) if m.group(3) else None, m.group(4).strip())
    return rows, bad


def ratchet(found: list[Finding], baseline_text: str, where: str = "the baseline"
            ) -> tuple[list[str], dict[str, int]]:
    """(refusals, {reason: findings baselined}) for `found` against the
    baseline's text."""
    rows, errs = read_baseline(baseline_text)
    errs = [f"{where}: {e}" for e in errs]
    by_key: dict[str, list[Finding]] = defaultdict(list)
    for f in found:
        by_key[f.key].append(f)
    kept: dict[str, int] = defaultdict(int)
    for key, fs in sorted(by_key.items()):
        if key not in rows:
            errs += [f"{f.text} — fix it: the baseline holds only the findings left when the charter began refusing" for f in fs]
            continue
        want, reason = rows[key]
        have = fs[0].count
        if (want is None) != (have is None):
            errs.append(f"{where}: `{key}` " + (f"counts: write it `{key}={have}`" if have is not None
                                                 else f"takes no count: drop `={want}`"))
            continue
        if have is not None and have > want:
            errs.append(f"{fs[0].text} — {where} allows {want}: fix the new one(s)")
            continue
        if have is not None and have < want:
            errs.append(f"{where}: `{key}={want}` now counts {have} — lower it to `{key}={have}`; "
                        "the baseline only shrinks")
            continue
        kept[reason] += 1
    for key in sorted(set(rows) - set(by_key)):
        row = key if rows[key][0] is None else f"{key}={rows[key][0]}"
        errs.append(f"{where}: `{row}` no longer fires — remove it; the baseline only shrinks")
    return errs, dict(kept)


def self_test() -> None:
    page = "# T\n\n<!-- type: how-to · level: 2 -->\n\n" + "word " * 400 + "\n```bash\nnros build\n```\n" \
           "```bash probe=10\nnros run\n```\n```text\nout\n```\n"
    clean = page.replace("```bash\nnros build", "```bash probe=5\nnros build")
    assert declaration(page) == ("how-to", 2, None)
    assert declaration("# T\n<!-- type: tutorial · level: 1 · topic: rmw -->\n") == ("tutorial", 1, "rmw")
    assert prose_words(page) == 402, prose_words(page)  # "#", "T" and 400
    tabbed = ("intro words\n<!-- tabs -->\n#### Rust\none two three\n```rust\nx y z\n```\n"
              "#### C\none two\n```bash\nmake\n```\n<!-- /tabs -->\nend\n")
    assert prose_words(tabbed) == 6, prose_words(tabbed)  # intro words + the Rust tab's three + end
    assert unprobed_steps(tabbed) == [10], unprobed_steps(tabbed)  # each fence is still a fence
    assert unprobed_steps(page) == [6], unprobed_steps(page)

    def rules(rel: str, text: str) -> list[str]:
        return [f.rule for f in page_findings(rel, text)]

    # The clean page trips nothing, in a section whose type it fits.
    assert page_findings("guide/x.md", clean) == [], page_findings("guide/x.md", clean)
    # Principle 14: a shell step with no probe fence, counted.
    f = page_findings("guide/x.md", page)
    assert [(x.rule, x.count) for x in f] == [("unprobed-steps", 1)] and "no probe fence (line 6)" in f[0].text, f
    assert f[0].key == "guide/x.md unprobed-steps" and f[0].row() == "guide/x.md unprobed-steps=1"
    # D2: a missing, malformed or unknown declaration; a level outside 1-4.
    assert rules("a.md", "# T\n") == ["declaration"] and "no type" in page_findings("a.md", "# T\n")[0].text
    assert "malformed" in page_findings("a.md", "# T\n<!-- type: how-to -->\n")[0].text
    assert rules("a.md", clean.replace("how-to ·", "guide ·")) == ["declaration"]
    assert rules("a.md", clean.replace("level: 2", "level: 7")) == ["level"]
    # Principle 7: a type that does not fit its section.
    assert rules("tutorial/x.md", clean) == ["section-type"], rules("tutorial/x.md", clean)
    assert rules("guide/x.md", clean) == []
    # Principle 15: the floor, and a budget outside Reference only.
    short = "# T\n<!-- type: how-to · level: 1 -->\nfew words\n"
    assert rules("guide/a.md", short) == ["floor"] and "floor" in page_findings("a.md", short)[0].text
    long_how_to = "# T\n<!-- type: how-to · level: 1 -->\n" + "w " * 1600
    assert rules("guide/a.md", long_how_to) == ["over-budget"]
    assert "budget of 1500" in page_findings("a.md", long_how_to)[0].text
    assert rules("tutorial/a.md", "# T\n<!-- type: tutorial · level: 1 -->\n" + "w " * 400) == ["under-budget"]
    assert rules("reference/a.md", long_how_to.replace("how-to", "reference") + "w " * 5000) == []
    # Topic levels: a "needs first" link to a higher level, and not to a lower one.
    assert resolve("tutorial/a.md", "../guide/b.md") == "guide/b.md"
    pages = {"tutorial/a.md": "# A\n<!-- type: tutorial · level: 1 · topic: t -->\n**Needs first:** [b](../guide/b.md)\n",
             "guide/b.md": "# B\n<!-- type: how-to · level: 3 -->\n",
             "guide/c.md": "# C\n<!-- type: tutorial · level: 1 · topic: t -->\n"}
    lf = level_findings(pages)
    assert len(lf) == 1 and "tutorial/a.md:3: level 1 page needs guide/b.md, a level 3" in lf[0].text, lf
    assert lf[0].key == "tutorial/a.md needs-first:guide/b.md", lf[0].key
    assert not level_findings({**pages, "guide/b.md": "# B\n<!-- type: how-to · level: 1 -->\n"})
    # Principle 7: two pages of one type for one topic, and two types for one.
    tf = topic_findings(pages)
    assert len(tf) == 1 and tf[0].key == "topic:t duplicate-tutorial", tf
    assert not topic_findings({**pages, "guide/c.md": "# C\n<!-- type: how-to · level: 1 · topic: t -->\n"})
    # Principle 3: a board page no row admits.
    rows = [({"crate": "a", "book_run": "t/a.md#run"}, "Verified on host (merge)"),
            ({"crate": "b", "book_run": "t/b.md#run"}, "Boots, manual (2026-11-03)"),
            ({"crate": "c", "book_run": "t/c.md#r", "emulator": "wokwi:pico", "hardware": "pico"}, "Boots, manual (x)"),
            ({"crate": "d", "book_flash": "t/a.md#f"}, "Builds")]
    bf = board_findings(rows)
    assert len(bf) == 1 and bf[0].text.startswith("t/b.md:") and bf[0].key == "t/b.md unadmitted-board", bf
    assert not board_findings(rows[:1] + rows[2:]), "an admitted row, or a twin of a named board"
    # phase-537 W9: a platform page an admitted row names is admitted by it.
    assert not board_findings(rows[:1] + [({"crate": "e", "book_platform": "t/b.md"}, "Verified on host (merge)")]
                              + rows[1:2]), "a platform page of an admitted row"
    # The platform groups come from the rows; SUMMARY must agree.
    host = ({"crate": "h", "book_platform": "guide/platforms/h.md"}, "Verified on host (merge)")
    emu = ({"crate": "q", "book_platform": "guide/platforms/q.md"}, "Verified in emulator (daily)")
    twin = ({"crate": "n", "book_platform": "guide/platforms/n.md", "emulator": "renode:x", "hardware": "n"},
            "Boots, manual (2026-11-01)")
    assert platform_group(*twin) == "Twin and hardware" and platform_group({}, "Builds") is None
    good = ("# User guide\n\n- [Platforms]()\n  - [Verified on host]()\n    - [H](./guide/platforms/h.md)\n"
            "  - [Verified in emulator]()\n    - [Q](./guide/platforms/q.md)\n"
            "  - [Twin and hardware]()\n    - [N](./guide/platforms/n.md)\n"
            "  - [West app](./guide/platforms/w.md)\n- [Real-time]()\n  - [R](./guide/realtime/r.md)\n")
    assert summary_platforms(good) == {"guide/platforms/h.md": "Verified on host",
                                       "guide/platforms/q.md": "Verified in emulator",
                                       "guide/platforms/n.md": "Twin and hardware",
                                       "guide/platforms/w.md": None}, summary_platforms(good)
    assert platform_findings(good, [host, emu, twin]) == []
    moved = good.replace("    - [H](./guide/platforms/h.md)\n", "").replace(
        "    - [Q](./guide/platforms/q.md)\n", "    - [Q](./guide/platforms/q.md)\n    - [H](./guide/platforms/h.md)\n")
    pf = platform_findings(moved, [host, emu, twin])
    assert len(pf) == 1 and "h.md: SUMMARY.md places it in Platforms › Verified in emulator" in pf[0].text, pf
    assert pf[0].key == "guide/platforms/h.md platform-group", pf[0].key
    typed = good.replace("  - [West app](./guide/platforms/w.md)\n",
                         "  - [Verified on hardware]()\n    - [W](./guide/platforms/w.md)\n")
    pf = platform_findings(typed, [host, emu, twin])
    assert len(pf) == 1 and "w.md" in pf[0].text and "never typed" in pf[0].text, pf
    split = ({"crate": "h2", "book_platform": "guide/platforms/h.md"}, "Verified in emulator (daily)")
    assert any("split the page" in f.text for f in platform_findings(good, [host, split, emu, twin]))
    unadmitted = ({"crate": "s", "book_platform": "guide/platforms/s.md"}, "Builds")
    assert any("not admitted" in f.text for f in platform_findings(good, [host, emu, twin, unadmitted]))
    gone = good.replace("    - [N](./guide/platforms/n.md)\n", "")
    assert any("n.md" in f.text and "does not list" in f.text for f in platform_findings(gone, [host, emu, twin]))

    # phase-537 W13: every finding refuses unless the baseline lists it, and
    # the baseline only shrinks.
    found = page_findings("guide/x.md", page) + bf
    base = "# header\n\nguide/x.md unprobed-steps=1  # W6\nt/b.md unadmitted-board  # W4, its twin\n"
    errs, kept = ratchet(found, base)
    assert errs == [] and kept == {"W6": 1, "W4, its twin": 1}, (errs, kept)
    errs, _ = ratchet(found, "guide/x.md unprobed-steps=1  # W6\n")
    assert len(errs) == 1 and errs[0].startswith("t/b.md:") and "fix it" in errs[0], errs  # not listed
    errs, _ = ratchet(bf, base)
    assert errs == ["the baseline: `guide/x.md unprobed-steps=1` no longer fires — remove it; "
                    "the baseline only shrinks"], errs  # stale
    errs, _ = ratchet(found, base.replace("=1", "=2"))
    assert len(errs) == 1 and "lower it to `guide/x.md unprobed-steps=1`" in errs[0], errs  # count fell
    errs, _ = ratchet(page_findings("guide/x.md", page.replace("```bash probe=10", "```bash")) + bf, base)
    assert len(errs) == 1 and "allows 1" in errs[0], errs  # count rose
    errs, _ = ratchet(found, base.replace("=1", ""))
    assert len(errs) == 1 and "counts: write it" in errs[0], errs
    errs, _ = ratchet(found, base.replace("  # W6", ""))
    assert len(errs) == 2 and "not `<page> <rule>" in errs[0], errs  # a row needs its reason
    errs, _ = ratchet(found, base + "t/b.md unadmitted-board  # again\n")
    assert any("listed twice" in e for e in errs), errs


if __name__ == "__main__":
    self_test()
    print("book_charter self-test: OK")
