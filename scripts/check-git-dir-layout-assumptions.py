#!/usr/bin/env python3
"""Nothing here may model git's directory layout — issue 1336 (dedupes 1306).

`packages/cli/nros-cli-core/build.rs` wrote the CLI source stamp's one VCS
input as `root.join(".git/index")`, guarded by `if index.exists()`. That is
correct in a main checkout and wrong in a LINKED WORKTREE, where `<root>/.git`
is a FILE holding `gitdir: <common>/.git/worktrees/<name>` and the index lives
under that directory. So the guard was false, the `rerun-if-changed` was
silently never emitted, and after a commit `build.rs` never re-ran: `just
setup-cli` reported `built:` on every attempt while `just check cli-fresh` kept
answering `STALE — built from <a>, sources are now <b>`, clearable only by
`touch build.rs`. Every agent session here works in a linked worktree, so that
was the DEFAULT condition, not an edge case.

TWO SHAPES OF THE SAME MISTAKE, and the gate checks both:

  R1  A path built by joining a literal subpath under `.git`
      (`.git/index`, `.git/HEAD`, `.git/modules/<n>`, …). Ask git instead:
      `git rev-parse --path-format=absolute --git-path <name>` answers in a
      main checkout, in a linked worktree and under `GIT_INDEX_FILE`, because
      it asks the process that owns the layout. `--path-format=absolute`
      matters — without it the answer is relative to git's cwd.

  R2  An existence test on `.git` that demands a DIRECTORY (`is_dir()`,
      `isdir`, `[ -d … ]`, `IS_DIRECTORY`). A worktree's `.git` is a file and
      so is a submodule's, and `is_dir()` answers "not a checkout" for both.
      `nros-pkg-index::detect_workspace_root` had this: its `.git` pass found
      nothing in either shape and the walk continued past the real root.
      `exists()` is the question actually being asked.

WHY THE RULE IS MEASURED AND NOT ASSERTED

A grep alone would be a style rule, and a style rule is what gets waived. So
the self-test BUILDS BOTH CHECKOUT SHAPES — a real `git init` plus a real `git
worktree add` — and measures that `<root>/.git/index` exists in one and not the
other, while `--git-path index` names an existing file in both. That is the
`check-hook-repo-side-effects` precedent: a claim about a checkout shape is
worth what it was measured in, which is exactly how this defect survived
review in a file whose own comments discuss `.git` being a file.

Being a gate that builds a throwaway repository, it clears the inherited git
environment first (issues 0986/0988) through the one sanctioned spelling.

Run:  python3 scripts/check-git-dir-layout-assumptions.py [--self-test]
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

# A gate that builds a repository must not be steerable by an inherited
# `GIT_DIR` & co. — they override both a path argument and `git -C`.
nros_clear_inherited_git_env()

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
)

# What gets read. `git ls-files` (never a filesystem walk — see
# `check-no-tracked-file-find.sh`), tracked sources only.
PATHSPECS = [
    "*.rs", "*.py", "*.sh", "*.just", "*.cmake",
    "justfile", "CMakeLists.txt", ".github/workflows/*.yml", ".githooks/*",
]
SKIP_PREFIXES = ("third-party/", "docs/", "book/")

# R1: a subpath UNDER `.git`. `.gitignore` / `.gitmodules` / `.gitattributes`
# cannot match — they have no slash — and neither can a bare `.git`, which is
# R2's business.
R1 = re.compile(r"\.git/[A-Za-z_]")

# PROSE, which R1 must not read as a path. Two shapes, both measured against
# real hits rather than imagined: text inside backticks (how this tree quotes a
# path in a diagnostic or a doc-comment, including across a Rust string
# continuation, where no closing quote appears on the line), and a quoted
# string that contains WHITESPACE — a sentence, or fixture data like
# `"gitdir: /elsewhere/.git/worktrees/w"`. A path literal has no spaces in it.
#
# Unquoted text is deliberately still scanned, because `just` and cmake
# interpolate paths bare (`{{justfile_directory()}}/.git/index`) and a gate
# blind to that would be narrower than the rule it enforces (issue 0196).
BACKTICKED = re.compile(r"`[^`]*`")
QUOTED = re.compile(r'"[^"]*"|\'[^\']*\'')

# R2: `.git` tested for directory-ness, in each language's spelling.
R2 = re.compile(
    r"""
      \.git["']?\s*\)?\s*\)?\.is_dir\(\)     # Rust:   dir.join(".git").is_dir()
    | isdir\([^)]*\.git                      # Python: os.path.isdir(f"{r}/.git")
    | -d\s+"[^"]*\.git"                      # shell:  [ -d "$SRC/.git" ]
    | IS_DIRECTORY[^)]*\.git                 # cmake:  if(IS_DIRECTORY …/.git)
    """,
    re.X,
)

# A line whose first non-blank characters begin a comment is prose. Every
# in-tree mention of this class today is such a line — including the ones in
# this gate's own neighbourhood that DESCRIBE the hazard — and a rule that
# flagged them would be unsatisfiable for anyone documenting it.
COMMENT_START = re.compile(r"^\s*(#|//|/\*|\*|--)")

# This file — it quotes every banned spelling, by necessity.
SELF = "scripts/check-git-dir-layout-assumptions.py"

# Sites that name a `.git` subpath or demand a directory and are RIGHT to.
# Keyed by (path, distinctive substring) so that a NEW offending line in the
# same file is still flagged, and each carries the reason — the
# `abi_hand_decls` `out_of_surface` shape.
ALLOWED: dict[tuple[str, str], str] = {
    # --- R1: the path literal is the DATA under test, not a path being used.
    ("scripts/check-source-manifest.sh", '"$sandbox/.git/index"'):
        "fabricates a depfile naming a .git path, to assert issue 0635's "
        "exclusion drops it; the path is the fixture, never resolved",
    ("packages/cli/nros-cli-core/src/cmd/check_workspace.rs", '.git/Cargo.toml'):
        "a test that CREATES a .git directory in a tempdir to assert the "
        "discovery walk skips it",
    ("packages/cli/nros-cli-core/src/cmd/check_workspace.rs", '.git/system.toml'):
        "same test, second manifest",
    # --- R2: none. The four `-d "<dir>/.git"` tests that existed (the runner
    # bootstrap, esp-idf, zenoh-c, the Zephyr Kconfig trees) each guarded a
    # clone the script itself makes, so each was arguably right — and three of
    # the four sat on an OPERATOR-SETTABLE directory, where `-d` sends a
    # perfectly good worktree or submodule into the clone branch. `-e` is never
    # worse for the question being asked, so the class was fixed rather than
    # declared. Keep it that way: an exception here should be hard to write.
}


def tracked_lines():
    """(path, lineno, line) for every non-comment line of every scanned file."""
    out = subprocess.run(
        ["git", "ls-files", "--", *PATHSPECS],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    for rel in out.splitlines():
        if rel.startswith(SKIP_PREFIXES) or rel == SELF:
            continue
        try:
            text = (ROOT / rel).read_text(encoding="utf8")
        except (OSError, UnicodeDecodeError):
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if COMMENT_START.match(line):
                continue
            yield rel, n, line


def without_prose(line: str) -> str:
    """`line` with every prose span blanked, leaving path-like text behind.

    Three passes, in this order because each depends on the previous one having
    already taken its spans out of the bookkeeping:

    1. Backticked spans — how this tree quotes a path inside a message.
    2. Balanced quoted strings CONTAINING whitespace — a sentence, or fixture
       data like `"gitdir: /elsewhere/.git/worktrees/w"`. A space-free quoted
       string is kept: that is what a path literal looks like.
    3. An UNTERMINATED quote, i.e. a string that continues on the next line
       (a `\\`-continued Rust message, a multi-line shell diagnostic). Anything
       after it is inside that string. Counted only after pass 2 has removed
       the balanced spans, so an apostrophe inside `"don't"` is not mistaken
       for one.
    """
    line = BACKTICKED.sub(" ", line)

    kept: list[str] = []

    def hold(m: re.Match[str]) -> str:
        s = m.group(0)
        kept.append(" " if re.search(r"\s", s[1:-1]) else s)
        return f"\x00{len(kept) - 1}\x00"

    line = QUOTED.sub(hold, line)
    for q in ('"', "'"):
        if line.count(q) % 2 == 1:
            line = line[: line.rindex(q)]
    return re.sub(r"\x00(\d+)\x00", lambda m: kept[int(m.group(1))], line)


def rule_of(line: str) -> str | None:
    """Which rule this line breaks, before exceptions. Pure."""
    if R1.search(without_prose(line)):
        return "R1"
    return "R2" if R2.search(line) else None


def classify(rel: str, line: str) -> str | None:
    """`"R1"`, `"R2"`, or None — exceptions applied. Pure."""
    rule = rule_of(line)
    if rule is None:
        return None
    for (path, needle) in ALLOWED:
        if path == rel and needle in line:
            return None
    return rule


def offenders():
    return [
        (rel, n, line.strip(), rule)
        for rel, n, line in tracked_lines()
        if (rule := classify(rel, line))
    ]


def measure_both_checkout_shapes() -> list[str]:
    """Prove the rule, not just the regex: build both shapes and look.

    Returns a list of failures. This is the half that makes the ban a fact —
    if `<root>/.git/index` existed in a linked worktree, R1 would be pedantry.
    """
    bad = []
    env_note = "clean git env (issues 0986/0988)"
    with tempfile.TemporaryDirectory() as tmp:
        main = Path(tmp) / "main"
        main.mkdir()

        def run(cwd, *args):
            r = subprocess.run(
                ["git", *args], cwd=cwd, capture_output=True, text=True,
                env={
                    **nros_clear_inherited_git_env(dict(os.environ)),
                    "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
                    "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t",
                },
            )
            if r.returncode != 0:
                bad.append(f"`git {' '.join(args)}` failed in {cwd} ({env_note}): "
                           f"{r.stderr.strip()}")
            return r.stdout.strip()

        run(main, "init", "-q", "-b", "main", ".")
        (main / "f.rs").write_text("fn a() {}\n")
        run(main, "add", "f.rs")
        run(main, "commit", "-qm", "init")
        if bad:
            return bad

        wt = Path(tmp) / "wt"
        run(main, "worktree", "add", "-q", "-b", "wt", str(wt), "HEAD")
        if bad:
            return bad

        def git_path_index(cwd):
            return run(cwd, "rev-parse", "--path-format=absolute",
                       "--git-path", "index")

        # Shape 1 — main checkout: `.git` is a directory and the literal works.
        # (Which is precisely why nobody noticed it was wrong.)
        if not (main / ".git").is_dir():
            bad.append("a main checkout's `.git` should be a directory")
        if not (main / ".git" / "index").is_file():
            bad.append("a main checkout should have `.git/index`")
        if Path(git_path_index(main)) != (main / ".git" / "index"):
            bad.append("`--git-path index` disagrees with the main checkout's index")

        # Shape 2 — linked worktree: the literal is a path that does not exist.
        if not (wt / ".git").is_file():
            bad.append("a linked worktree's `.git` should be a FILE")
        if (wt / ".git" / "index").exists():
            bad.append("`<worktree>/.git/index` exists — R1 would be pedantry, "
                       "not a rule; re-measure before relaxing the gate")
        wt_index = Path(git_path_index(wt))
        if not wt_index.is_file():
            bad.append(f"`--git-path index` named a missing file in a worktree: {wt_index}")
        if wt_index == (main / ".git" / "index"):
            bad.append("a linked worktree must have its OWN index")

        # And `is_dir()` — R2's shape — is false for a perfectly good checkout.
        if (wt / ".git").is_dir():
            bad.append("`.git` in a worktree read as a directory; R2 needs re-measuring")
    return bad


def self_test() -> int:
    """Negative control on BOTH halves, on the normal path (phase-395)."""
    bad = []
    must_flag = [
        ("packages/x/build.rs", 'let index = root.join(".git/index");', "R1"),
        ("scripts/x.sh", 'head="$root/.git/HEAD"', "R1"),
        ("scripts/x.py", 'p = root / ".git/modules" / name', "R1"),
        # Unquoted interpolation — the shape `just` and cmake actually write.
        ("just/x.just", 'cat {{justfile_directory()}}/.git/index', "R1"),
        ("packages/x/src/lib.rs", 'dir.join(".git").is_dir()', "R2"),
        ("scripts/x.py", 'if os.path.isdir(os.path.join(r, ".git")):', "R2"),
        ("scripts/x.sh", 'if [ -d "$SRC/.git" ]; then', "R2"),
        ("cmake/x.cmake", 'if(IS_DIRECTORY "${d}/.git")', "R2"),
    ]
    must_not_flag = [
        # The sanctioned resolutions.
        ("packages/x/build.rs", '&["rev-parse", "--path-format=absolute", "--git-path", "index"]'),
        ("scripts/x.sh", 'common="$(git rev-parse --path-format=absolute --git-common-dir)"'),
        # `.git` of EITHER shape — the fixed predicate.
        ("packages/x/src/lib.rs", 'dir.join(".git").exists()'),
        ("scripts/x.sh", 'if [ -e "$d/.git" ]; then'),
        # A directory-NAME skip in a tree walk: a `.git` FILE is skipped just
        # as well, so the shape is irrelevant there.
        ("scripts/x.py", 'SKIP = {"target", "build", ".git", "generated"}'),
        ("packages/x/src/lib.rs", 'matches!(name, ".git" | ".cargo")'),
        # Sibling dotfiles that merely start with the same four letters.
        ("scripts/x.sh", 'cat .gitignore .gitmodules .gitattributes'),
        # A declared exception, matched by (path, needle).
        ("scripts/check-component-entity-bounds.py", '    "/.git/",'),
        # PROSE, not a path — the four shapes real hits took. A rule that
        # flagged these would be unsatisfiable for anyone writing a diagnostic
        # about the hazard, which is most of what mentions it.
        ("scripts/x.sh", 'ok "this repo\'s own .git/config is byte-identical"'),
        ("packages/x/src/lib.rs", '    "the literal `<root>/.git/index` must not resolve — \\'),
        ("packages/x/tests/t.rs", 'fs::write(p, "gitdir: /elsewhere/.git/worktrees/w\\n")'),
        ("scripts/x.py", 'raise SystemExit("could not read .git/HEAD for this checkout")'),
        # A multi-line shell diagnostic: the quote never closes on this line.
        ("scripts/x.sh", 'bad "this repo\'s own .git/config CHANGED across a run — that is'),
    ]
    for rel, line, want in must_flag:
        got = classify(rel, line)
        if got != want:
            bad.append(f"MISSED ({want}, got {got}): {rel}: {line}")
    for rel, line in must_not_flag:
        got = classify(rel, line)
        if got is not None:
            bad.append(f"WRONGLY flagged ({got}): {rel}: {line}")
    # A comment line is prose, whatever it says.
    if not COMMENT_START.match('    // watches `.git/index` on purpose'):
        bad.append("a leading-comment line was not recognised as prose")
    # An allowed needle must be file-SCOPED: the same text elsewhere still fails.
    leaked = next(
        ((p, n) for (p, n) in ALLOWED
         if classify("packages/somewhere/else.rs", n) is None
         and rule_of(n) is not None),
        None,
    )
    if leaked:
        bad.append(f"an exception leaked out of the file it was declared for: {leaked}")
    # NO DEAD EXCEPTIONS (the issue-0743 class: a stale waiver is silently
    # inert, so the table reads as knowledge it no longer has). Every entry
    # must name a line that is really there and would really be flagged.
    for (path, needle), reason in ALLOWED.items():
        try:
            text = (ROOT / path).read_text(encoding="utf8")
        except OSError:
            bad.append(f"exception names a missing file: {path}")
            continue
        live = [
            ln for ln in text.splitlines()
            if needle in ln and not COMMENT_START.match(ln) and rule_of(ln)
        ]
        if not live:
            bad.append(
                f"DEAD exception ({path}, {needle!r}): no non-comment line there "
                f"would be flagged, so this waiver is inert — delete it. "
                f"Reason on file: {reason}"
            )

    bad += measure_both_checkout_shapes()

    if bad:
        print("check-git-dir-layout-assumptions SELF-TEST FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    print("check-git-dir-layout-assumptions self-test: OK "
          f"({len(must_flag)} flagged, {len(must_not_flag)} left alone, "
          "both checkout shapes measured)")
    return 0


def main() -> int:
    if self_test() != 0:
        return 2
    if "--self-test" in sys.argv:
        return 0

    bad = offenders()
    if bad:
        print("check-git-dir-layout-assumptions: FAILED — git's layout is "
              "MODELLED here, not asked:", file=sys.stderr)
        for rel, n, line, rule in bad:
            print(f"  [{rule}] {rel}:{n}: {line}", file=sys.stderr)
        print(
            "\n  R1 — a literal subpath under `.git`. In a linked worktree `.git`\n"
            "       is a FILE (`gitdir: …`), so the path does not exist and an\n"
            "       `exists()` guard fails OPEN: no watch, no diagnostic. Ask git:\n"
            "           git rev-parse --path-format=absolute --git-path <name>\n"
            "       (Rust: `source_stamp.rs::git_index_path` is the in-tree one.)\n"
            "\n  R2 — a `.git` existence test that demands a DIRECTORY. A linked\n"
            "       worktree's and a submodule's are files, so `is_dir()` answers\n"
            "       \"not a checkout\" for two ordinary shapes. Use `exists()` /\n"
            "       `-e`, which is the question being asked.\n"
            "\n  If a site is right as written — it only ever sees a clone the\n"
            "  script itself makes, or the literal is test DATA — add it to\n"
            f"  ALLOWED in {SELF} with the reason.\n"
            "\n  Issue 1336 (absorbed 1306).",
            file=sys.stderr,
        )
        return 1

    n = sum(1 for _ in tracked_lines())
    print("check-git-dir-layout-assumptions: OK "
          f"({n} tracked source line(s) scanned, {len(ALLOWED)} declared "
          "exception(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
