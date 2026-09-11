#!/usr/bin/env python3
"""phase-445 W6 (RFC-0098 D1) — no example may TRACK a file under `.cargo/`.

# The rule

`Cargo.toml` and `.cargo/config.toml` carry Rust-toolchain facts only, and a
user never edits either to support a board. An example states which board it
deploys to once, in `system.toml`; everything that choice implies — the triple,
the link group, `build-std`, the pool budgets, the in-repo `[patch.crates-io]`
rows — is GENERATED into `build/<image>/nros-cargo.toml` and handed to cargo
with `--config`.

So nothing under `examples/**/.cargo/` is a source file, and the ones that were
tracked are deleted rather than gitignored: a committed generated file is a
mirror, and a mirror drifts.

# What was measured, and what it cost

On `main` at 2026-09-11, 78 files: 45 `config.toml` and 33 `nros-board.toml`
board projections (36 projections counting the three outside `examples/`, which
went with them). Between them they carried

  * 19 leaves' `[build] target` (phase-445 W2 moved it to the descriptors);
  * a hand-mirrored copy of each board's link group — phase-341's drift class,
    which issue 0440 had already caught losing a whole `-l<kernel>` group in a
    package collapse: valid TOML, happy cargo, every NuttX Rust entry failing at
    LINK time;
  * 17 leaves' `[env]` blocks, which are now the board descriptor's
    `[board.knobs]`, a transport implication, or `[image.<id>] env`;
  * an `include = [...]` line that `nros sync` REWROTE on every build, which
    CLAUDE.md answered with a rule ("never commit that line") that had already
    been broken twice.

# What this gate does NOT say

It is about TRACKED files, because that is the invariant a clone and CI see. A
leaf may still hold a `.cargo/config.toml` on disk: `nros sync` writes the
central `[patch.crates-io]` include for a leaf a plain `cargo` or the metadata
probe runs INSIDE (the Zephyr west lane is the one that has no `--config` seam
of its own — issue 1288). Those are gitignored by `.gitignore`'s blanket
`**/.cargo/config.toml`, so they never reach a commit, and this gate is what
makes that blanket safe: without it a `git add -f` would put one back and only
the machine that ran it would build.

Run: python3 scripts/check-example-cargo-dirs.py
"""

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

REPO = Path(__file__).resolve().parents[1]

# Every tracked path under an `examples/**/.cargo/` directory is a finding.
# There is no allowlist and there is deliberately no shape exemption: the whole
# point is that this directory has no source files left in it, so any entry is
# either a mirror of something generated or a hand-written board fact that
# belongs in `system.toml` or the board descriptor.
PATHSPEC = "examples/**/.cargo/*"


def offenders(repo: Path) -> "list[str]":
    """Tracked paths under an `examples/**/.cargo/` directory, sorted."""
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z", "--", PATHSPEC],
        capture_output=True,
        text=True,
        check=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    ).stdout
    return sorted(p for p in out.split("\0") if p)


def self_test() -> None:
    """The NEGATIVE CONTROL, on the normal path (`check-gate-selftests`).

    A gate whose subject is "git tracks nothing matching X" prints OK both when
    the rule holds and when the pathspec is wrong, the repo is elsewhere, or
    `ls-files` quietly returns nothing — three failures that look identical to
    success. So a throwaway repo with exactly one such file must come back as a
    finding before the real answer is believed.
    """
    import tempfile

    env = nros_clear_inherited_git_env(dict(os.environ))

    def git(repo, *args):
        subprocess.run(["git", "-C", str(repo), *args], check=True, env=env,
                       capture_output=True)

    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        git(repo, "init", "-q")
        git(repo, "config", "user.email", "t@t")
        git(repo, "config", "user.name", "t")
        leaf = repo / "examples" / "board" / "rust" / "talker"
        (leaf / ".cargo").mkdir(parents=True)
        (leaf / "Cargo.toml").write_text("[package]\nname = \"t\"\n")
        git(repo, "add", "examples/board/rust/talker/Cargo.toml")
        assert offenders(repo) == [], "a leaf with no .cargo/ file is not a finding"

        (leaf / ".cargo" / "config.toml").write_text("[build]\ntarget = \"x\"\n")
        git(repo, "add", "-f", "examples/board/rust/talker/.cargo/config.toml")
        found = offenders(repo)
        assert found == ["examples/board/rust/talker/.cargo/config.toml"], found

        # …and a file in the same directory that is NOT `config.toml` is caught
        # too: the projection this gate also retired was `nros-board.toml`.
        (leaf / ".cargo" / "nros-board.toml").write_text("[build]\n")
        git(repo, "add", "-f", "examples/board/rust/talker/.cargo/nros-board.toml")
        assert len(offenders(repo)) == 2, offenders(repo)

        # A `.cargo/` OUTSIDE `examples/` is somebody else's rule
        # (`check-cargo-config-tracked`), and must not be reported here.
        other = repo / "packages" / "testing" / "bin" / ".cargo"
        other.mkdir(parents=True)
        (other / "config.toml").write_text("[build]\ntarget = \"x\"\n")
        git(repo, "add", "-f", "packages/testing/bin/.cargo/config.toml")
        assert len(offenders(repo)) == 2, offenders(repo)


def main() -> int:
    self_test()
    found = offenders(REPO)
    if not found:
        print("check-example-cargo-dirs: OK (no tracked examples/**/.cargo/* file)")
        return 0
    print(
        f"check-example-cargo-dirs: {len(found)} tracked file(s) under an "
        f"examples `.cargo/` directory:\n",
        file=sys.stderr,
    )
    for p in found:
        print(f"  {p}", file=sys.stderr)
    print(
        "\nAn example's build configuration is GENERATED (RFC-0098 D1): `nros sync`\n"
        "or `nros build` writes `<leaf>/build/<image>/nros-cargo.toml` from the board\n"
        "the leaf's `system.toml` names, and cargo reads it through `--config`.\n"
        "\n"
        "  a triple / link group / build-std  -> the board descriptor's `cargo_config`\n"
        "  a hardware budget                  -> the descriptor's `[board.knobs]`\n"
        "  a knob only THIS image wants       -> `[image.<id>] env` in `system.toml`\n"
        "  a `[patch.crates-io]` row          -> generated; do not commit it\n"
        "\n"
        "Untrack it:  git rm --cached <path>",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
