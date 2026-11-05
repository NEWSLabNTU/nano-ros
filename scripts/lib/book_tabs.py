"""Language tabs in the book's markdown (RFC-0124 D9, phase-537 W2).

A page that shows one step in Rust, C and C++ writes the three versions as one
group, and the rendered book shows them as tabs:

    <!-- tabs -->

    #### Rust

    ```rust
    executor.spin_once(10)?;
    ```

    #### C

    ```c
    rclc_executor_spin_some(&executor, 10);
    ```

    <!-- /tabs -->

The group opens and closes with the two HTML comments, each alone on its line
at column 0. Its first non-blank line is a heading whose text is a tab label,
and every heading at that level inside the group starts the next tab. A tab
holds any markdown: prose, several fences, lists. Deeper headings are content;
a heading at that level which is not a label, or a shallower one, is an error.
Without the preprocessor (GitHub's view of the file) a group reads as the same
content under one sub-heading per language, which is the form these pages
took before tabs.

This module is the one parser: `scripts/mdbook-lang-tabs.py` renders the
groups, `scripts/probe/extract-book-steps.py` takes a step's language from the
tab it sits in, and the book gates read through the groups with it. Pure
functions over text, so each caller's self-test needs no tree.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

# Tab label as written -> the language id a probe track and a reader's saved
# choice use. One place, so a new language is one line here.
LABELS = {"Rust": "rust", "C": "c", "C++": "cpp"}

OPEN = "<!-- tabs -->"
CLOSE = "<!-- /tabs -->"
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")


class TabError(ValueError):
    """A malformed group. `line` is 1-based."""

    def __init__(self, line: int, msg: str):
        super().__init__(f"line {line}: {msg}")
        self.line = line


@dataclass
class Tab:
    label: str
    lang: str
    heading: int  # 0-based index of the tab's heading line
    body: list[str] = field(default_factory=list)
    start: int = 0  # 0-based index of the body's first line


@dataclass
class Group:
    open: int  # 0-based index of the `<!-- tabs -->` line
    close: int  # 0-based index of the `<!-- /tabs -->` line
    level: int
    tabs: list[Tab] = field(default_factory=list)


def _fence_step(line: str, fence: tuple[str, int] | None) -> tuple[tuple[str, int] | None, bool]:
    """Advance the fence state over one line. Returns (state, line_is_fence_marker)."""
    m = FENCE.match(line)
    if fence is None:
        if m and not (m.group(1)[0] == "`" and "`" in m.group(2)):
            return (m.group(1)[0], len(m.group(1))), True
        return None, False
    if m and m.group(1)[0] == fence[0] and len(m.group(1)) >= fence[1] and not m.group(2).strip():
        return None, True
    return fence, False


def groups(text: str) -> list[Group]:
    """Every tab group of a page, in order. Raises TabError on a malformed one."""
    lines = text.splitlines()
    out: list[Group] = []
    fence = None
    cur: Group | None = None
    for n, line in enumerate(lines):
        was_in_fence = fence is not None
        fence, marker = _fence_step(line, fence)
        if was_in_fence or marker:
            if cur is not None and cur.tabs:
                cur.tabs[-1].body.append(line)
            elif cur is not None:
                raise TabError(n + 1, "a tab group must start with a tab heading (#### Rust, …)")
            continue
        stripped = line.rstrip()
        if stripped == OPEN:
            if cur is not None:
                raise TabError(n + 1, f"a tab group cannot open inside another (opened at line {cur.open + 1})")
            cur = Group(open=n, close=-1, level=0)
            continue
        if stripped == CLOSE:
            if cur is None:
                raise TabError(n + 1, f"{CLOSE} closes no group")
            if len(cur.tabs) < 2:
                raise TabError(cur.open + 1, "a tab group needs at least two tabs")
            cur.close = n
            out.append(cur)
            cur = None
            continue
        if line.strip() in (OPEN, CLOSE):
            raise TabError(n + 1, f"a tab marker must start at column 0: {line.strip()}")
        if cur is None:
            continue
        h = HEADING.match(line)
        if not cur.tabs:
            if not line.strip():
                continue
            if not h:
                raise TabError(n + 1, "a tab group must start with a tab heading (#### Rust, …)")
            cur.level = len(h.group(1))
        if h and len(h.group(1)) <= cur.level:
            label = h.group(2)
            if len(h.group(1)) < cur.level:
                raise TabError(n + 1, f"heading {line.strip()!r} is shallower than the group's tabs; close the group first")
            if label not in LABELS:
                raise TabError(n + 1, f"tab label {label!r} is not one of {', '.join(LABELS)}")
            if any(t.label == label for t in cur.tabs):
                raise TabError(n + 1, f"two {label} tabs in one group")
            cur.tabs.append(Tab(label=label, lang=LABELS[label], heading=n, start=n + 1))
            continue
        cur.tabs[-1].body.append(line)
    if cur is not None:
        raise TabError(cur.open + 1, f"tab group never closed with {CLOSE}")
    return out


def line_langs(text: str) -> list[str | None]:
    """Per line (0-based), the language id of the tab it lies in, else None."""
    n_lines = len(text.splitlines())
    out: list[str | None] = [None] * n_lines
    for g in groups(text):
        for t in g.tabs:
            for i in range(t.start, t.start + len(t.body)):
                out[i] = t.lang
    return out


def structure_lines(text: str) -> set[int]:
    """0-based indexes of the lines the preprocessor consumes: the two markers
    and every tab heading. They are not in the rendered page."""
    out: set[int] = set()
    for g in groups(text):
        out.update({g.open, g.close}, (t.heading for t in g.tabs))
    return out


def segments(text: str) -> list[str | Group]:
    """The page as alternating plain text (str) and groups, in order."""
    lines = text.splitlines()
    out: list[str | Group] = []
    at = 0
    for g in groups(text):
        out.append("\n".join(lines[at:g.open]))
        out.append(g)
        at = g.close + 1
    out.append("\n".join(lines[at:]))
    return out


def self_test() -> None:
    page = ("# P\n\nintro\n\n" + OPEN + "\n\n#### Rust\n\nrust prose\n\n```rust\nlet x = 1;\n```\n\n"
            "##### deeper heading\n\n#### C\n\n```c\n#### not a heading\n```\n\n" + CLOSE + "\n\nafter\n")
    gs = groups(page)
    assert len(gs) == 1 and [t.lang for t in gs[0].tabs] == ["rust", "c"], gs
    assert gs[0].level == 4
    langs = line_langs(page)
    lines = page.splitlines()
    assert langs[lines.index("let x = 1;")] == "rust"
    assert langs[lines.index("#### not a heading")] == "c"
    assert langs[lines.index("after")] is None and langs[lines.index("intro")] is None
    assert {lines[i] for i in structure_lines(page)} == {OPEN, CLOSE, "#### Rust", "#### C"}
    seg = segments(page)
    assert len(seg) == 3 and isinstance(seg[1], Group) and seg[2].strip() == "after", seg
    # a marker inside a fence is text, not a group
    assert groups("```md\n" + OPEN + "\n#### Rust\n" + CLOSE + "\n```\n") == []
    assert groups("no tabs\n") == []
    bad = {
        OPEN + "\nprose first\n#### Rust\n" + CLOSE: "must start with a tab heading",
        OPEN + "\n#### Rust\nx\n#### Go\ny\n" + CLOSE: "is not one of",
        OPEN + "\n#### Rust\nx\n#### Rust\ny\n" + CLOSE: "two Rust tabs",
        OPEN + "\n#### Rust\nx\n### C\ny\n" + CLOSE: "shallower",
        OPEN + "\n#### Rust\nx\n#### C\n": "never closed",
        OPEN + "\n#### Rust\nx\n" + CLOSE: "at least two tabs",
        CLOSE: "closes no group",
        OPEN + "\n#### Rust\n" + OPEN: "inside another",
        "  " + OPEN: "column 0",
    }
    for text, want in bad.items():
        try:
            groups(text)
        except TabError as e:
            assert want in str(e), (text, str(e))
        else:
            raise AssertionError(f"accepted a malformed group: {text!r}")


if __name__ == "__main__":
    self_test()
    print("book_tabs self-test: OK")
