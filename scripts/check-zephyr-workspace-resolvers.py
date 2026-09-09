#!/usr/bin/env python3
"""Refuse a fourth spelling of the Zephyr workspace resolution chain.

phase-440 W1, RFC-0095 D4. `scripts/lib/zephyr-workspace.sh` is the ONE
resolver: `$NROS_ZEPHYR_WORKSPACE` -> the checkout-relative trees for the
selected line -> `$NROS_STORE/workspaces/zephyr/<version>`. Before it existed
the chain was spelled three times — a `just` expression, a shell `for` loop and
a third, shorter shell copy — and the three did not agree. Measured on the 4.4
line, `just zephyr` named the 4.4 sibling while `scripts/build/west-fixtures.sh`
resolved the 3.7 in-tree workspace and `scripts/check-tier-preconditions.sh`
reported no workspace at all. Nothing was broken loudly; the three simply
answered a shared question differently, which is the two-spellings shape
CLAUDE.md says this repo keeps paying for.

## What this gate keys on, and why that literal

The LEGACY SIBLING spelling — `nano-ros-workspace`, not preceded by `-`.

`zephyr-workspace` is the wrong signal: it is a legitimate path constant all
over the tree (build roots, rsync excludes, `zephyr-workspace-builds`), so a
gate keyed on it would be mostly noise. The sibling literal is different. It
appears only where somebody restated the FALLBACK RUNG — nobody writes
`../nano-ros-workspace` except to say "and if that is not there, look here",
which is precisely the ladder. The `(?<!-)` guard drops the unrelated
`--nano-ros-workspace` CLI flag of `nros metadata`, which names the nano-ros
repository rather than a Zephyr workspace.

Lines that are entirely a COMMENT are not a spelling. A gate about resolvers
must not fire on prose describing one — including the header you are reading.

## The direction of drift, both ways

Like `check-cli-source-dirs` (issue 0604), the point is to say which way the
tree moved, because both directions are wrong for different reasons:

* **a file gained the literal** and is not in the ratchet — a fourth spelling.
  Call the helper. If a consumer genuinely cannot (a Rust or Python caller, a
  patch script wanting only the legacy tree), record it with the reason.
* **a listed file lost the literal** — the ratchet moved the right way and its
  line is now a lie about the tree. Delete it, so the file keeps shrinking.

`.config/zephyr-workspace-resolvers.txt` is a RATCHET in the spirit of
`.config/ungated-gates.txt` and `.config/gate-lane-exempt.txt`: it starts at
the 31 files that spell the ladder today, it should only ever shrink, and every
line carries a reason so a reader can tell a deliberate consumer from a backlog
item. Folding the remaining ones is phase-440 W4/W5 work, not W1's.

Run: python3 scripts/check-zephyr-workspace-resolvers.py [--list]
"""

import argparse
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
from tracked import tracked  # noqa: E402  (issue 0721 — the index, never a walk)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RATCHET = os.path.join(".config", "zephyr-workspace-resolvers.txt")

# The legacy sibling rung. `(?<!-)` drops `--nano-ros-workspace`, an unrelated
# `nros metadata` flag naming the nano-ros repo.
LADDER = re.compile(r"(?<!-)nano-ros-workspace")

# A line that is only a comment is prose about the ladder, not a spelling of it.
# Covers `#` (sh, python, just), `//` and `///` (rust), and block-comment
# continuation lines.
COMMENT = re.compile(r"^\s*(#|//|/\*|\*/|\*\s|\*$)")

# Prose lives here. `west.yml` documents `west init -m … ~/nano-ros-workspace`,
# which is an upstream invocation, not our chain.
SKIP_SUFFIX = (".md", ".yml", ".yaml", ".txt")
SKIP_PREFIX = ("docs/", "book/", ".config/")


def tracked_files():
    """Every tracked path, repo-relative. The index, not a walk (issue 0721):
    `packages/` and `examples/` hold build output measured in hundreds of GB."""
    return [os.path.relpath(p, ROOT) for p in tracked(ROOT)]


def ladder_lines(text):
    """Line numbers in `text` that spell the ladder outside a comment.

    Split out from the file reading so the selftest can drive it directly —
    the classification IS the gate, and a negative control that had to lay out
    a directory tree to reach it would be the slow kind nobody keeps."""
    return [
        n
        for n, line in enumerate(text.splitlines(), 1)
        if LADDER.search(line) and not COMMENT.match(line)
    ]


def spells_ladder(path):
    """Line numbers on which `path` spells the ladder outside a comment."""
    if path.endswith(SKIP_SUFFIX) or path.startswith(SKIP_PREFIX):
        return []
    full = os.path.join(ROOT, path)
    try:
        with open(full, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except (IsADirectoryError, FileNotFoundError):
        # A submodule gitlink, or a path in the index but not the worktree.
        return []
    return ladder_lines(text)


def current_set():
    return {p: hits for p in tracked_files() if (hits := spells_ladder(p))}


def drift(found, listed):
    """(added, gone) — the two directions, as the report names them."""
    return sorted(set(found) - set(listed)), sorted(set(listed) - set(found))


def read_ratchet():
    """path -> reason. Format is `.config/gate-lane-exempt.txt`'s: one entry per
    line, sorted, a `# reason` REQUIRED on every one."""
    path = os.path.join(ROOT, RATCHET)
    if not os.path.exists(path):
        sys.exit(f"check-zephyr-workspace-resolvers: {RATCHET} is missing")
    entries, order, problems = {}, [], []
    with open(path, encoding="utf-8") as fh:
        for n, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            if "#" not in line:
                problems.append(f"  {RATCHET}:{n}: no `# reason` — one is required")
                continue
            name, reason = line.split("#", 1)
            name, reason = name.strip(), reason.strip()
            if not reason:
                problems.append(f"  {RATCHET}:{n}: empty reason")
            if name in entries:
                problems.append(f"  {RATCHET}:{n}: {name} listed twice")
            entries[name] = reason
            order.append(name)
    if order != sorted(order):
        problems.append(f"  {RATCHET}: entries are not sorted")
    return entries, problems


def selftest(verbose=False):
    """The negative control. Runs on the NORMAL path, every time — a selftest
    behind a flag alone is executed once, by its author, and is prose after
    that (phase-395, `check-gate-selftests`)."""
    ok = fail = 0

    def chk(what, cond):
        nonlocal ok, fail
        if cond:
            ok += 1
            if verbose:
                print(f"  ok   {what}")
        else:
            fail += 1
            print(f"  FAIL {what}", file=sys.stderr)

    # What the gate must SEE — one rung of the ladder, in each language that
    # spells it today.
    chk("shell candidate", ladder_lines('ws="$root/../nano-ros-workspace"\n') == [1])
    chk("shell 4.4 candidate", ladder_lines('ws=../nano-ros-workspace-4.4\n') == [1])
    chk("python candidate", ladder_lines('WS = ROOT.parent / "nano-ros-workspace"\n') == [1])
    chk("rust candidate", ladder_lines('let s = p.join("nano-ros-workspace");\n') == [1])
    chk("just candidate", ladder_lines('X := "../nano-ros-workspace"\n') == [1])
    chk("trailing comment does not hide it",
        ladder_lines('ws=../nano-ros-workspace  # the sibling\n') == [1])
    chk("every offending line is reported, not just the first",
        ladder_lines("a=../nano-ros-workspace\nb=1\nc=../nano-ros-workspace-4.4\n")
        == [1, 3])

    # What it must NOT see. Each of these was a real false positive before the
    # rule narrowed: the `nros metadata --nano-ros-workspace` flag names the
    # nano-ros REPOSITORY, and prose about the ladder is not a spelling of it.
    chk("the --nano-ros-workspace flag is not this ladder",
        ladder_lines('  "--nano-ros-workspace <path> or set NROS_WORKSPACE"\n') == [])
    chk("a shell comment is not a spelling",
        ladder_lines("# falls back to ../nano-ros-workspace\n") == [])
    chk("a rust // comment is not a spelling",
        ladder_lines("    // 3. Sibling ../nano-ros-workspace\n") == [])
    chk("a rustdoc /// comment is not a spelling",
        ladder_lines("/// `../nano-ros-workspace`\n") == [])
    chk("a block-comment body is not a spelling",
        ladder_lines(" * then ../nano-ros-workspace\n") == [])
    chk("an unrelated file is silent", ladder_lines("echo hello\n") == [])

    # And the classifier must be reachable through the file reader, or the two
    # halves can disagree while each looks right.
    chk("the helper itself is seen through spells_ladder",
        spells_ladder("scripts/lib/zephyr-workspace.sh") != [])
    chk("prose roots are skipped wholesale",
        spells_ladder("docs/design/0095-nros-store-is-the-root.md") == [])

    # Both directions of drift, which is the whole point of the ratchet.
    added, gone = drift({"a": [1], "b": [2]}, {"a": "r"})
    chk("a new spelling is reported as ADDED", added == ["b"])
    chk("a folded file is reported as GONE", drift({"a": [1]}, {"a": "r", "z": "r"})[1] == ["z"])
    chk("agreement is silent", drift({"a": [1]}, {"a": "r"}) == ([], []))

    if verbose:
        print(f"check-zephyr-workspace-resolvers --selftest: {ok} ok, {fail} failed")
    if fail:
        print(
            f"check-zephyr-workspace-resolvers: SELFTEST FAILED ({fail})",
            file=sys.stderr,
        )
    return 1 if fail else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--list",
        action="store_true",
        help="print every file that spells the ladder, with line numbers",
    )
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest(verbose=True)
    # On the NORMAL path, every time. A negative control nobody runs decays
    # into a comment.
    if selftest():
        return 1

    found = current_set()

    if args.list:
        for path in sorted(found):
            print(f"{path}  # lines {', '.join(str(n) for n in found[path])}")
        print(f"\n{len(found)} file(s)")
        return 0

    listed, problems = read_ratchet()

    added, gone = drift(found, listed)

    if added:
        problems.append("")
        problems.append(
            "  A FOURTH SPELLING of the Zephyr workspace chain (drift: the tree "
            "GAINED one):"
        )
        for path in added:
            lines = ", ".join(str(n) for n in found[path])
            problems.append(f"    {path}:{lines}")
        problems.append("")
        problems.append(
            "  Call the ONE resolver instead of restating the ladder:\n"
            "      source scripts/lib/zephyr-workspace.sh\n"
            "      ws=\"$(nros_zephyr_ws_resolve || true)\"          # canonical spelling\n"
            "      ws=\"$(nros_zephyr_ws_resolve_abs '' \"$repo_root\" || true)\"\n"
            "  or, from a non-shell caller, run it as a command:\n"
            "      scripts/lib/zephyr-workspace.sh [--version V] [--absolute] resolve\n"
            "  It already walks $NROS_ZEPHYR_WORKSPACE, the checkout-relative trees\n"
            "  for the selected line, and $NROS_STORE/workspaces/zephyr/<version>\n"
            "  (RFC-0095 D4). If this caller genuinely cannot use it, add the path\n"
            f"  to {RATCHET} with a reason saying WHY."
        )

    if gone:
        problems.append("")
        problems.append(
            "  The ratchet moved the RIGHT way (drift: the tree LOST a spelling) "
            "— these lines are now stale:"
        )
        for path in gone:
            problems.append(f"    {RATCHET}: delete `{path}`  ({listed[path]})")
        problems.append(
            "\n  This file only ever shrinks. A line kept after its file stopped\n"
            "  spelling the ladder is a licence nobody needs and a claim that is\n"
            "  no longer true of the tree."
        )

    if problems:
        print("check-zephyr-workspace-resolvers: FAIL", file=sys.stderr)
        for line in problems:
            print(line, file=sys.stderr)
        return 1

    print(
        f"check-zephyr-workspace-resolvers: OK — {len(found)} known spelling(s), "
        "no new one"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
