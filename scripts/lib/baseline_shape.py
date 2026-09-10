"""The shape of a ratchet baseline — the half of the file no gate reads.

A ratchet baseline is rows a gate compares against plus a comment header a
person reads, and every loader in this tree skips `#` lines. So nothing about a
gate's verdict depends on the header, which is exactly why it can be destroyed
with every lane green. It has been, twice:

  * `.config/prose-issue-ref-baseline.txt` (issue 1241) went through `sort` in
    a conflict resolution; its CLASSIFIED block — which rows are debt and which
    are deliberate — ended up interleaved among the rows it classifies.
  * `.config/gate-selftest-baseline.txt` went through `sort -u`: its header
    came back in byte order with one of its two `#` spacers deduplicated away,
    and sat on main that way.

1241 added a check for the first shape to one file. This module is the class,
for every baseline, in one spelling (`scripts/check-baseline-shape.py` runs it):

  RULE 1 — no comment block is in sorted order. Prose is never written sorted;
  a block of MIN_SORTED_TEXT or more text lines that IS sorted — in byte order
  (`LC_ALL=C sort`) or in a locale-style fold (`sort` under en_US ignores
  punctuation and case) — came out of a sort. The threshold is measured, not a
  taste: four authored lines fall in order by chance 1 time in 24 per ordering,
  and a 3-line block of `doc-commit-citations-baseline.txt` does today.

  RULE 2 — the whole file is not sorted. A locale `sort` interleaves comments
  with rows that share a first letter, leaving no block long enough for rule 1.
  That is the 1241 shape.

  RULE 3 — where the WRITER emits a fixed header (`BASELINE_HEADER` in its
  script), the file's leading comment block IS that header, line for line. The
  strongest check, and available only where there is a source of truth to
  compare with, which is why rules 1 and 2 exist for the files that have none.

"Comments first" is NOT a rule here: `doc-commit-citations-baseline.txt`
groups its hashes under per-document explanations on purpose. The one file
whose convention it is enforces it with `late_comment_lines`.
"""

import re

MIN_SORTED_TEXT = 5


def _loose(line):
    """A locale-style collation key: case folded, punctuation dropped."""
    return re.sub(r"[^a-z0-9 ]", "", line.lower()).strip()


ORDERINGS = (("byte order", lambda l: l), ("locale order", _loose))


def _text_lines(lines):
    return [l for l in lines if l.startswith("#") and l.strip("# \t")]


def comment_blocks(lines):
    """(0-based start, lines) for each run of consecutive `#` lines."""
    blocks, start, cur = [], None, []
    for i, line in enumerate(lines):
        if line.startswith("#"):
            if not cur:
                start = i
            cur.append(line)
        elif cur:
            blocks.append((start, cur))
            cur = []
    if cur:
        blocks.append((start, cur))
    return blocks


def _sorted_by(seq):
    for name, key in ORDERINGS:
        keys = [key(l) for l in seq]
        if keys == sorted(keys):
            return name
    return None


def sorted_blocks(lines):
    """[(0-based start, block length, ordering)] for rule 1."""
    hits = []
    for start, block in comment_blocks(lines):
        if len(_text_lines(block)) < MIN_SORTED_TEXT:
            continue
        order = _sorted_by(block)
        if order:
            hits.append((start, len(block), order))
    return hits


def whole_file_sorted(lines):
    """The ordering the whole file is in, for rule 2, or None."""
    body = [l for l in lines if l.strip()]
    if len(_text_lines(body)) < MIN_SORTED_TEXT:
        return None
    return _sorted_by(body)


def leading_header(text):
    """The comment block before the first row, trailing blank lines dropped."""
    head = []
    for line in text.split("\n"):
        if line.startswith("#") or not line.strip():
            head.append(line)
        else:
            break
    while head and not head[-1].strip():
        head.pop()
    return head


def header_diff(text, expected):
    """(1-based line, file's line, writer's line) at the first difference, or None."""
    got, want = leading_header(text), leading_header(expected)
    for i in range(max(len(got), len(want))):
        g = got[i] if i < len(got) else "<end of header>"
        w = want[i] if i < len(want) else "<end of header>"
        if g != w:
            return i + 1, g, w
    return None


def late_comment_lines(lines):
    """0-based indices of `#` lines after the first row (for a comments-first file)."""
    first_row = next((i for i, l in enumerate(lines)
                      if l.strip() and not l.startswith("#")), None)
    if first_row is None:
        return []
    return [i for i, l in enumerate(lines[first_row:], first_row) if l.startswith("#")]


def problems(text, expected_header=None):
    """Every shape problem in one baseline's text, as messages without a path."""
    lines = text.split("\n")
    out = []
    hits = sorted_blocks(lines)
    for start, n, order in hits:
        out.append(
            f"line {start + 1}: its {n}-line comment block is in {order} — the "
            f"shape a `sort` leaves behind; nobody writes prose sorted")
    if not hits:
        order = whole_file_sorted(lines)
        if order:
            out.append(
                f"the whole file is in {order}, comments among the rows — the "
                f"shape a locale `sort` leaves behind (issue 1241)")
    if expected_header is not None:
        d = header_diff(text, expected_header)
        if d:
            n, got, want = d
            out.append(
                f"line {n}: the header differs from the one its writer emits\n"
                f"      file:   {got}\n"
                f"      writer: {want}")
    return out


def selftest():
    """Failures of this module's own negative controls, [] when they all hold."""
    fails = []

    def expect(name, got, want):
        if got != want:
            fails.append(f"{name}: got {got!r}, want {want!r}")

    header = (
        "# Gate scripts that do NOT yet run their own selftest on the normal\n"
        "# path. A RATCHET, not an allowlist: this file may only shrink.\n"
        "#\n"
        "# `check-gate-selftests` fails when a script here gains a selftest\n"
        "# (remove its line) or disappears (delete its line) — so the debt\n"
        "# cannot silently grow and cannot silently go stale, which is the\n"
        "# issue-0743 class.\n"
        "#\n"
        "# Regenerate: python3 scripts/check-gate-selftests.py --write-baseline\n")
    authored = header + "scripts/a.sh\nscripts/b.sh\n"
    # What `LC_ALL=C sort -u` makes of it — the gate-selftest baseline on main.
    sort_u = "\n".join(sorted({l for l in authored.split("\n") if l})) + "\n"

    expect("an authored header passes", problems(authored), [])
    expect("... and matches its writer", problems(authored, header), [])
    expect("`sort -u` trips rule 1", len(problems(sort_u)), 1)
    expect("... and rule 3", len(problems(sort_u, header)), 2)
    expect("`sort -u` dropped a spacer",
           len(leading_header(sort_u)), len(leading_header(header)) - 1)

    # A locale sort interleaves: `# cannot`, `cannot/x`, `# Gate`, `gate/y` ...
    rows = "cannot/x\ngate/y\nissue/z\npath/w\nremove/v\n"
    loc = "\n".join(sorted((l for l in (header + rows).split("\n") if l),
                           key=_loose)) + "\n"
    expect("a locale sort trips rule 2",
           [p.split(",")[0] for p in problems(loc)],
           ["the whole file is in locale order"])

    four = "# alpha\n# beta\n# delta\n# gamma\nrow\n"
    expect("four lines in order by chance are not a sort", problems(four), [])

    sections = header + "aaa\n\n# per-document note, deliberately\n# after rows\nbbb\n"
    expect("comments after rows are legitimate here", problems(sections, header), [])
    expect("late_comment_lines still sees them",
           len(late_comment_lines(sections.split("\n"))), 2)

    spacer_lost = header.replace("#\n# Regenerate", "# Regenerate") + "row\n"
    expect("a lost spacer is a rule 3 difference at its line",
           header_diff(spacer_lost, header)[0], 8)
    return fails
