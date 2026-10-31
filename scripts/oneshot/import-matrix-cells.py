#!/usr/bin/env python3
"""One-shot (phase-522 W2): move `matrix::CELLS` into the board registry.

Reads the hand-written `cell(...)` list of
packages/testing/nros-tests/src/matrix.rs and writes it as the registry's cell
table, packages/boards/board-support-cells.toml: one `[[cell]]` per
invocation, with its five coordinates, its status expression, its `.with(...)`
configuration and the comment block that preceded it (comments carry history,
and RFC-0114 moves them into the registry rather than dropping them).

After this ran once, scripts/gen-board-support.py writes the Rust list from the
TOML and the TOML is what people edit. Kept for the record and so the
migration can be re-derived from git history; it is not run by any gate.

Run:  python3 scripts/oneshot/import-matrix-cells.py <matrix.rs> <out.toml>
"""

import sys
from pathlib import Path


def scan_balanced(text: str, i: int) -> int:
    """Index just past the `)` matching the `(` at text[i], string-aware."""
    assert text[i] == "("
    depth = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise ValueError("unbalanced cell(...)")


def split_top(args: str) -> list[str]:
    """Split on commas outside parens, brackets and strings."""
    out, cur, depth, i = [], [], 0, 0
    while i < len(args):
        c = args[i]
        if c == '"':
            j = i + 1
            while args[j] != '"':
                j += 2 if args[j] == "\\" else 1
            cur.append(args[i:j + 1])
            i = j + 1
            continue
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        if c == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(c)
        i += 1
    out.append("".join(cur))
    return out


def parse_region(body: str) -> tuple[list[dict], list[str]]:
    cells, pending = [], []
    i, n = 0, len(body)
    while i < n:
        nl = body.find("\n", i)
        nl = n if nl < 0 else nl
        line = body[i:nl]
        s = line.strip()
        if s.startswith("cell("):
            start = i + line.index("cell(") + len("cell")
            end = scan_balanced(body, start)
            args = split_top(body[start + 1:end - 1])
            assert len(args) == 6, args
            coords = [a.strip() for a in args[:5]]
            status = args[5].strip()
            j = end
            with_ = None
            if body.startswith(".with(", j):
                wend = scan_balanced(body, j + len(".with"))
                with_ = body[j + len(".with("):wend - 1].strip()
                j = wend
            assert body[j] == ",", body[j:j + 40]
            j += 1
            rest = body[j:body.find("\n", j)]
            assert not rest.strip(), f"trailing text after a cell: {rest!r}"
            cells.append({"at": coords, "status": status, "with": with_,
                          "comment": pending})
            pending = []
            i = body.find("\n", j) + 1
            continue
        assert s == "" or s.startswith("//"), f"unexpected line in CELLS: {line!r}"
        pending.append(s)
        i = nl + 1
    return cells, pending


def toml_str(v: str) -> str:
    if "\n" not in v and "'" not in v:
        return f"'{v}'"
    assert "'''" not in v
    return "'''\n" + v + "'''" if "\n" in v else "'''" + v + "'''"


def main() -> int:
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    text = src.read_text()
    head = "pub const CELLS: &[Cell] = &[\n"
    a = text.index(head) + len(head)
    b = text.index("\n];\n", a) + 1
    cells, tail = parse_region(text[a:b])
    lines = []
    for c in cells:
        lines.append("[[cell]]")
        if c["comment"]:
            lines.append("comment = " + toml_str("\n".join(c["comment"]) + "\n"))
        lines.append("at = [" + ", ".join(f'"{x}"' for x in c["at"]) + "]")
        lines.append("status = " + toml_str(c["status"]))
        if c["with"]:
            lines.append("with = " + toml_str(c["with"]))
        lines.append("")
    if tail:
        lines.append("# trailing comment block of the old list (none expected)")
    out.write_text("\n".join(lines))
    print(f"{len(cells)} cells -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
