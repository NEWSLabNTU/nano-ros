#!/usr/bin/env python3
"""phase-383 W10.c — an example workspace's ROOT is generated, never tracked.

RFC-0065 D3 makes `nros build` emit the root `Cargo.toml` / `CMakeLists.txt`
from the discovered packages plus the `[image.*]` table. Committing one puts a
GENERATED member list under review and, worse, makes it authoritative: the
builder uses a tracked root as-is rather than writing its own, so a stale one
silently overrides the declarations it was derived from.

The class this gate exists for is "someone re-adds a hand-written root", and it
has already happened once in the migration that removed them: a `git add -A`
re-added three generated roots to the index (`features`, `realtime-rust`,
`sizing`). `.gitignore` does not help there — it does not apply to a file that
is already tracked — so the shape can only be kept out by asking git.

PACKAGE manifests are untouched: this looks at exactly ONE level below
`examples/workspaces/`, which is where a root lives. `src/<pkg>/Cargo.toml` is
a normal package and stays tracked.

Runs its own negative control on every invocation, per AGENTS.md — a gate that
has never been shown to fail is a comment.
"""

import os
import re
import subprocess
import sys

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    capture_output=True, text=True, check=True,
).stdout.strip()

# One level below `examples/workspaces/` or `examples/templates/` — the
# workspace ROOT, not its packages. phase-445 W5 (RFC-0098 D9, issue 1108
# acceptance 5) extended it to the templates: they are what `nros new` copies
# and what a user reads first, and they had kept hand-written roots after every
# example workspace lost its own. A template that is a single PACKAGE (a
# `package.xml` beside its root build file — `cpp-port-minimal-publisher`,
# `rclcpp-compat-smoke`, `topic-state-monitor-port`) is not a workspace and its
# build file is its own; those are exempted by name below, by checking the
# index for the `package.xml`.
ROOT_RE = re.compile(r"^examples/(workspaces|templates)/[^/]+/(Cargo\.toml|CMakeLists\.txt)$")

MARKER = ".colcon_workspace"


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    return out.splitlines()


def offenders(files):
    """Tracked root build files of a WORKSPACE. A directory that is itself one
    package (its `package.xml` beside the build file) is a package, not a
    workspace root."""
    present = set(files)
    return sorted(
        f for f in files
        if ROOT_RE.match(f) and f"{f.rsplit('/', 1)[0]}/package.xml" not in present
    )


def self_test():
    """Prove the matcher can fail, and that it does not overreach."""
    must_flag = [
        "examples/workspaces/rust/Cargo.toml",
        "examples/workspaces/bridge-xrce/CMakeLists.txt",
        # phase-445 W5 — the templates are workspaces too (issue 1108).
        "examples/templates/multi-node-workspace/Cargo.toml",
        "examples/templates/pure-c-workspace/CMakeLists.txt",
    ]
    must_not_flag = [
        # Package manifests, one level deeper — the common case, and the one a
        # sloppier pattern would sweep up with the roots.
        "examples/workspaces/rust/src/talker_pkg/Cargo.toml",
        "examples/workspaces/c/src/zephyr_entry/CMakeLists.txt",
        "examples/templates/local-msg-package/src/consumer/CMakeLists.txt",
        # The tracked marker that says this dir IS a workspace root.
        f"examples/workspaces/rust/{MARKER}",
        # Not an example at all.
        "packages/api/nros/Cargo.toml",
    ]
    # A single-PACKAGE template: its build file is its own, not a root.
    package_template = [
        "examples/templates/cpp-port-minimal-publisher/CMakeLists.txt",
        "examples/templates/cpp-port-minimal-publisher/package.xml",
    ]
    bad = []
    for f in must_flag:
        if not offenders([f]):
            bad.append(f"MISSED a tracked root: {f}")
    for f in must_not_flag:
        if offenders([f]):
            bad.append(f"WRONGLY flagged: {f}")
    if offenders(package_template):
        bad.append(f"WRONGLY flagged a single-package template: {package_template[0]}")
    # ...and the same file IS flagged once its package.xml is gone — the
    # exemption is the manifest, not the directory name.
    if not offenders(package_template[:1]):
        bad.append(f"MISSED a root once its package.xml is gone: {package_template[0]}")
    if bad:
        print("check-no-tracked-workspace-roots SELF-TEST FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    print("check-no-tracked-workspace-roots self-test: OK "
          f"({len(must_flag)} flagged, {len(must_not_flag) + 1} left alone)")
    return 0


def main():
    if self_test() != 0:
        return 2
    if "--self-test" in sys.argv:
        return 0

    files = tracked_files()
    bad = offenders(files)
    if bad:
        print("check-no-tracked-workspace-roots: FAILED — generated workspace "
              "root(s) are TRACKED:", file=sys.stderr)
        for f in bad:
            print(f"  {f}", file=sys.stderr)
        print(
            "\n  `nros build` GENERATES these from the discovered packages plus\n"
            "  the `[image.*]` table (RFC-0065 D3). A tracked one is used as-is\n"
            "  instead, so it silently overrides the declarations it came from.\n"
            "\n  Remove it from the index and let the builder write it:\n"
            "      git rm --cached <path>\n"
            f"  A `.gitignore` rule alone will NOT help — it does not apply to a\n"
            f"  file git already tracks. The tracked marker for a workspace root\n"
            f"  is `{MARKER}`.",
            file=sys.stderr,
        )
        return 1

    n = len({"/".join(f.split("/")[:3]) for f in files
             if f.startswith(("examples/workspaces/", "examples/templates/"))
             and len(f.split("/")) > 3})
    print(f"check-no-tracked-workspace-roots: OK ({n} example workspace(s) and "
          "template(s), no tracked root)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
