#!/usr/bin/env python3
"""The book's USER track teaches no `just` — it is a contributor dependency.

phase-368 W12 (issue 0694's sibling): a user has `nros`, their vendor's build
tool, and a shell. Every `just` invocation on the user track either got a
user spelling (`nros setup`, `ros2 run rmw_zenoh_cpp rmw_zenohd`, the
platform's own tool) or was wrapped in an explicitly contributor-marked block.
This gate keeps it that way. (phase-517 W2's `check-book-no-checkout` is
stricter about code blocks — no `just` at all in a user chapter's fences —
and also reads porting/; this one still covers inline `just <recipe>` spans.)

Scope: tracked .md under book/src/{getting-started,user-guide,start-here,
platform-guides}. Everything else (internals/, porting/, reference/, …) is
contributor-facing and out of scope.

A line INVOKES just when a code-fence line starts with `just ` (optionally
after `$ `, `#`-comment lines excluded) or contains `` `just <word>` `` as an
inline command. English uses ("just works", "just a") never match — the
pattern requires a following recipe-shaped token.

An invocation is ALLOWED when its context is contributor-marked, where
context is any of (checked in this order):

* the line itself or any of the 8 lines above it contains "contributor"
  (case-insensitive) or the explicit escape hatch "no-just-ok" (an HTML
  comment for quoted tool output and other verbatim text);
* the nearest preceding markdown heading contains "contributor" — a
  "## Contributor setup" section licenses its whole body;
* for a table row: the current table's header row contains "contributor" —
  a "recipes (contributors)" column licenses the rows under it.

A marker at the top of the page does NOT exempt the page; only the nearest
heading does, and only when the heading itself says so.

Exit 0 when clean; 1 with file:line rows otherwise.
"""

import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "lib"))
import book_contributor  # noqa: E402  the exemption, shared with check-user-surface-ids

TRACKS = [
    "book/src/getting-started/*.md",
    "book/src/user-guide/*.md",
    "book/src/start-here/*.md",
    "book/src/platform-guides/*.md",
]

# `just <recipe>` in command position inside a fence, or `` `just <recipe>` ``
# inline. The recipe token must look like a recipe (letters/dash/underscore,
# or `--flag`), which is what keeps "just works" / "just a moment" out.
FENCE_CMD = re.compile(r"^\s*(?:\$\s+)?just\s+[a-z_-]{2,}")
INLINE_CMD = re.compile(r"`just\s+[a-z_-]{2,}[^`]*`")


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parent.parent
    files = subprocess.run(
        ["git", "ls-files", *TRACKS], cwd=repo, capture_output=True, text=True, check=True
    ).stdout.split()

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
        "rmw_zenoh_cpp rmw_zenohd`) or mark the block contributor-only —\n"
        'a "**Contributors (…):**" lead within 8 lines. Contributor content\n'
        "that dominates a page belongs in internals/ instead.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
