#!/usr/bin/env python3
"""phase-445 W6 (RFC-0098 D9) — a workspace has no root build file.

# The rule

A nano-ros workspace is a DIRECTORY OF PACKAGES, like a colcon workspace: `src/`
holds the packages, and everything generated lives in `build/`, `dist/` and
`log/`. There is no `<ws>/Cargo.toml` and no `<ws>/CMakeLists.txt`.

The cargo root is the GENERATED entry, `build/<coord>/<entry>/Cargo.toml`, which
reaches `src/*` as path dependencies; the cmake root is
`build/<coord>/CMakeLists.txt`. phase-445 W5 stopped generating the workspace
root and deleted the six template roots that were tracked; this gate is what
stops one coming back.

# Why a root is worse than merely redundant

It re-imposes the constraint RFC-0065 D3 carried and RFC-0098 D9 removes: a
package finds its workspace by walking UP, so every member must sit below the
root. With no root there is nothing to walk up to, and a path dependency need
not be below anything. Measured 2026-09-10 (RFC-0098 D9): two packages in
`src/`, a generated entry in `build/img/entry/`, no root manifest — builds.
Negative control, same tree plus a root `[workspace]` `Cargo.toml`:
`error: current package believes it's in a workspace when it's not`.

# The exemption is a SHAPE, never a name

Three directories under `examples/templates/` are not workspaces at all — they
are single-package examples that happen to live there
(`cpp-port-minimal-publisher`, `rclcpp-compat-smoke`, `topic-state-monitor-port`).
A single package is exactly the thing whose `CMakeLists.txt` belongs at its own
root, and the shape that says so is already in the tree: a `package.xml` beside
the build file. Naming the three instead would be a list that goes stale the
first time a fourth is added or one is renamed — the drift class this phase
exists to remove — and it would also let a real workspace be exempted by being
added to it.

Run: python3 scripts/check-workspace-root-build-files.py
"""

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

REPO = Path(__file__).resolve().parents[1]

# The two trees whose immediate children are workspaces.
WORKSPACE_PARENTS = ("examples/workspaces", "examples/templates")

# The build files a root must not have. `.colcon_workspace`, `README.md`,
# `.gitignore` and `system.toml` are the files a workspace root legitimately
# carries.
ROOT_BUILD_FILES = ("Cargo.toml", "CMakeLists.txt")

# The shape that says "this directory is a PACKAGE, not a workspace".
PACKAGE_MARKER = "package.xml"


def tracked_paths(repo: Path) -> "set[str]":
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    ).stdout
    return {p for p in out.split("\0") if p}


def offenders(repo: Path) -> "list[str]":
    """Tracked root build files under a workspace directory, sorted.

    A "root" is the IMMEDIATE child of one of `WORKSPACE_PARENTS`; a build file
    deeper than that belongs to a package and is not this gate's business.
    """
    paths = tracked_paths(repo)
    found = []
    for p in paths:
        parts = p.split("/")
        if len(parts) != 4:
            continue
        parent = "/".join(parts[:2])
        if parent not in WORKSPACE_PARENTS or parts[3] not in ROOT_BUILD_FILES:
            continue
        root = "/".join(parts[:3])
        if f"{root}/{PACKAGE_MARKER}" in paths:
            continue  # a single-package example, exempt BY SHAPE
        found.append(p)
    return sorted(found)


def self_test() -> None:
    """The NEGATIVE CONTROL, on the normal path (`check-gate-selftests`).

    This gate's subject is empty on a healthy tree, so "prints OK" is also what
    a wrong prefix, a wrong depth or an `ls-files` that returned nothing looks
    like. It must be shown to FIND a root, to SKIP a package-shaped one, and to
    ignore a build file one level deeper — that last is the case that would turn
    the gate into a ban on every workspace member.
    """
    import tempfile

    env = nros_clear_inherited_git_env(dict(os.environ))

    def git(repo, *args):
        subprocess.run(["git", "-C", str(repo), *args], check=True, env=env,
                       capture_output=True)

    def add(repo, rel, body="x\n"):
        p = repo / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
        git(repo, "add", "-f", rel)

    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        git(repo, "init", "-q")
        git(repo, "config", "user.email", "t@t")
        git(repo, "config", "user.name", "t")

        # A member's build file, one level deeper than a root. Not a finding.
        add(repo, "examples/workspaces/rust/src/talker_pkg/Cargo.toml")
        add(repo, "examples/workspaces/rust/.colcon_workspace")
        assert offenders(repo) == [], offenders(repo)

        # A root `Cargo.toml`. A finding.
        add(repo, "examples/workspaces/rust/Cargo.toml")
        assert offenders(repo) == ["examples/workspaces/rust/Cargo.toml"], offenders(repo)

        # A root `CMakeLists.txt` beside a `package.xml` is a single-package
        # example, exempt by SHAPE.
        add(repo, "examples/templates/cpp-port/CMakeLists.txt")
        add(repo, "examples/templates/cpp-port/package.xml")
        assert offenders(repo) == ["examples/workspaces/rust/Cargo.toml"], offenders(repo)

        # …and the same file WITHOUT the marker is a finding, so the exemption
        # is measured rather than assumed.
        add(repo, "examples/templates/ws/CMakeLists.txt")
        assert offenders(repo) == [
            "examples/templates/ws/CMakeLists.txt",
            "examples/workspaces/rust/Cargo.toml",
        ], offenders(repo)

        # A root build file OUTSIDE the two parents is not this gate's rule.
        add(repo, "packages/testing/thing/Cargo.toml")
        assert len(offenders(repo)) == 2, offenders(repo)


def main() -> int:
    self_test()
    found = offenders(REPO)
    if not found:
        print(
            "check-workspace-root-build-files: OK (no tracked root Cargo.toml / "
            "CMakeLists.txt under "
            + " or ".join(f"{p}/" for p in WORKSPACE_PARENTS)
            + ")"
        )
        return 0
    print(
        f"check-workspace-root-build-files: {len(found)} workspace root build file(s):\n",
        file=sys.stderr,
    )
    for p in found:
        print(f"  {p}", file=sys.stderr)
    print(
        "\nA workspace is a directory of packages and has NO root build file\n"
        "(RFC-0098 D9). The cargo root is the entry `nros build` generates,\n"
        "`build/<coord>/<entry>/Cargo.toml`; the cmake root is\n"
        "`build/<coord>/CMakeLists.txt`. A root `[workspace]` manifest also puts\n"
        "the walk-up constraint back: every member would have to sit below it.\n"
        "\n"
        "If this directory is a SINGLE PACKAGE rather than a workspace, give it a\n"
        f"`{PACKAGE_MARKER}` beside the build file — that is the shape this gate\n"
        "exempts, and the shape colcon already reads.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
