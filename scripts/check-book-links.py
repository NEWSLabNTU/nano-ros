#!/usr/bin/env python3
"""Every relative link in the book must resolve to a file that exists.

`check-doc-refs` already guards the NUMBERED SERIES — it asks "does issue
0123 exist?", anywhere in the tree, and deliberately answers yes when the
file has been archived, because the id is what the reference means.

That is a different question from the one a reader asks. A book page linking
`../../../docs/roadmap/phase-115-runtime-transport-vtable.md` names a path
that mdbook resolves literally; when the phase was archived the id still
existed, `check-doc-refs` stayed green, and the link 404'd. Nine links were
dead this way when this gate was written — seven of them not archival at all
but plain depth errors (`../../docs/…` from `book/src/<dir>/`, which is
`book/docs/`, one level short of the repo root). Both classes are invisible
to a gate that resolves by id, and both are invisible to a reader until they
click.

Scope: tracked `book/src/**/*.md`, relative links only. Absolute URLs are
somebody else's uptime. `/api/**.html` links are EXEMPT — `just book`
generates that tree (rustdoc + doxygen output copied into `book/book/api/`),
so it does not exist in the source tree and its absence here means nothing.

Enumeration is `git ls-files`: tracked files only, the same index-driven
discipline the rest of the gates use, so an untracked scratch page cannot
fail the build and a build tree cannot slow it down.

Exit 0 when every link resolves, 1 otherwise.
"""

import pathlib
import re
import subprocess
import sys

# Any inline link target, with an optional `#anchor` that we do not follow —
# an anchor is a heading, and heading drift is a different gate than file
# existence.
#
# Deliberately NOT restricted to `./` and `../`. The first version of this
# gate matched only those two prefixes and reported "469 links OK" while
# skipping every bare-filename link in the book — and the book writes plenty
# of them (`](custom-platform.md)`, `](no-std.md)`, six and six). A gate whose
# green is narrower than its message is the thing it was written to prevent.
LINK_RE = re.compile(r"\]\(([^)#\s]+)(?:#[^)\s]*)?\)")

# Not ours to resolve: absolute URLs, protocol-relative, mail, and same-page
# anchors (which LINK_RE's optional group already strips to an empty target).
EXTERNAL = re.compile(r"^(?:[a-z][a-z0-9+.-]*:|//)", re.I)

# `just book` generates this tree (rustdoc + doxygen copied into
# `book/book/api/`); the source tree never has it, so its absence means
# nothing. Anchored at a path boundary so `api/rust/…` counts as well as
# `../api/rust/…`.
GENERATED_API = re.compile(r"(?:^|/)api/.*\.html$")


def main() -> int:
    repo_root = pathlib.Path(__file__).resolve().parent.parent
    listed = subprocess.run(
        ["git", "ls-files", "book/src/**/*.md", "book/src/*.md"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()

    dead = []
    links = 0
    for rel in listed:
        page = repo_root / rel
        for match in LINK_RE.finditer(page.read_text()):
            link = match.group(1)
            if not link or EXTERNAL.match(link) or link.startswith("/"):
                continue
            if GENERATED_API.search(link):
                continue
            links += 1
            if not (page.parent / link).resolve().exists():
                dead.append((rel, link))

    if not dead:
        print(f"check-book-links: OK ({links} relative link(s) in {len(listed)} page(s))")
        return 0

    print(
        f"check-book-links: {len(dead)} relative link(s) resolve to nothing:",
        file=sys.stderr,
    )
    for rel, link in dead:
        print(f"  {rel}\n      -> {link}", file=sys.stderr)
    print(
        "\nA link is a PATH, not an id. Two things make one dead while"
        "\n`check-doc-refs` stays green:"
        "\n  * the target was ARCHIVED — point at `docs/<series>/archived/…`;"
        "\n  * the depth is wrong — from `book/src/<dir>/page.md` the repo"
        "\n    root is `../../../`, so `../../docs/…` is `book/docs/…`.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
