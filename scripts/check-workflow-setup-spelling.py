#!/usr/bin/env python3
"""Provision with `just setup <scope>`, not `just <scope> setup` -- in workflows
AND in the prose that teaches a reader what to type.

The two spellings look interchangeable and are not:

    just setup zephyr     -> dispatcher: runs `_setup-common`, THEN `just zephyr setup`
    just zephyr setup     -> the module recipe ALONE

`_setup-common` is where the host facts every tier asserts get provisioned --
cross Rust targets, pinned corrosion, the in-tree CLI, `nros-launch-resolve`,
clang-format. The module spelling skips all of it silently: the recipe exists,
it succeeds, and the lane fails later on a precondition nothing provisioned.

That is not hypothetical. `nightly.yml`'s platform job used the module spelling
and carried a hand-rolled "Install cross targets from config/rust-targets.txt"
step to compensate -- a workaround for a defect one word wide. host-tests hit
the same class from the other direction and cost three CI rounds to unpick.

`check-preconditions-provisioned` asserts that `just setup` PROVIDES those
facts. This asserts that workflows INVOKE the form which runs them. Neither
implies the other.

PROSE is the second arm (phase-422 W3). 64 occurrences across 38 files taught
the module spelling, so a reader who copied the book got a broken provisioning
-- the same defect as the workflow half, one surface over. The scanned surface
is INSTRUCTIONAL prose only, and the exclusions below are the whole reason this
arm is usable rather than noise: `docs/roadmap/`, `docs/issues/` and
`docs/research/` are the three RECORD series. They narrate what a command DID
at a time ("`just esp32 setup` tolerates that failure by design"), so a rewrite
falsifies the record, and new entries there will legitimately keep quoting the
module spelling. Gating them would produce a steady false-positive stream and
teach people to add exemptions reflexively.

Both arms share one exemption discipline: an entry carries a reason and is
checked in BOTH directions -- an exemption that matches nothing is deleted,
because a stale allow-list is how a gate quietly stops covering what it names.

Run:  python3 scripts/check-workflow-setup-spelling.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

# `just <scope> setup ...` occurrences that may stay, and why.
#
# EMPTY, and that is the point. The one exemption this gate shipped with --
# `just zephyr setup --skip-sdk` -- existed because the dispatcher took a scope
# and nothing else, so a call that had to pass a FLAG had no dispatcher spelling
# available. phase-422 W3 gave `setup` a variadic tail (and re-homes a
# leading-dash value that `just` binds to `tier`), which made
# `just setup zephyr --skip-sdk` work; phase-422 W4 converted the three nightly
# zephyr jobs, and the exemption went with them rather than outliving its cause.
#
# A new entry needs a reason that is a PROPERTY OF THE DISPATCHER, not a
# preference: "this lane provisions the host facts itself" is not one, because
# nothing checks that claim.
EXEMPT = {}

# --- prose arm ---------------------------------------------------------------

# Instructional prose: files a reader copies commands out of. `book/` is the
# user-facing manual, `AGENTS.md`/`CLAUDE.md` are the agent-facing one, the
# `docs/` subtrees named here are living reference (RFCs are explicitly living
# documents in this repo's convention), and a README is instructional wherever
# it sits.
DOC_FILES = ("AGENTS.md", "CLAUDE.md")
DOC_DIRS = (
    "book",
    "docs/design",
    "docs/development",
    "docs/guides",
    "docs/reference",
    "docs/release",
)
# Every README.md outside a pruned subtree is scanned too.
README_NAME = "README.md"

# Subtrees never scanned, and why. These are the RECORD series -- build output
# and vendored trees need no entry, because the candidate set is `git ls-files`
# (untracked paths and submodule contents never appear).
PRUNE_DIRS = {
    "docs/roadmap": "phase docs record work at a time, module spelling included",
    "docs/issues": "issue prose quotes what the module recipe did; the ledger is history",
    "docs/research": "dated snapshots -- rewriting one falsifies its measurement",
    "third-party": "vendored",
}

# (path, exact stripped line) -> reason. Keyed on the LINE TEXT, not the line
# number, so an unrelated edit above it does not silently move the exemption
# onto a different line. Checked in both directions.
DOC_EXEMPT = {
    (
        "docs/development/zephyr-version-support.md",
        "`NROS_ZEPHYR_VERSION=4.4 just zephyr setup` exited 0 and produced a",
    ): (
        "Past-tense incident report: this run happened, with this spelling, and "
        "produced a workspace that could not build Cortex-M. Converting it would "
        "assert a run that never occurred. The step's INSTRUCTIONS above it use "
        "the dispatcher."
    ),
}


def scope_tokens():
    """Platform scope names, from the same table the dispatcher consults."""
    path = os.path.join(ROOT, "scripts", "build", "scope.sh")
    try:
        with open(path, encoding="utf8") as fh:
            text = fh.read()
    except OSError:
        return set()
    names = set()
    for m in re.finditer(r'_NROS_SCOPE_PLATFORMS="([^"]*)"', text):
        names.update(re.findall(r"[a-z0-9_]+", m.group(1)))
    return names


def offenders(text, scopes):
    """[(lineno, line)] for executable `just <scope> setup` calls."""
    out = []
    for i, raw in enumerate(text.split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        # Two shapes, and missing the second made this gate useless on the very
        # workflow that motivated it: nightly's platform job spells the scope as
        # a matrix expression, `just ${{ matrix.plat }} setup`. A templated scope
        # in that position is ALWAYS the module form -- there is nothing to
        # resolve, and the spelling alone is the defect. Caught by mutation-
        # testing this gate against the pre-conversion nightly.yml.
        m = re.search(r"just (\$\{\{[^}]*\}\}) setup\b(.*)$", line)
        if m:
            scope, rest = m.group(1), m.group(2).strip()
        else:
            m = re.search(r"just ([a-z0-9_]+) setup\b(.*)$", line)
            if not m:
                continue
            scope, rest = m.group(1), m.group(2).strip()
            if scope not in scopes:
                continue
        call = ("just %s setup %s" % (scope, rest)).strip()
        out.append((i, line, call))
    return out


def _doc_pattern(scopes):
    # `(?![-\w])` is load-bearing and the workflow arm above does not need it:
    # prose is full of `just qemu setup-qemu` / `just qemu setup-network`, which
    # are DIFFERENT verbs (thin `nros setup --tool` callers) and must not be
    # flagged. A bare `\b` matches them, because `p` -> `-` is a word boundary.
    return re.compile(r"just (%s) setup(?![-\w])" % "|".join(sorted(scopes)))


def doc_offenders(text, scopes):
    """[(lineno, line)] for `just <scope> setup` in prose, wraps included.

    Prose does NOT skip `#` lines: in markdown those are headings, and inside a
    fenced block they are shell comments that teach just as loudly as the
    command below them.

    A doc line wraps where a workflow line does not, and the wrap hides the
    call from a single-line grep -- `docs/reference/zephyr-armv8r-setup.md`
    carried `just zephyr\\nsetup` for exactly that reason. So each line is also
    matched joined to its successor, and a match is attributed to the line its
    FIRST character falls in, which is what keeps the two passes from
    double-reporting.
    """
    pat = _doc_pattern(scopes)
    lines = text.split("\n")
    out = []
    for i, raw in enumerate(lines, 1):
        head = raw.strip()
        nxt = lines[i].strip() if i < len(lines) else ""
        joined = (head + " " + nxt) if nxt else head
        for m in pat.finditer(joined):
            if m.start() >= len(head):
                continue  # belongs to the next line; reported on its own pass
            out.append((i, head, m.group(0)))
    return out


def doc_paths():
    """Every instructional-prose file, with pruned subtrees skipped.

    The candidate set comes from `git ls-files`, not a filesystem walk: prose
    that is not committed cannot teach anyone, and an index lookup cannot wander
    into `build/`, `target/` or a provisioned `zephyr-workspace/` the way a walk
    can. `check-no-tracked-file-find` requires this, and it is right to.
    """
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z"],
        capture_output=True,
        check=True,
    ).stdout.decode("utf8")
    prune = tuple(PRUNE_DIRS)
    found = []
    for rel in out.split("\0"):
        if not rel:
            continue
        if any(rel == p or rel.startswith(p + "/") for p in prune):
            continue
        base = rel.rsplit("/", 1)[-1]
        if rel in DOC_FILES:
            found.append(rel)
            continue
        in_doc_dir = any(rel.startswith(d + "/") for d in DOC_DIRS)
        if base == README_NAME or (in_doc_dir and rel.endswith(".md")):
            found.append(rel)
    return sorted(set(found))


def self_test():
    scopes = {"zephyr", "freertos"}
    t = "\n".join(
        [
            "      # a comment about just zephyr setup is not a call",
            "          just zephyr setup --skip-sdk",
            "          just setup zephyr",
            "          just freertos setup",
            "          just workspace setup",
            "          just ${{ matrix.plat }} setup",
        ]
    )
    got = offenders(t, scopes)
    assert [g[0] for g in got] == [2, 4, 6], got
    assert got[0][2] == "just zephyr setup --skip-sdk", got[0]
    assert got[1][2] == "just freertos setup", got[1]

    # --- prose arm ---
    scopes = {"zephyr", "freertos", "qemu"}
    d = "\n".join(
        [
            "Run `just zephyr setup` to provision.",        # 1  offender
            "The dispatcher is `just setup zephyr`.",        # 2  fine
            "`just qemu setup-qemu` is a different verb.",   # 3  fine, NOT setup
            "`just qemu setup-network` needs sudo.",         # 4  fine
            "# just freertos setup in a heading still counts",  # 5  offender
            "before `just zephyr",                           # 6  offender (wrap)
            "setup` produces a workspace.",                  # 7  the wrap's tail
            "`just workspace setup` is not a platform scope.",  # 8  fine
        ]
    )
    got = doc_offenders(d, scopes)
    assert [g[0] for g in got] == [1, 5, 6], got
    assert got[2][2] == "just zephyr setup", got[2]

    # A wrap must not be double-counted from the tail line's own pass.
    assert len(doc_offenders("just zephyr\nsetup", scopes)) == 1

    # The pruned record series really are pruned, and the instructional ones
    # really are scanned -- a scan set that silently narrows is the failure
    # this whole gate exists to prevent.
    paths = doc_paths()
    assert "book/src/getting-started/zephyr.md" in paths, "book not scanned"
    assert "AGENTS.md" in paths, "AGENTS.md not scanned"
    assert "examples/README.md" in paths, "READMEs not scanned"
    assert not any(p.startswith("docs/roadmap/") for p in paths), "roadmap scanned"
    assert not any(p.startswith("docs/issues/") for p in paths), "issues scanned"
    assert not any(p.startswith("docs/research/") for p in paths), "research scanned"
    assert not any(p.startswith("third-party/") for p in paths), "vendored scanned"

    sys.stdout.write("check-workflow-setup-spelling self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    scopes = scope_tokens()
    if not scopes:
        sys.stderr.write(
            "error: parsed ZERO platform scopes from scripts/build/scope.sh.\n"
            "This gate would then match nothing and pass vacuously.\n"
        )
        return 1

    problems = []
    seen_exempt = set()
    for fn in sorted(os.listdir(WORKFLOWS)):
        if not fn.endswith((".yml", ".yaml")):
            continue
        path = os.path.join(WORKFLOWS, fn)
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        for lineno, line, call in offenders(text, scopes):
            if call in EXEMPT:
                seen_exempt.add(call)
                continue
            problems.append(
                "%s:%d uses the MODULE spelling\n"
                "      %s\n"
                "    `just <scope> setup` runs the module recipe alone and skips\n"
                "    `_setup-common`, so the cross Rust targets, pinned corrosion,\n"
                "    the CLI, the resolver and clang-format are never provisioned.\n"
                "    Use the dispatcher:  just setup <scope>" % (fn, lineno, line)
            )

    for call in EXEMPT:
        if call not in seen_exempt:
            problems.append(
                "STALE exemption %r matches no workflow line.\n"
                "    Delete it — an allow-list checked one way stops covering\n"
                "    what it claims to." % call
            )

    # --- prose arm ---
    paths = doc_paths()
    if not paths:
        sys.stderr.write(
            "error: the prose arm resolved ZERO files. It would then pass\n"
            "vacuously; check DOC_DIRS / PRUNE_DIRS against the tree.\n"
        )
        return 1

    doc_seen_exempt = set()
    for rel in paths:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
                text = fh.read()
        except (OSError, UnicodeDecodeError):
            continue
        for lineno, line, call in doc_offenders(text, scopes):
            key = (rel, line)
            if key in DOC_EXEMPT:
                doc_seen_exempt.add(key)
                continue
            problems.append(
                "%s:%d TEACHES the module spelling\n"
                "      %s\n"
                "    A reader who copies `%s` gets the module recipe and none of\n"
                "    `_setup-common` — no cross Rust targets, no pinned corrosion,\n"
                "    no CLI, no resolver, no clang-format.\n"
                "    Write:  just setup <scope>   (it takes flags: `just setup zephyr --force`)\n"
                "    If the line RECORDS what a command did, exempt it in\n"
                "    DOC_EXEMPT with the reason." % (rel, lineno, line, call)
            )

    for key in DOC_EXEMPT:
        if key not in doc_seen_exempt:
            problems.append(
                "STALE prose exemption %r matches no line in that file.\n"
                "    Delete it — an allow-list checked one way stops covering\n"
                "    what it claims to." % (key,)
            )

    if problems:
        sys.stderr.write("check-workflow-setup-spelling: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        return 1

    sys.stdout.write(
        "check-workflow-setup-spelling: OK — %d scope(s); workflows: %d exemption(s); "
        "prose: %d file(s), %d exemption(s); all live.\n"
        % (len(scopes), len(EXEMPT), len(paths), len(DOC_EXEMPT))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
