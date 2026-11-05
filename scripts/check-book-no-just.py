#!/usr/bin/env python3
"""The book's USER track teaches no `just` — it is a contributor dependency.

phase-368 W12 (issue 0694's sibling): a user has `nros`, their vendor's build
tool, and a shell. Every `just` invocation on the user track either got a
user spelling (`nros setup`, `ros2 run rmw_zenoh_cpp rmw_zenohd`, the
platform's own tool) or was wrapped in an explicitly contributor-marked block.
This gate keeps it that way. (phase-517 W2's `check-book-no-checkout` is
stricter about code blocks — no `just` at all in a user chapter's fences —
and also reads porting/; this one still covers inline `just <recipe>` spans.)

Scope: every page of the user book, as `book/page-map.toml` assigns them
(RFC-0123 D3, phase-532 W3): contributor material lives in the Internals book,
so no user directory is out of scope any more.

A line INVOKES just when a code-fence line starts with `just ` (optionally
after `$ `, `#`-comment lines excluded) or contains `` `just <word>` `` as an
inline command. English uses ("just works", "just a") never match — the
pattern requires a following recipe-shaped token.

An invocation is ALLOWED only as quoted tool output: the line itself or any
of the 8 lines above it carries the escape `no-just-ok` (an HTML comment), or
the nearest heading or table header does. Since RFC-0123 D3 there is no
contributor exemption: contributor material lives in the Internals book.
A marker at the top of the page does NOT exempt the page; only the nearest
heading does, and only when the heading itself says so.

Exit 0 when clean; 1 with file:line rows otherwise.
"""

import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "lib"))
import book_contributor  # noqa: E402  the verbatim escape (`no-just-ok`)

import book_pages  # noqa: E402  the scope: every user-book page (RFC-0123 D3)

# `just <recipe>` in command position inside a fence, or `` `just <recipe>` ``
# inline. The recipe token must look like a recipe (letters/dash/underscore,
# or `--flag`), which is what keeps "just works" / "just a moment" out.
FENCE_CMD = re.compile(r"^\s*(?:\$\s+)?just\s+[a-z_-]{2,}")
INLINE_CMD = re.compile(r"`just\s+[a-z_-]{2,}[^`]*`")


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parent.parent
    files = book_pages.tracked_user_pages()

    bad = []
    for rel in files:
        lines = (repo / rel).read_text().splitlines()
        for ln in book_contributor.walk(lines, extra_markers=("no-just-ok",)):
            line = ln.text
            hit = FENCE_CMD.match(line) if ln.in_fence else INLINE_CMD.search(line)
            if hit and not ln.exempt:
                bad.append((rel, ln.number, line.strip()))

    if not bad:
        print(f"check-book-no-just: OK ({len(files)} user-track page(s))")
        return 0
    print(
        f"check-book-no-just: {len(bad)} unmarked `just` invocation(s) on the user track:",
        file=sys.stderr,
    )
    for rel, ln, text in bad:
        print(f"  {rel}:{ln}: {text[:90]}", file=sys.stderr)
    print(
        "\nUsers do not have `just`. Either give the command a user spelling\n"
        "(nros / bootstrap.sh / the platform's own tool / `ros2 run\n"
        "rmw_zenoh_cpp rmw_zenohd`), or move the contributor material to the\n"
        "Internals book (book-internals/, RFC-0123). Quoted tool output may\n"
        "carry a `<!-- no-just-ok -->` escape.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
