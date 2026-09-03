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

Inside a workflow `run:` block, an `apt-get install` / `apt install` line may not
name a package that appears in any `[prereq.*].apt` list.

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
* A `$(...)` substitution — that IS the fix, and it must not read as a violation.

Run: python3 scripts/check-workflow-indexed-apt.py [--self-test]
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")

INSTALL = re.compile(r"\bapt(?:-get)?\s+install\b([^\n]*)")
HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def indexed_apt_packages(path=INDEX):
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(path, "rb") as fh:
        index = toml.load(fh)
    out = {}
    for key, entry in (index.get("prereq") or {}).items():
        for pkg in entry.get("apt") or []:
            out.setdefault(pkg, key)
    return out


def command_lines(run):
    """Lines of a `run:` body that are commands — not comments, not heredocs."""
    out, terminator = [], None
    for line in (run or "").split("\n"):
        if terminator is not None:
            if line.strip() == terminator:
                terminator = None
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        out.append(line)
        m = HEREDOC.search(line)
        if m:
            terminator = m.group(2)
    return out


def named_packages(run):
    """(package, line) for every literal package an apt install line names.

    Continuations are joined first: the install list is routinely one package
    per line ending in `\\`, and reading line-by-line would see none of them.
    """
    joined, buf = [], ""
    for line in command_lines(run):
        buf += line.rstrip()
        if buf.endswith("\\"):
            buf = buf[:-1] + " "
            continue
        joined.append(buf)
        buf = ""
    if buf:
        joined.append(buf)

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
        # the remedy must not read as a violation
        ("sudo apt-get install -y $(python3 scripts/sdk/prereq-packages.py doxygen)", []),
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

    if failures:
        print(f"check-workflow-indexed-apt self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-workflow-indexed-apt self-test: OK ({len(cases)} cases + extraction)")
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
    bad = offenders(docs, indexed)
    if bad:
        print("check-workflow-indexed-apt: workflow(s) install a package the index declares:")
        for name, job, pkg, key, line in bad:
            print(f"  {name}  [{job}]  {pkg}  — declared by [prereq.{key}]")
            print(f"      {line[:100]}")
        print()
        print("  The index carries the apt/dnf/pacman/brew spellings and a presence")
        print("  probe. Restating one here is the same fact in two places, and this")
        print("  is the copy that goes stale. Use:")
        print("      $(python3 scripts/sdk/prereq-packages.py --manager apt <key>…)")
        print("  or `nros setup --system` in a job that already builds the CLI.")
        return 1

    print(
        f"check-workflow-indexed-apt: OK — {len(docs)} workflow(s), "
        f"{len(indexed)} indexed apt package(s), none restated."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
