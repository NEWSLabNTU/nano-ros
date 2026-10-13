#!/usr/bin/env python3
"""check-fixture-build-dirs-ignored — a fixture's cmake build tree is ignored
wherever the harness puts it (issue 1866).

The fixture harness configures a cmake row into `<dir>/build-<rmw>/`
(`Rmw::build_dir()` in `nros-tests/src/fixtures/binaries/mod.rs`), and other
roads into `<dir>/build/`. Those used to be ignored only by per-leaf
`.gitignore` lines, so a leaf that had none (`bins/contract-monitor-cpp`,
fixed one leaf at a time by issue 1845) left its whole build tree untracked
after a fixture build — noise in `git status`, and a tree a blanket `git add`
would commit.

The rule is asked of the REAL subjects, both derived:

  * the names: every string `Rmw::build_dir()` returns, plus `build`;
  * the dirs:  every `dir = "…"` row in `examples/fixtures.toml`.

`git check-ignore --no-index` answers for a path inside each would-be build
dir, so it measures the ignore rules as git applies them (root and per-leaf),
never a re-implementation of them. The selftest runs on every call
(check-gate-selftests): a parse of a planted `build_dir()` must yield its
names, and a dir no rule covers must be reported.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
RMW_SRC = "packages/testing/nros-tests/src/fixtures/binaries/mod.rs"
MANIFEST = "examples/fixtures.toml"


def build_dir_names(src: str) -> list[str]:
    """The names `Rmw::build_dir()` returns, read from its source."""
    m = re.search(r"pub fn build_dir\(self\)[^{]*\{(.*?)\n    \}", src, re.S)
    if not m:
        return []
    return sorted(set(re.findall(r'=>\s*"([^"]+)"', m.group(1))))


def fixture_dirs(manifest: str) -> list[str]:
    return sorted(set(re.findall(r'^dir\s*=\s*"([^"]+)"', manifest, re.M)))


def unignored(repo: Path, dirs: list[str], names: list[str]) -> list[str]:
    probes = [f"{d}/{n}/probe" for d in dirs for n in names]
    out = subprocess.run(
        ["git", "-C", str(repo), "check-ignore", "--no-index", "--stdin"],
        input="\n".join(probes) + "\n", capture_output=True, text=True,
    ).stdout.split("\n")
    hit = {p for p in out if p}
    return [p.rsplit("/", 1)[0] for p in probes if p not in hit]


def self_test() -> None:
    names = build_dir_names(
        '    pub fn build_dir(self) -> &\'static str {\n'
        '        match self {\n'
        '            Rmw::A => "build-a",\n'
        '            Rmw::B => "build-b",\n'
        '        }\n'
        '    }\n')
    assert names == ["build-a", "build-b"], names
    assert fixture_dirs('[[fixture]]\ndir = "x/y"\n[[fixture]]\ndir = "x/y"\n') == ["x/y"]
    with tempfile.TemporaryDirectory() as t:
        repo = Path(t)
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        (repo / ".gitignore").write_text("covered/**/build-a/\n")
        miss = unignored(repo, ["covered/leaf", "bare/leaf"], ["build-a"])
        assert miss == ["bare/leaf/build-a"], miss


def main() -> int:
    # The selftest builds a repository; a GIT_DIR inherited from a hook would
    # point every git call at the caller's instead (issue 0986).
    nros_clear_inherited_git_env()
    self_test()
    names = build_dir_names((ROOT / RMW_SRC).read_text())
    if not names:
        print(f"check-fixture-build-dirs-ignored: could not read Rmw::build_dir() in {RMW_SRC}",
              file=sys.stderr)
        return 1
    names.append("build")
    dirs = fixture_dirs((ROOT / MANIFEST).read_text())
    miss = unignored(ROOT, dirs, names)
    if miss:
        print("check-fixture-build-dirs-ignored FAILED — a fixture build tree git would show "
              "as untracked (issue 1866):\n  " + "\n  ".join(miss)
              + "\n  Cover it in the root .gitignore (one rule per build-dir name, beside "
              "`examples/**/build/`), not with another per-leaf line.", file=sys.stderr)
        return 1
    print(f"check-fixture-build-dirs-ignored: {len(dirs)} fixture dirs x {len(names)} "
          f"build-dir names ignored")
    return 0


if __name__ == "__main__":
    sys.exit(main())
