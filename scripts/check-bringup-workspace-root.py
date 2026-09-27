#!/usr/bin/env python3
"""issue 1515 / phase-470 W3 — a tree with a BRINGUP declares a workspace root.

# The rule

`nros_pkg_index::detect_workspace_root` walks up from a starting directory and
answers with the first of four rungs it meets:

1. `$NROS_WORKSPACE_ROOT` — an explicit override;
2. a `.colcon_workspace` file — the TRACKED marker (RFC-0065 D3 / phase-383
   W10.a). An RFC-0098 D9 workspace has no root `Cargo.toml`/`CMakeLists.txt` at
   all: `nros build` GENERATES the cargo root into `build/<coord>/`, which is
   gitignored, so the marker is the only thing in a fresh clone that says "this
   directory is a workspace";
3. a `Cargo.toml` declaring `[workspace]` — the original spelling, which a
   Rust-led tree carries anyway;
4. a `.git` entry — the LAST RESORT, and the reason this gate exists.

Only rungs 2 and 3 are TRACKED DECLARATIONS: facts a clone carries, that travel
with a copied-out tree, and that name the directory somebody meant. This gate
requires one of them for every bringup.

# Why rung 4 makes the absence invisible rather than loud

A tree that declares neither does not fail — it resolves to whatever ancestor
happens to hold a `.git`, which in this repository is the nano-ros checkout
itself. The pkg-index is then built over the WHOLE repo (the hazard
`nros-pkg-index`'s own `SKIP_DIRS` comment documents: an agent worktree makes
that a duplicate-package error), and a copy-out, which has no `.git` at all,
gets a `bail!` naming four markers instead.

Measured 2026-09-27 on `examples/templates/multi-node-workspace`, whose
generated entry carries its own `[workspace]` table so rung 3 stops AT the entry
rather than at the workspace. With the marker, a forced re-expansion of
`nros::main!` is clean; with the marker moved aside, the same command says::

    error: nros::main!: pkg `demo_bringup` not found in workspace
    `…/build/posix-zenoh/native_entry`. Known pkgs: []

That is what rung 2 buys, and nothing else in the tree supplies it: `nros build`
and `nros sync` take the workspace from the CWD and hand cargo a
`NROS_WORKSPACE_ROOT` besides, so they are green either way — the measurement
that says the marker matters has to be a BARE `cargo` invocation on the
generated entry, which is also what an IDE and rust-analyzer run.

# What a BRINGUP is — asked of the tree, never of the name

`nros_orchestration_ir::leaf_system::is_package_dir` is the discrimination:

    dir.join("Cargo.toml").is_file() || dir.join("CMakeLists.txt").is_file()

A `system.toml` BESIDE a package manifest is a single-package leaf and declares
its own deployment; a `system.toml` in a directory with neither is a workspace
bringup, whose images are chosen by the workspace builder. So the subject is
derived from the same predicate the resolver uses, and a `*_bringup` naming
convention is never consulted — `realtime-cpp-subnode-portable` calls its one
`deploy_bringup`, and `multi_pkg_workspace_zephyr`'s sits at the fixture root
with no `src/` above it.

# Why the walk STOPS BELOW the repository root

The repo's own `Cargo.toml` declares `[workspace]` (line 1). A walk that reached
it would rescue every bringup in the tree and this gate could never fail —
"prints OK" would mean nothing. It is also the wrong answer on its own terms:
that table is the nano-ros workspace, never a user's, and it is absent from a
copied-out tree. So the declaration must sit STRICTLY BELOW the repository root,
which is the same boundary as "does this fact travel with the directory".

`examples/`, `examples/workspaces/` and `examples/templates/` track no root
files at all, so nothing between a tree and the repo root can rescue it either.

# Reach = the rule, not the four sites issue 1515 named

The subject is every tracked `system.toml` in the repository, not a path list
and not `examples/` alone: 33 bringups on 2026-09-27, 21 under `examples/` and
12 under test fixtures, and the fixtures pass by the CARGO spelling at their own
fixture root. Scoping to `examples/` would have been narrower than the rule,
which is how a class comes back (issue 0196).

Run: python3 scripts/check-bringup-workspace-root.py
"""

import os
import subprocess
import sys
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

REPO = Path(__file__).resolve().parents[1]

# The tracked marker of rung 2.
COLCON_MARKER = ".colcon_workspace"
# The manifest of rung 3.
CARGO_MANIFEST = "Cargo.toml"
# The file that declares a deployment — `leaf_system::SYSTEM_TOML`.
SYSTEM_TOML = "system.toml"
# `leaf_system::is_package_dir`: either of these beside a `system.toml` makes the
# directory a PACKAGE, so its `system.toml` is a leaf deployment and not a
# bringup.
PACKAGE_MANIFESTS = ("Cargo.toml", "CMakeLists.txt")


def tracked_paths(repo: Path) -> "set[str]":
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    ).stdout
    return {p for p in out.split("\0") if p}


def declares_cargo_workspace(repo: Path, rel_manifest: str) -> bool:
    """The rung-3 sniff, character for character as the resolver writes it.

    `is_cargo_workspace_root` accepts `[workspace]`, `[workspace.` and
    `[workspace ]` after leading whitespace — so a `[workspace.dependencies]`
    with no bare table above it still declares a root, which is a real cargo
    shape. Matching the resolver matters more than matching intuition: a gate
    that is stricter than the code reports a defect that does not exist, and one
    that is looser passes a tree the resolver refuses.
    """
    try:
        text = (repo / rel_manifest).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    for line in text.splitlines():
        stripped = line.lstrip()
        if (
            stripped == "[workspace]"
            or stripped.startswith("[workspace.")
            or stripped.startswith("[workspace ]")
        ):
            return True
    return False


def declaring_ancestor(repo: Path, paths: "set[str]", bringup: str) -> "str | None":
    """The nearest ancestor of `bringup` that declares a root, or `None`.

    Ancestors are walked in resolver order and the REPOSITORY ROOT is excluded —
    see the module docstring: including it would make this gate unfailable and
    would accept an answer no copy-out can reproduce.
    """
    current = PurePosixPath(bringup).parent
    while str(current) != ".":
        rel = str(current)
        if f"{rel}/{COLCON_MARKER}" in paths:
            return rel
        manifest = f"{rel}/{CARGO_MANIFEST}"
        if manifest in paths and declares_cargo_workspace(repo, manifest):
            return rel
        current = current.parent
    return None


def bringups(paths: "set[str]") -> "list[str]":
    """Every tracked bringup DIRECTORY, sorted."""
    found = []
    for p in paths:
        path = PurePosixPath(p)
        if path.name != SYSTEM_TOML:
            continue
        d = str(path.parent)
        if d == ".":
            continue  # a repo-root `system.toml` is not a bringup in a tree
        if any(f"{d}/{m}" in paths for m in PACKAGE_MANIFESTS):
            continue  # a single-package leaf, not a bringup
        found.append(d)
    return sorted(found)


def offenders(repo: Path, paths: "set[str] | None" = None) -> "list[str]":
    if paths is None:
        paths = tracked_paths(repo)
    return [b for b in bringups(paths) if declaring_ancestor(repo, paths, b) is None]


def self_test() -> None:
    """The NEGATIVE CONTROL, on the normal path (`check-gate-selftests`).

    The subject is non-empty on a healthy tree, so unlike its sibling
    `check-workspace-root-build-files` this gate does at least count something —
    but "0 offenders" is still what a walk that runs one directory too far, a
    `is_package_dir` test with the wrong sense, or a rung-3 sniff that matches
    everything would print. Each of those is a case below.
    """
    import tempfile

    env = nros_clear_inherited_git_env(dict(os.environ))

    def git(repo, *args):
        subprocess.run(
            ["git", "-C", str(repo), *args], check=True, env=env, capture_output=True
        )

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

        # The repo root declares `[workspace]`, exactly as nano-ros does. Nothing
        # below may be rescued by it — if this line ever starts rescuing, every
        # other case here goes green for the wrong reason.
        add(repo, "Cargo.toml", "[workspace]\n")

        # A bringup with no declaration anywhere below the root. A finding.
        add(repo, "examples/workspaces/ws/src/demo_bringup/system.toml")
        add(repo, "examples/workspaces/ws/src/demo_bringup/package.xml")
        assert offenders(repo) == ["examples/workspaces/ws/src/demo_bringup"], offenders(
            repo
        )

        # The rung-2 marker at the tree root clears it.
        add(repo, "examples/workspaces/ws/.colcon_workspace", "# marker\n")
        assert offenders(repo) == [], offenders(repo)

        # The rung-3 spelling clears it too — a C/C++-led tree has no root
        # manifest, but a Rust-led one does, and both are legal roots.
        git(repo, "rm", "-q", "--cached", "examples/workspaces/ws/.colcon_workspace")
        (repo / "examples/workspaces/ws/.colcon_workspace").unlink()
        assert offenders(repo) == ["examples/workspaces/ws/src/demo_bringup"], offenders(
            repo
        )
        add(repo, "examples/workspaces/ws/Cargo.toml", "[workspace]\nmembers = []\n")
        assert offenders(repo) == [], offenders(repo)

        # A manifest with NO `[workspace]` table is not a declaration. Without
        # this case the rung-3 arm would be satisfied by any `Cargo.toml`.
        add(repo, "examples/workspaces/ws/Cargo.toml", "[package]\nname = \"x\"\n")
        assert offenders(repo) == ["examples/workspaces/ws/src/demo_bringup"], offenders(
            repo
        )

        # `[workspace.package]` with no bare table above it IS one, because the
        # resolver's own sniff accepts the prefix.
        add(repo, "examples/workspaces/ws/Cargo.toml", "[workspace.package]\nedition = \"2024\"\n")
        assert offenders(repo) == [], offenders(repo)

        # A `system.toml` BESIDE a package manifest is a single-package LEAF, so
        # it is not a bringup and needs no enclosing root. Both spellings of
        # `is_package_dir`.
        add(repo, "examples/native/rust/talker/system.toml")
        add(repo, "examples/native/rust/talker/Cargo.toml", "[package]\nname = \"t\"\n")
        add(repo, "examples/native/c/talker/system.toml")
        add(repo, "examples/native/c/talker/CMakeLists.txt")
        assert offenders(repo) == [], offenders(repo)

        # …and the same directory WITHOUT a package manifest IS a bringup, so the
        # exemption above is measured rather than assumed.
        add(repo, "examples/native/cpp/orphan/system.toml")
        assert offenders(repo) == ["examples/native/cpp/orphan"], offenders(repo)
        git(repo, "rm", "-q", "--cached", "examples/native/cpp/orphan/system.toml")

        # A bringup outside `examples/` is in scope — the rule is about bringups,
        # not about a directory name.
        add(repo, "packages/testing/fixtures/fx/src/demo_bringup/system.toml")
        assert offenders(repo) == ["packages/testing/fixtures/fx/src/demo_bringup"], (
            offenders(repo)
        )
        add(repo, "packages/testing/fixtures/fx/Cargo.toml", "[workspace]\n")
        assert offenders(repo) == [], offenders(repo)

        # A declaration on an INTERMEDIATE ancestor counts: that is the question
        # the resolver asks, and a nested workspace is a legal shape.
        add(repo, "packages/testing/fixtures/deep/a/b/bring/system.toml")
        add(repo, "packages/testing/fixtures/deep/a/.colcon_workspace")
        assert offenders(repo) == [], offenders(repo)


def main() -> int:
    self_test()
    paths = tracked_paths(REPO)
    subject = bringups(paths)
    found = offenders(REPO, paths)
    if not found:
        print(
            f"check-bringup-workspace-root: OK ({len(subject)} bringup(s); each "
            "has a tracked `.colcon_workspace` or `[workspace]` root below the "
            "repo root)"
        )
        return 0
    print(
        f"check-bringup-workspace-root: {len(found)} of {len(subject)} bringup(s) "
        "sit in a tree that declares NO workspace root:\n",
        file=sys.stderr,
    )
    for b in found:
        print(f"  {b}/{SYSTEM_TOML}", file=sys.stderr)
    print(
        "\nA directory holding a `system.toml` with no `Cargo.toml` /\n"
        "`CMakeLists.txt` beside it is a workspace BRINGUP, so the tree around it\n"
        "is a workspace and has to SAY so in a tracked file. Otherwise\n"
        "`detect_workspace_root` falls through to its `.git` rung and answers\n"
        "with the nano-ros checkout — or, in a copied-out tree with no `.git`,\n"
        "refuses outright.\n"
        "\n"
        f"Fix: add an empty `{COLCON_MARKER}` at the tree root (the spelling an\n"
        "RFC-0098 D9 workspace uses, since its cargo/cmake roots are generated\n"
        f"into `build/` and gitignored), or, for a Rust-led tree, a `{CARGO_MANIFEST}`\n"
        "declaring `[workspace]` there.\n"
        "\n"
        "If the directory is NOT a workspace bringup — a single-package example\n"
        "whose `system.toml` states its own deployment — it needs a package\n"
        "manifest beside that file, which is the shape `leaf_system::is_package_dir`\n"
        "reads and the shape this gate exempts.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
