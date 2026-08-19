#!/usr/bin/env python3
"""The book's USER track teaches no `just` — it is a contributor dependency.

phase-368 W12 (issue 0694's sibling): a user has `nros`, their vendor's build
tool, and a shell. Every `just` invocation on the user track either got a
user spelling (the front door `./scripts/bootstrap.sh`, `nros setup`,
`ros2 run rmw_zenoh_cpp rmw_zenohd`, the platform's own tool) or was wrapped
in an explicitly contributor-marked block. This gate keeps it that way.

Scope: tracked .md under book/src/{getting-started,user-guide,start-here,
platform-guides}. Everything else (internals/, porting/, reference/, …) is
contributor-facing and out of scope.

A line INVOKES just when a code-fence line starts with `just ` (optionally
after `$ `, `#`-comment lines excluded) or contains `` `just <word>` `` as an
inline command. English uses ("just works", "just a") never match — the
pattern requires a following recipe-shaped token.

An invocation is ALLOWED when it sits inside a contributor-marked region: any
of the 8 lines above it (same file) contains the marker string "Contributor"
— the sweep marks blocks with "**Contributors (in-tree …):**" leads and
blockquotes. Keep the marker within 8 lines of the command it licenses; a
marker at the top of the page does not exempt the whole page.

Exit 0 when clean; 1 with file:line rows otherwise.
"""

import pathlib
import re
import subprocess
import sys

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
MARKER = "Contributor"
LOOKBACK = 8


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parent.parent
    files = subprocess.run(
        ["git", "ls-files", *TRACKS], cwd=repo, capture_output=True, text=True, check=True
    ).stdout.split()

    bad = []
    for rel in files:
        lines = (repo / rel).read_text().splitlines()
        in_fence = False
        for i, line in enumerate(lines):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            hit = FENCE_CMD.match(line) if in_fence else INLINE_CMD.search(line)
            if not hit:
                continue
            window = lines[max(0, i - LOOKBACK) : i]
            if any(MARKER in w for w in window) or MARKER in line:
                continue
            bad.append((rel, i + 1, line.strip()))

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
