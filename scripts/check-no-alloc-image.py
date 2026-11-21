#!/usr/bin/env python3
"""The book's no-alloc wording, held to the images the L3 lane checks (issue 0816).

Every line of `book/src/**/*.md` that promises an image free of allocation
must be rostered in `BOOK_CLAIMS` below, and each `image` row must name an
image that `just ci l3` really runs through `nros image check --no-alloc
--tier heap-free` (read from `just/ci.just`, so a row cannot outlive the
check). Every other row says why its sentence is not a promise about a
linked image.

The check of an image itself is the user's verb, `nros image check
--no-alloc` (issue 2667: `nros-cli-core/src/alloc_scan.rs`). This script
read images too until then; it keeps only the book side, and refuses an image
argument by naming the verb, so the check has one spelling.

Usage
-----
    scripts/check-no-alloc-image.py --claims      # the book roster
    scripts/check-no-alloc-image.py --selftest    # prove it fails

Exit codes: 0 every promise backed or classified, 1 a problem, 2 the check
could not be run (no book pages, or an image argument).
"""

import argparse
import contextlib
import io
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ---------------------------------------------------------------------------
# The book's promises (issue 0816). Kept here rather than in a report so the
# answer to "which claim is backed by a check?" ages with the tool instead of
# with a markdown file, and `--claims` is a GATE over it, not a listing:
#
#   * every line of `book/src/**/*.md` matching CLAIM_PHRASE must be rostered
#     here (an unrostered promise is the thing this issue was filed about);
#   * every roster row must still match a book line (a stale row is a claim
#     that moved and is no longer known to be checked);
#   * a row of kind `image` must name, in `backed_by`, an image that the L3
#     lane (`just/ci.just`) actually runs through `--tier heap-free`, read
#     from the recipe, so it cannot go stale toward OK;
#   * every other kind says why the sentence is not a promise about a linked
#     image (a type's storage, a design rationale, the disclaimer itself).
#
# There is deliberately no `unbacked` kind. A sentence promising a heap-free
# image that no lane checks is fixed by scoping the SENTENCE, not by a row.
#
# `text` is a substring of the matching line, never a line number: the old
# roster carried numbers and two of its four had drifted by the time it was
# next read.
# ---------------------------------------------------------------------------
CLAIM_PHRASE = re.compile(
    r"no-alloc|without alloc\b|heap-free|\bno heap\b|"
    r"fully static (?:memory )?allocation|allocation-free|zero allocations",
    re.IGNORECASE,
)

# What `heap-free-poc-mps2` links, and therefore all an `image` row may claim
# it covers: `Executor::open_in` over caller-owned backing,
# `install_node_typed_in`, `spin_once` / `spin_some` / `spin_forever`, and the
# core `ParameterServer` (`new_in` / `declare` / `set` / `get`) over
# caller-owned `ParameterStorage`. NO RMW backend: every shipped transport
# allocates in C, so a transport image is `unified` tier at best.
HEAP_FREE_POC = "packages/testing/nros-tests/bins/heap-free-poc-mps2"

BOOK_CLAIMS = (
    {
        "file": "book/src/background/static-memory-and-pools.md",
        "text": "The core `ParameterServer` API works without alloc",
        "kind": "image",
        "backed_by": HEAP_FREE_POC,
        "covers": "ParameterServer::new_in/declare/set/get over ParameterStorage<4>",
    },
    {
        "file": "book/src/background/client-library.md",
        "text": "reach a `no_std` no-alloc target",
        "kind": "image",
        "backed_by": HEAP_FREE_POC,
        "covers": "Executor::spin_once, spin_some and spin_forever are all linked",
    },
    {
        "file": "book/src/tutorial/pick-the-rmw.md",
        "text": "not heap-free: each allocation goes through",
        "kind": "disclaimer",
        "why": "says XRCE is NOT heap-free (it replaced 'no heap required', "
        "which was false: every XRCE entity is a nros_xrce_calloc)",
    },
    {
        "file": "book/src/background/static-memory-and-pools.md",
        "text": "heap-free and not *proof* of it",
        "kind": "disclaimer",
        "why": "the sentence that says a feature list is not proof — the "
        "warning this gate exists to act on",
    },
    {
        "file": "book/src/background/static-memory-and-pools.md",
        "text": "own executor path is heap-free in that checked sense",
        "kind": "image",
        "backed_by": HEAP_FREE_POC,
        "covers": "the executor open + component install + spin path, with no transport (moved from the dispatch-strategy page by phase-537 W10)",
    },
    {
        "file": "book/src/background/static-memory-and-pools.md",
        "text": "A `Promise` is allocation-free: it borrows its client's",
        "kind": "type-level",
        "why": "where one type keeps its state (a borrowed slot), true of the "
        "type whatever the image links",
    },
    {
        "file": "book/src/background/differences-from-ros2.md",
        "text": "an array parameter, with no heap",
        "kind": "type-level",
        "why": "`Seq<T, N>` is inline storage; a property of the value type",
    },
    {
        "file": "book/src/guide/build/organize-a-larger-workspace.md",
        "text": "no-alloc trampoline",
        "kind": "type-level",
        "why": "a C++ template trampoline with `this` as ctx — no storage is "
        "created, by construction of the call",
    },
    {
        "file": "book/src/background/execution-model.md",
        "text": "stack frame, no heap, no boxing, so closures stay",
        "kind": "type-level",
        "why": "where an inline closure's captures live (the spin task's "
        "stack frame), not a statement about the linked image",
    },
    {
        "file": "book/src/guide/build/bring-your-own-cpp-library.md",
        "text": "`nros image check <image> --no-alloc` reads the built image",
        "kind": "disclaimer",
        "why": "says an image whose package calls nano_ros_hosted_cxx() cannot "
        "claim to be free of allocation; the match is the check's own flag",
    },
    # issue 2667: the CLI reference is `nros --help` written out, and its
    # `nros image check` section describes the check, not an image.
    {
        "file": "book/src/reference/cli.md",
        "text": "--no-alloc",
        "kind": "reference",
        "why": "the help of `nros image check --no-alloc`, the verb that "
        "checks an image; it promises nothing about one",
    },
    {
        "file": "book/src/reference/cli.md",
        "text": "heap-free",
        "kind": "reference",
        "why": "the `--tier heap-free` value of that verb's help",
    },
    {
        "file": "book/src/guide/realtime/analyze-timing.md",
        "text": "No heap in RT code",
        "kind": "rationale",
        "why": "a clippy disallowed-methods entry a user writes",
    },
    # phase-537 W12: the two disclaimer rows of the RMW API design page left
    # with it, when it moved to Internals (rmw/rmw-api-design). The gate reads
    # book/src only, and the page's wording there is unchanged.
)


def heap_free_images_in_lane(ci_text):
    """The image dirs the L3 recipe runs through `--tier heap-free`.

    Read from `just/ci.just` itself, so a row cannot claim a check the lane
    stopped running. Returns the dir prefix (everything before `/target`) of
    the quoted `--elf` path of each `nros image check --no-alloc --tier
    heap-free` invocation.
    """
    out = set()
    pat = re.compile(
        r"image check --no-alloc --tier heap-free --elf\s*\\?\s*\"([^\"]+)\"",
        re.DOTALL,
    )
    for m in pat.finditer(ci_text):
        path = re.split(r"/target[^/]*/", m.group(1), maxsplit=1)[0]
        out.add(path.rstrip("/"))
    return out


def harvest_book(book_files):
    """[(file, lineno, line)] for every line matching CLAIM_PHRASE."""
    hits = []
    for rel, text in sorted(book_files.items()):
        for n, line in enumerate(text.splitlines(), 1):
            if CLAIM_PHRASE.search(line):
                hits.append((rel, n, line))
    return hits


def check_claims(book_files, ci_text, roster=BOOK_CLAIMS):
    """(problems, {row index: [line numbers]}) for the book against the lane."""
    problems = []
    lane = heap_free_images_in_lane(ci_text)
    if not lane:
        problems.append(
            "just/ci.just runs no `nros image check --no-alloc --tier heap-free` "
            "on any image, so no `image` row can be backed"
        )
    seen = {}
    for rel, n, line in harvest_book(book_files):
        rows = [
            i for i, c in enumerate(roster) if c["file"] == rel and c["text"] in line
        ]
        if not rows:
            problems.append(
                f"{rel}:{n}: UNROSTERED no-alloc wording: {line.strip()!r} — "
                "scope the sentence to what is checked, or add a BOOK_CLAIMS "
                "row saying why it is not a promise about an image"
            )
        for i in rows:
            seen.setdefault(i, []).append(n)
    for i, c in enumerate(roster):
        if i not in seen:
            problems.append(
                f"{c['file']}: STALE roster row — no matching line contains {c['text']!r}"
            )
        if c["kind"] == "image":
            if c.get("backed_by") not in lane:
                problems.append(
                    f"{c['file']}: {c['text']!r} claims {c.get('backed_by')!r}, "
                    "which the L3 lane does not run through --tier heap-free "
                    f"(it runs: {sorted(lane) or 'nothing'})"
                )
        elif not c.get("why"):
            problems.append(
                f"{c['file']}: {c['text']!r} is kind {c['kind']!r} with no written reason"
            )
    return problems, seen


def load_book():
    """Every TRACKED book page (an index lookup, never a filesystem walk)."""
    book = {}
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--", "book/src/*.md", "book/src/**/*.md"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for rel in sorted(set(out.split())):
        with open(os.path.join(ROOT, rel), encoding="utf-8") as fh:
            book[rel] = fh.read()
    return book


def report_claims():
    """Print the roster; exit 0 only if every promise is backed or classified."""
    with open(os.path.join(ROOT, "just", "ci.just"), encoding="utf-8") as fh:
        ci_text = fh.read()
    book = load_book()
    if not book:
        print("check-no-alloc-image --claims: CANNOT CHECK — no book/src/**/*.md")
        return 2
    problems, seen = check_claims(book, ci_text)
    lane = heap_free_images_in_lane(ci_text)
    print("# book no-alloc wording, and what backs each line")
    print("")
    print(f"heap-free images the L3 lane checks: {sorted(lane) or 'NONE'}")
    print("")
    for i, c in enumerate(BOOK_CLAIMS):
        lines = ",".join(str(n) for n in seen.get(i, [])) or "?"
        print(f"  {c['file']}:{lines}  [{c['kind']}]")
        print(f'    "{c["text"]}"')
        if c["kind"] == "image":
            print(f"    backed by {c['backed_by']} — {c['covers']}")
        else:
            print(f"    not an image promise: {c['why']}")
    print("")
    if problems:
        print(f"{len(problems)} problem(s):")
        for p in problems:
            print(f"  FAIL {p}")
        return 1
    backed = sum(1 for c in BOOK_CLAIMS if c["kind"] == "image")
    print(
        f"OK — {backed} image promise(s), each backed by an image the L3 lane "
        f"links and checks; {len(BOOK_CLAIMS) - backed} other line(s) classified."
    )
    return 0


def selftest():
    """The claims gate has to be able to FAIL in each of its directions."""
    ci_ok = (
        '"$(nros_cli_bin --checkout-only)" image check --no-alloc --tier heap-free --elf \\\n'
        '    "pkg/img/target/thumbv7m-none-eabi/$(dir)/img"\n'
    )
    assert heap_free_images_in_lane(ci_ok) == {"pkg/img"}, heap_free_images_in_lane(
        ci_ok
    )
    row = {
        "file": "b.md",
        "text": "is heap-free",
        "kind": "image",
        "backed_by": "pkg/img",
        "covers": "x",
    }
    book = {"b.md": "The thing is heap-free.\n"}
    assert check_claims(book, ci_ok, (row,))[0] == [], "a backed row must pass"
    assert check_claims(book, "", (row,))[0], "a lane that checks nothing must fail"
    assert check_claims(book, ci_ok, ({**row, "backed_by": "pkg/other"},))[0], (
        "an image the lane does not run must fail"
    )
    unrostered = check_claims({"b.md": "It is fully no-alloc.\n"}, ci_ok, (row,))[0]
    assert any("UNROSTERED" in p for p in unrostered), "unrostered wording must fail"
    assert any("STALE" in p for p in unrostered), "a row matching nothing must fail"
    assert check_claims(book, ci_ok, ({**row, "kind": "rationale"},))[0], (
        "a non-image row with no reason must fail"
    )

    def quiet(argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            return main(argv, _run_selftest=False)

    assert quiet(["some.elf"]) == 2, "an image argument must be refused, not checked"
    print("selftest: ok: the claims gate fails in each direction")
    return 0


def main(argv=None, _run_selftest=True):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("elf", nargs="*", help=argparse.SUPPRESS)
    ap.add_argument(
        "--claims",
        action="store_true",
        help="gate the book's no-alloc wording against the images L3 checks",
    )
    ap.add_argument("--selftest", action="store_true", help="prove the check can fail")
    args, rest = ap.parse_known_args(argv)

    if args.selftest:
        return selftest()
    if args.elf or rest:
        print(
            "check-no-alloc-image: an image is checked by `nros image check "
            "--no-alloc` (issue 2667); this script holds only the book's wording "
            "(`--claims`)",
            file=sys.stderr,
        )
        return 2
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    if _run_selftest:
        with contextlib.redirect_stdout(io.StringIO()) as out:
            rc = selftest()
        if rc:
            sys.stdout.write(out.getvalue())
            return rc
    if args.claims:
        return report_claims()
    ap.error("give --claims or --selftest")
    return 2


if __name__ == "__main__":
    sys.exit(main())
