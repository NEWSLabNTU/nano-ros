#!/usr/bin/env python3
"""A workflow must not install a package the index already declares — W3.

phase-413 W3, from the audit in issue 0996.

## The asymmetry this closes

`check-sysdep-remedies` already refuses a hand-written `sudo apt` in a `just`
recipe, and `[prereq.doxygen]`'s own `why` field reads "found undeclared by
check-sysdep-remedies" — so the index-side gap is guarded. The REVERSE was not:
nothing stopped a workflow apt-installing a package `[prereq.*]` already names,
and three did. `docs.yml` installed doxygen and graphviz, `nightly.yml` installed
clang and libclang-dev, and all four were in the index with their dnf/pacman/brew
spellings and a presence probe beside them.

That is not a style violation. It is the same fact in two places, and the copy in
YAML is the one nobody updates when the index moves — the drift RFC-0062 exists
to delete.

## What is checked

1. Inside a workflow `run:` block, an `apt-get install` / `apt install` line may
   not name a package that appears in any `[prereq.*].apt` list.

2. Every call site of `prereq-packages.py` — anywhere in the tree — must CAPTURE
   the script's exit status, in the one shape that works with or without
   `set -e`: `if ! <var>="$(python3 … prereq-packages.py …)"; then … fi`.

## Why rule 2 lives in the gate that prescribes the idiom — issue 1466

This gate's remedy text used to print

    sudo apt-get install -y $(python3 scripts/sdk/prereq-packages.py … )

and carried that spelling as a PASSING self-test case. A command substitution's
exit status does not propagate to the command it sits in, and `apt-get install`
with zero package arguments exits 0 — so when the script failed, the step
installed NOTHING and SUCCEEDED. Measured live: `live-peer regression` run
35817736997 printed a `ModuleNotFoundError` traceback three lines above
`0 upgraded, 0 newly installed`, went green, and ran the lane without clang.

So the defect was not four typos, it was one prescription: every site that
complied with this gate inherited it, and a new site would have too. A gate that
teaches a bug keeps re-creating it after every current site is fixed, which is
why the idiom and its enforcement move together and why rule 2 is here rather
than in a gate of its own.

`prereq-packages.py` can never legitimately print an empty list — it raises
rather than print nothing, including for the deliberate `noble = []` "not
packaged there" case — so capturing the status is sufficient and a `test -n`
would be redundant. The status is the whole signal; nothing was consuming it.

Rule 2's reach is every call site of the helper, not every workflow, because
that is the rule. `just/ci.just` reached the same swallow through a missing
`-e` rather than through argument position (`set -uo pipefail`, then an
uninspected `pkg=` feeding `apt-get install -y "$pkg"`, which exits 0 on an
empty argument), so a workflow-only reach would have left it — the 0196 shape.

The sibling class this does NOT claim to cover: a `$(…)` in argument position in
general. `check-set-e-bare-assignment` cannot see that construct either (it
matches only an assignment at statement position), and the tree holds ~740 such
substitutions, so a blanket rule would need an allowlist — the authored-list
drift this repo refuses. Scoped to the helper this gate prescribes, the rule
needs no list at all.

## What is deliberately allowed

* A package the index does NOT declare. This gate says "do not restate the
  index", not "never apt-get". `gnupg` and `lsb-release` in `gate.yml` exist to
  add a third-party apt source, and RFC-0062's providers are system / sdk /
  source / submodule — none of which can express "add this repository first", so
  indexing them would claim a capability the index does not have.
* `ros-humble-*`. Same reason: they come from packages.ros.org, which has to be
  added first, and the `ci-base` image is the right home for a ROS stack.
* Comment lines and heredoc bodies, for the reason `check-workflow-repo-env`
  documents: a gate that cannot tell a command from a sentence about a command is
  worse than no gate.
* A `$(...)` substitution — resolving from the index IS the fix for rule 1, and
  it must not read as a violation of it. Rule 2 then constrains the SHAPE of
  that substitution, which is a different question about the same line.

Run: python3 scripts/check-workflow-indexed-apt.py [--self-test]
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import index_packages  # noqa: E402 — phase-447 D2: the one manager-field reader

WORKFLOWS = os.path.join(ROOT, ".github", "workflows")
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")

INSTALL = re.compile(r"\bapt(?:-get)?\s+install\b([^\n]*)")
HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")

# Rule 2. The tool whose status every caller must capture, and the one shape
# that captures it whether or not the enclosing file runs under `set -e`.
#
# The marker is the tool INSIDE a command substitution, not the tool's name
# anywhere on the line. A mention is not a call site, and the first draft of
# this rule flagged all four of its own remedy `echo`s — a gate reporting its
# own advice as the defect. It is also exactly the right marker: a plain
# `python3 … prereq-packages.py > out` has nothing to swallow, because its
# status IS the command's.
PREREQ_TOOL = re.compile(r"\$\([^)]*prereq-packages\.py")
SAFE_CAPTURE = re.compile(r"^\s*if\s+!\s+[A-Za-z_][A-Za-z0-9_]*=\"\$\(")
# Where a caller can live. Not `*.py`: a Python caller checks a returncode, a
# different question with a different answer, and the only `.py` mentions in
# this tree are prose about the shell idiom.
CALLER_GLOBS = (
    ".github/workflows/*.yml",
    "just/*.just",
    "justfile",
    "scripts/*.sh",
    "scripts/**/*.sh",
    ".githooks/*",
)

# The ONE spelling both rules point at, printed rather than described so a
# reader can paste it. `if !` and not a bare assignment, because the assignment
# is only fatal where `set -e` is on and half this tree's recipes are not.
REMEDY = """      if ! pkgs="$(python3 scripts/sdk/prereq-packages.py --manager apt <key>…)"; then
        echo "::error::prereq-packages.py could not resolve <key> from nros-sdk-index.toml"
        exit 1
      fi
      # shellcheck disable=SC2086  # deliberate: $pkgs is a package LIST
      apt-get install -y $pkgs"""


def indexed_apt_packages(path=INDEX):
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(path, "rb") as fh:
        index = toml.load(fh)
    out = {}
    for key, entry in (index.get("prereq") or {}).items():
        # Every release's names: a workflow installing noble's `libssl3t64` is
        # installing an indexed package. `entry.get("apt")` on a per-release
        # table would iterate the RELEASE NAMES instead (phase-447 D2).
        for pkg in index_packages.all_names(entry.get("apt")):
            out.setdefault(pkg, key)
    return out


def command_lines_numbered(text):
    """(1-based line number, line) for lines that are commands.

    Not comments and not heredoc bodies, for the reason `check-workflow-repo-env`
    documents: a gate that cannot tell a command from a sentence about a command
    is worse than no gate.
    """
    out, terminator = [], None
    for n, line in enumerate((text or "").split("\n"), 1):
        if terminator is not None:
            if line.strip() == terminator:
                terminator = None
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        out.append((n, line))
        m = HEREDOC.search(line)
        if m:
            terminator = m.group(2)
    return out


def command_lines(run):
    """Lines of a `run:` body that are commands — not comments, not heredocs."""
    return [line for _, line in command_lines_numbered(run)]


def logical_lines(text):
    """(1-based line number, joined text) for every command line in `text`.

    Continuations are joined: an install list is routinely one package per line
    ending in `\\`, and reading line-by-line would see none of them. The number
    is the FIRST physical line of the logical one, which is where a reader has
    to go to fix it.
    """
    out, buf, start = [], "", None
    for n, line in command_lines_numbered(text):
        if buf == "":
            start = n
        buf += line.rstrip()
        if buf.endswith("\\"):
            buf = buf[:-1] + " "
            continue
        out.append((start, buf))
        buf = ""
    if buf:
        out.append((start, buf))
    return out


def named_packages(run):
    """(package, line) for every literal package an apt install line names."""
    joined = [text for _, text in logical_lines(run)]

    found = []
    for line in joined:
        m = INSTALL.search(line)
        if not m:
            continue
        rest = m.group(1)
        # A command substitution is the REMEDY. Do not read its contents as
        # literal package names.
        rest = re.sub(r"\$\([^)]*\)", " ", rest)
        for tok in rest.split():
            if tok.startswith("-") or tok.startswith("$"):
                continue
            if re.fullmatch(r"[a-z0-9][a-z0-9.+-]*", tok):
                found.append((tok, line.strip()))
    return found


def load_workflows():
    import yaml

    docs = []
    for name in sorted(os.listdir(WORKFLOWS)):
        if not name.endswith(".yml"):
            continue
        path = os.path.join(WORKFLOWS, name)
        with open(path) as fh:
            docs.append((name, yaml.safe_load(fh)))
    return docs


def caller_files():
    """Tracked files that could invoke the helper, read as raw text.

    Raw text rather than parsed YAML on purpose: a `run:` body is plain text
    inside the document, so one reader serves workflows, `just` recipes and
    shell scripts alike, and it can report a real file:line — which is what a
    reader needs in order to go and fix it.
    """
    import subprocess

    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", *CALLER_GLOBS],
        capture_output=True,
        text=True,
        check=True,
    )
    files = []
    for rel in sorted(set(out.stdout.split())):
        path = os.path.join(ROOT, rel)
        if not os.path.isfile(path):
            continue
        try:
            with open(path, encoding="utf-8") as fh:
                files.append((rel, fh.read()))
        except (OSError, UnicodeDecodeError):
            continue
    return files


def swallowing_callers(files):
    """(file, line, text) for every call site that discards the tool's status."""
    bad = []
    for rel, text in files:
        for lineno, line in logical_lines(text):
            if not PREREQ_TOOL.search(line):
                continue
            if SAFE_CAPTURE.match(line):
                continue
            bad.append((rel, lineno, line.strip()))
    return bad


def offenders(docs, indexed):
    bad = []
    for name, doc in docs:
        for job_name, job in (doc.get("jobs") or {}).items():
            for step in job.get("steps", []) or []:
                for pkg, line in named_packages(step.get("run") or ""):
                    if pkg in indexed:
                        bad.append((name, job_name, pkg, indexed[pkg], line))
    return bad


def self_test():
    indexed = {"doxygen": "doxygen", "graphviz": "graphviz", "curl": "curl"}
    cases = [
        ("sudo apt-get install -y doxygen graphviz", ["doxygen", "graphviz"]),
        ("apt-get install -y --no-install-recommends curl", ["curl"]),
        # The remedy must not read as a violation of rule 1. Note the SHAPE:
        # this case used to carry the swallowing spelling, which is how the
        # gate came to prescribe it (issue 1466).
        ('if ! pkgs="$(python3 scripts/sdk/prereq-packages.py doxygen)"; then\n'
         "  exit 1\nfi\nsudo apt-get install -y $pkgs", []),
        # an unindexed package is allowed
        ("sudo apt-get install -y gnupg lsb-release", []),
        ("echo doxygen", []),
    ]
    failures = 0
    for run, want in cases:
        got = [p for p, _ in named_packages(run) if p in indexed]
        if got != want:
            print(f"  self-test FAIL: {run!r} -> {got}, want {want}")
            failures += 1

    # continuations: one package per line is the common spelling
    multi = "sudo apt-get install -y \\\n  doxygen \\\n  graphviz"
    if [p for p, _ in named_packages(multi) if p in indexed] != ["doxygen", "graphviz"]:
        print("  self-test FAIL: line continuations not joined")
        failures += 1

    if command_lines("# apt-get install doxygen\napt-get install curl\n") != [
        "apt-get install curl"
    ]:
        print("  self-test FAIL: comment read as a command")
        failures += 1
    if command_lines("cat <<EOF\napt-get install doxygen\nEOF\n") != ["cat <<EOF"]:
        print("  self-test FAIL: heredoc body read as commands")
        failures += 1

    # --- rule 2, both directions (issue 1466) ---------------------------
    #
    # The swallowing spelling MUST be caught and the capturing one MUST NOT,
    # or the fix would be a second thing nobody checks. The first case is the
    # mutation control: revert the four workflow sites and it fires.
    tool = "python3 scripts/sdk/prereq-packages.py --manager apt clang"
    rule2 = [
        (f"apt-get install -y $({tool})", 1, "argument position, the measured defect"),
        (f"apt-get install -y \\\n  $({tool})", 1, "the same across a continuation"),
        (f'pkgs="$({tool})"', 1, "a bare assignment: no -e, or -e and nobody looks"),
        (f'if ! pkgs="$({tool})"; then\n  exit 1\nfi', 0, "the capturing shape"),
        (f"# apt-get install -y $({tool})", 0, "a comment is not a call site"),
        (f'echo "prereq-packages.py could not resolve"', 0, "a mention is not a call"),
        (f"python3 scripts/sdk/prereq-packages.py --manager apt clang > out", 0,
         "a plain command has nothing to swallow"),
        (f"cat <<EOF\napt-get install -y $({tool})\nEOF", 0, "a heredoc body is prose"),
    ]
    for text, want, why in rule2:
        got = len(swallowing_callers([("<self-test>", text)]))
        if got != want:
            print(f"  self-test FAIL: rule 2 ({why}) -> {got} finding(s), want {want}")
            failures += 1

    # A real line number, not just a verdict: `(file, 0, …)` would send the
    # reader to the top of a 900-line workflow.
    numbered = swallowing_callers([("<self-test>", f"true\ntrue\napt-get install $({tool})")])
    if [n for _, n, _ in numbered] != [3]:
        print(f"  self-test FAIL: rule 2 reported line {numbered}, want line 3")
        failures += 1

    if failures:
        print(f"check-workflow-indexed-apt self-test: {failures} case(s) FAILED")
        return 1
    print(
        f"check-workflow-indexed-apt self-test: OK ({len(cases)} rule-1 case(s), "
        f"{len(rule2)} rule-2 case(s) + extraction)"
    )
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if self_test() != 0:
        return 1

    indexed = indexed_apt_packages()
    docs = load_workflows()
    failed = False

    bad = offenders(docs, indexed)
    if bad:
        failed = True
        print("check-workflow-indexed-apt: workflow(s) install a package the index declares:")
        for name, job, pkg, key, line in bad:
            print(f"  {name}  [{job}]  {pkg}  — declared by [prereq.{key}]")
            print(f"      {line[:100]}")
        print()
        print("  The index carries the apt/dnf/pacman/brew spellings and a presence")
        print("  probe. Restating one here is the same fact in two places, and this")
        print("  is the copy that goes stale. Resolve the names from the index and")
        print("  CAPTURE the status — never a bare `$(…)` in argument position:")
        print(REMEDY)
        print("  or `nros setup --system` in a job that already builds the CLI.")

    files = caller_files()
    swallowed = swallowing_callers(files)
    if swallowed:
        failed = True
        print("check-workflow-indexed-apt: call site(s) discard prereq-packages.py's status:")
        for rel, lineno, line in swallowed:
            print(f"  {rel}:{lineno}")
            print(f"      {line[:100]}")
        print()
        print("  A command substitution's exit status does NOT reach the command it")
        print("  sits in, and `apt-get install` with zero packages exits 0 — so when")
        print("  the script fails the step installs nothing and goes GREEN. Measured:")
        print("  live-peer run 35817736997 ran a whole lane without clang (issue 1466).")
        print("  An assignment alone is not enough either: under `set -uo pipefail`")
        print("  with no `-e` it just leaves the variable empty. Write:")
        print(REMEDY)

    if failed:
        return 1

    print(
        f"check-workflow-indexed-apt: OK — {len(docs)} workflow(s), "
        f"{len(indexed)} indexed apt package(s), none restated; "
        f"{len(files)} caller file(s), every prereq-packages.py status captured."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
