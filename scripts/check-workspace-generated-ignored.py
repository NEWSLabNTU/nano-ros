#!/usr/bin/env python3
"""issue 2470 — a workspace package's `generated/` must be ignored.

# What this protects

Message codegen writes a package's message crates into `generated/` beside
that package's `package.xml`. In a one-package (or any) workspace, the
packages live under `<ws>/src/<pkg>/`, so the output lands in
`<ws>/src/<pkg>/generated/`, one level below where a standalone leaf put it.
Those trees are USER-side build output (CLAUDE.md: never commit `generated/`).

# The defect it was written for

phase-531 W7 moved three `nros-bench` programs (`large-msg-baremetal`,
`loan-copy-count`, `wake-latency-cortex-m3`) into one-package workspaces. Each
leaf's `.gitignore` kept `/generated/`, which matched the OLD location and not
`src/<pkg>/generated/`. Nothing above them covered it either: `examples/` has
the repo-root `examples/**/generated/` rule and `nros-tests/bins/` has
`/*/src/*/generated/`, but `nros-bench/` had neither. So `just _codegen` left
untracked, unignored message crates there, and `check-message-crate-identity`
(on the fast line) refused them as unprefixed `builtin_interfaces`, `std_msgs`
and so on. Every local `just ci gate` after a codegen went red.

# The rule

For every tracked `.colcon_workspace` marker (a workspace root), and every
tracked `<ws>/src/<pkg>/package.xml` under it, `<ws>/src/<pkg>/generated/` is
ignored by git.

# Detection

STATIC, so it holds on a tree nothing has built: it asks
`git check-ignore --no-index` about a probe path inside each would-be
`generated/` directory. Git applies whichever `.gitignore` covers it (the
package's, the workspace's, a parent's or the root's), so the gate has no
pattern logic of its own. A bringup package gets no codegen today, but it is
checked too: it is a package, and a rule that names only some packages is the
narrower-than-the-rule reach of issue 0196.

Sweep (the same question by hand):
    for ws in $(git ls-files '*/.colcon_workspace' | xargs -n1 dirname); do
      for m in $(git ls-files "$ws/src/*/package.xml" | xargs -rn1 dirname); do
        git check-ignore -q --no-index "$m/generated/x/Cargo.toml" || echo "$m"
      done
    done

Run: python3 scripts/check-workspace-generated-ignored.py
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

PROBE = "generated/probe/Cargo.toml"


def _git(repo: Path, *args: str, stdin: str | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        input=stdin,
        capture_output=True,
        text=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    )


def workspace_packages(repo: Path) -> list[str]:
    """`<ws>/src/<pkg>` for every tracked package under a tracked workspace root."""
    markers = _git(repo, "ls-files", "-z", "--", "*.colcon_workspace").stdout.split("\0")
    roots = sorted({str(Path(m).parent) for m in markers if m.endswith("/.colcon_workspace")})
    if not roots:
        return []
    specs = [f"{r}/src/*/package.xml" for r in roots]
    xmls = _git(repo, "ls-files", "-z", "--", *specs).stdout.split("\0")
    pkgs = set()
    for x in xmls:
        if not x:
            continue
        p = Path(x).parent
        # Exactly `<ws>/src/<pkg>/package.xml`: the pathspec `*` crosses `/`.
        if str(p.parent.parent) in roots and p.parent.name == "src":
            pkgs.add(str(p))
    return sorted(pkgs)


def unignored(repo: Path) -> list[str]:
    pkgs = workspace_packages(repo)
    if not pkgs:
        return []
    probes = "\n".join(f"{p}/{PROBE}" for p in pkgs) + "\n"
    # Exit 1 means "none ignored", which is a finding, not an error.
    out = _git(repo, "check-ignore", "--no-index", "--stdin", stdin=probes)
    if out.returncode not in (0, 1):
        sys.exit(f"check-workspace-generated-ignored: git check-ignore failed: {out.stderr.strip()}")
    ignored = {line.strip() for line in out.stdout.splitlines()}
    return [p for p in pkgs if f"{p}/{PROBE}" not in ignored]


def selftest() -> None:
    """The negative control, on the normal path, against a repository it builds."""
    cases = (
        # The defect: the leaf ignores its OLD `generated/`, not `src/*/generated/`.
        ("the W7 bench shape", "/generated/\n", 1),
        ("a workspace-level rule", "/generated/\n/src/*/generated/\n", 0),
    )
    for name, ignore, want in cases:
        with tempfile.TemporaryDirectory() as td:
            repo = Path(td)
            if _git(repo, "init", "-q").returncode:
                sys.exit("check-workspace-generated-ignored SELFTEST: git init failed")
            ws = repo / "bench" / "probe-ws"
            (ws / "src" / "probe_pkg").mkdir(parents=True)
            (ws / ".colcon_workspace").write_text("# marker\n")
            (ws / ".gitignore").write_text(ignore)
            (ws / "src" / "probe_pkg" / "package.xml").write_text("<package/>\n")
            # A directory that is not a workspace is not this gate's subject.
            (repo / "plain" / "src" / "other").mkdir(parents=True)
            (repo / "plain" / "src" / "other" / "package.xml").write_text("<package/>\n")
            _git(repo, "add", "--", ".")
            got = unignored(repo)
            if len(got) != want:
                sys.exit(
                    f"check-workspace-generated-ignored SELFTEST FAILED: {name} -> "
                    f"{got}, expected {want} finding(s)"
                )


def main() -> int:
    selftest()
    repo = Path(__file__).resolve().parent.parent
    pkgs = workspace_packages(repo)
    if not pkgs:
        print("check-workspace-generated-ignored: no workspace package found; the "
              "discovery is broken", file=sys.stderr)
        return 1
    bad = unignored(repo)
    if bad:
        print("workspace packages whose `generated/` is not ignored by git:", file=sys.stderr)
        for p in bad:
            print(f"  {p}/generated/", file=sys.stderr)
        print(
            "\nCodegen writes message crates beside each package.xml, so a workspace's\n"
            "`src/<pkg>/generated/` must be ignored: add `/src/*/generated/` to the\n"
            "workspace's .gitignore (or a covering rule above it). See issue 2470.",
            file=sys.stderr,
        )
        return 1
    print(f"check-workspace-generated-ignored: OK ({len(pkgs)} workspace package(s), "
          "every `generated/` ignored)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
