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

phase-517 W1 widened the reach by one class: an ABSOLUTE link into THIS
repository's own tree (`https://github.com/NEWSLabNTU/nano-ros/blob/main/<path>`,
or `tree/main/`) is a relative link with a longer spelling — it names a path in
the checkout, it 404s the moment that path moves, and six were dead (four
archived issues, two archived phases) while this gate reported OK. Those are
resolved against the tree too. A chapter the book's own table of contents marks
dormant or retired (`[ESP32 (esp-hal, dormant)](…)` in `SUMMARY.md`) is not
held to that arm: it is parked text, kept unchanged on purpose (issue 1525).

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
# issue 1615 (W6): a REFERENCE definition (`[id]: ./page.md`) is a link too —
# `[text][id]` resolves through it, and only the inline form was read. Same
# spelling as `check-markdown-links.py`'s REFDEF.
REFDEF_RE = re.compile(r"^ {0,3}\[[^\]\n]+\]:[ \t]*<?([^\s>#]+)(?:#[^\s>]*)?>?", re.M)

# Not ours to resolve: absolute URLs, protocol-relative, mail, and same-page
# anchors (which LINK_RE's optional group already strips to an empty target).
EXTERNAL = re.compile(r"^(?:[a-z][a-z0-9+.-]*:|//)", re.I)

# `just book` generates this tree (rustdoc + doxygen copied into
# `book/book/api/`); the source tree never has it, so its absence means
# nothing. Anchored at a path boundary so `api/rust/…` counts as well as
# `../api/rust/…`.
GENERATED_API = re.compile(r"(?:^|/)api/.*\.html$")

# phase-517 W1 — an absolute link into this repo's own default-branch tree.
OWN_TREE = re.compile(r"^https://github\.com/NEWSLabNTU/nano-ros/(?:blob|tree)/main/([^?#]+)$")

# A SUMMARY.md entry whose title ends "(…, dormant)" or "(…, retired)".
PARKED = re.compile(r"\[[^\]]*\((?:[^)]*,\s*)?(?:dormant|retired)\)\]\(\./([^)]+)\)", re.I)


def self_test() -> None:
    """Both link spellings are read (issue 1615)."""
    assert [m.group(1) for m in REFDEF_RE.finditer("x\n[r]: ./p.md#a\n")] == ["./p.md"]
    assert [m.group(1) for m in LINK_RE.finditer("[t](./q.md)")] == ["./q.md"]
    url = "https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/x.md"
    assert OWN_TREE.match(url).group(1) == "docs/x.md"
    assert not OWN_TREE.match("https://github.com/other/repo/blob/main/docs/x.md")
    assert PARKED.findall("- [ESP32 (esp-hal, dormant)](./getting-started/esp32.md)") == [
        "getting-started/esp32.md"
    ]
    assert not PARKED.findall("- [Retired names](./reference/retired-names.md)")


def main() -> int:
    self_test()
    repo_root = pathlib.Path(__file__).resolve().parent.parent
    listed = subprocess.run(
        ["git", "ls-files", "book/src/**/*.md", "book/src/*.md"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()

    summary = (repo_root / "book/src/SUMMARY.md").read_text()
    parked = {f"book/src/{p}" for p in PARKED.findall(summary)}

    dead = []
    links = 0
    own = 0
    for rel in listed:
        page = repo_root / rel
        text = page.read_text()
        for match in [*LINK_RE.finditer(text), *REFDEF_RE.finditer(text)]:
            link = match.group(1)
            tree = OWN_TREE.match(link)
            if tree:
                if rel in parked:
                    continue
                own += 1
                if not (repo_root / tree.group(1).rstrip("/")).exists():
                    dead.append((rel, link))
                continue
            if not link or EXTERNAL.match(link) or link.startswith("/"):
                continue
            if GENERATED_API.search(link):
                continue
            links += 1
            if not (page.parent / link).resolve().exists():
                dead.append((rel, link))

    if not dead:
        print(
            f"check-book-links: OK ({links} relative + {own} own-tree link(s) "
            f"in {len(listed)} page(s))"
        )
        return 0

    print(
        f"check-book-links: {len(dead)} link(s) into this tree resolve to nothing:",
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
