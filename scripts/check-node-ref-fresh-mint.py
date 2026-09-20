#!/usr/bin/env python3
"""Issue 1386 — a node reference MINTED from the node may not be asked whether
it is live, because the answer is yes by construction.

`node_ref_of(node)` builds `{ node_id, generation: current_generation(node_id) }`
and `node_ref_is_live(r)` is `r.is_bound() && current_generation(r.node_id) ==
r.generation`. Composed in one expression, the second conjunct reads one atomic
counter twice and compares it with itself, and the first is `generation != 0`,
which the counters never are. So the predicate is CONSTANT TRUE for any in-range
slot — false only when `node_id >= MAX_NODES`, which no node an executor built
can be.

That is not a slightly-weak check. It reads as coverage:

  * three `NROS_RET_STALE_NODE` arms could never be returned, one of them in a
    verb whose `# Returns` block documented the verdict (PR #1064);
  * `rcl_node_is_valid` claimed to check "the generation it was bound at" on an
    `nros_node_t` that stores no such generation;
  * and in `nros_node_resolve_name` the inert guard stood in front of
    `get_executor(&mut executor._opaque)`, so a finalised executor produced
    `NROS_RET_OK` out of zeroed storage rather than a refusal.

The phase-379 W4 mechanism is sound in its ONE legitimate shape: an entity
stores the reference when it is created and compares it LATER, which is what
`publisher.rs`, `subscription.rs` and `executor.rs` do. The defect is only the
composition — mint and ask in the same breath.

WHY A GATE: four sites wrote it independently (the issue found three; the sweep
found the fourth, which had no return code and so produced no evidence). The
shape is one line, it looks exactly like a guard, and nothing about reading it
says "this is always true". A reviewer cannot catch it by eye, and no test can:
a tautology passes every assertion that expects `true`.

Deliberately TEXTUAL and deliberately narrow. It refuses `node_ref_is_live`
applied to a `node_ref_of` call — including across a line break, which is how
`rustfmt` renders the longer spellings — and says nothing about either function
used on its own.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `node_ref_is_live( … node_ref_of( … ) … )`, tolerating a `crate::node::`
# qualification on either, whitespace, and a newline between them.
#
# A lint, not a parser — but the gap may contain no `;`, `(` or `)`, which is
# what keeps it from spanning two statements: `node_ref_is_live(before));` then
# a later `node_ref_of(&node)` is the shape `node_ref_tests` legitimately
# writes, and a `.{0,200}?` gap matched it.
COMPOSED = re.compile(r"node_ref_is_live\s*\([^;()]{0,120}?node_ref_of\s*\(")

SEARCH_ROOTS = ("packages/",)

# The doc that EXPLAINS the defect must be able to quote it.
EXEMPT_SUFFIXES = (".md", ".json")


def tracked_rust_files():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *SEARCH_ROOTS],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.splitlines()
    return [
        rel
        for rel in out
        if rel.endswith(".rs") and not rel.endswith(EXEMPT_SUFFIXES)
    ]


def strip_comments(text):
    """Blank out `//` comments, keeping every newline so line numbers hold.

    A comment is where the retired shape SHOULD still appear: the four fixed
    sites each carry a note saying what stood there and why it was constant
    true, and a gate that forbids describing its own subject makes the record
    unwritable. (Crude by design — `//` inside a string literal would also be
    stripped, which cannot matter here: these two identifiers appear in no
    string in the tree, and the gate only ever reads for this one pattern.)
    """
    out = []
    for line in text.split("\n"):
        idx = line.find("//")
        out.append(line if idx < 0 else line[:idx])
    return "\n".join(out)


def hits_in(text):
    """Line numbers where the composition appears, in CODE.

    Reported at the line the OUTER call starts on, which is where the reader
    has to make the correction.
    """
    code = strip_comments(text)
    found = []
    for match in COMPOSED.finditer(code):
        found.append(code.count("\n", 0, match.start()) + 1)
    return found


SELF_TESTS = [
    # (source, expected number of hits, what the case is for)
    (
        "if !node_ref_is_live(node_ref_of(node)) { return NROS_RET_STALE_NODE; }",
        1,
        "the reported shape",
    ),
    (
        "return node_ref_is_live(node_ref_of(node));",
        1,
        "the `rcl_node_is_valid` shape",
    ),
    (
        "if !crate::node::node_ref_is_live(crate::node::node_ref_of(node)) {",
        1,
        "fully qualified, both halves",
    ),
    (
        "set_executor_node_identity(rust_exec, unsafe { crate::node::node_ref_of(node) });",
        0,
        "a mint alone is how an entity STORES its reference — never a hit",
    ),
    (
        "if publisher.node.is_bound() && !crate::node::node_ref_is_live(publisher.node) {",
        0,
        "the legitimate shape: a STORED reference, asked later",
    ),
    (
        "node_ref_is_live(\n    node_ref_of(node),\n)",
        1,
        "rustfmt splits the longer spellings across lines",
    ),
    (
        "assert!(node_ref_is_live(before));\nlet after = node_ref_of(&node);",
        0,
        "two separate statements are not the composition",
    ),
    (
        "// this arm used to be `node_ref_is_live(node_ref_of(node))`, which",
        0,
        "a comment recording the retired shape must stay writable",
    ),
    (
        "/// The arm that stood here was `node_ref_is_live(node_ref_of(node))`.",
        0,
        "including a doc comment",
    ),
]


def self_test():
    failures = 0
    for source, expected, why in SELF_TESTS:
        got = len(hits_in(source))
        if got != expected:
            failures += 1
            print(f"  FAIL ({why}): expected {expected} hit(s), got {got}")
            print(f"        {source!r}")
    if failures:
        print(f"\ncheck-node-ref-fresh-mint --self-test: {failures} case(s) FAILED")
        return 1
    print(
        f"check-node-ref-fresh-mint --self-test: {len(SELF_TESTS)} case(s) OK "
        "(3 positive, 6 negative)"
    )
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()

    # The classifier is checked on EVERY run, not only behind the flag: a
    # pattern that stopped matching would report this gate's own subject as
    # clean, which is the failure mode the gate exists to name one layer down.
    if self_test() != 0:
        return 1

    files = tracked_rust_files()
    offenders = []
    for rel in files:
        path = ROOT / rel
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "node_ref_is_live" not in text:
            continue
        for line_no in hits_in(text):
            line = text.splitlines()[line_no - 1].strip()
            offenders.append((rel, line_no, line))

    if offenders:
        print("[FAIL] a node reference is minted and immediately asked whether")
        print("       it is live — constant true (issue 1386):")
        print()
        for rel, line_no, line in offenders:
            print(f"         {rel}:{line_no}")
            print(f"           {line}")
        print()
        print("       `node_ref_of` reads the slot's CURRENT generation, and")
        print("       `node_ref_is_live` compares its argument against the")
        print("       slot's CURRENT generation. Composed, that is one counter")
        print("       compared with itself, so any arm behind it is dead code")
        print("       that reads as a guard.")
        print()
        print("       The generation answers ONE question: has this slot been")
        print("       retired since a reference was STORED? Compare a reference")
        print("       the entity kept (`publisher.node`, `subscription.node`).")
        print()
        print("       If what you need is 'is this node usable NOW', that is")
        print("       `rcl_node_is_valid` — its own state plus the context it")
        print("       names, via `executor_context_is_valid` for an")
        print("       executor-bound node.")
        return 1

    print(
        f"check-node-ref-fresh-mint: OK ({len(files)} tracked Rust file(s); "
        "no reference is minted and asked in one expression)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
