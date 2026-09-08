"""The ONE way to drop a git-repository-local environment, in Python — issue 0986.

This is the Python twin of `scripts/lib/git-hook-env.sh`, and it exists for the
same reason and shares its NAME: `nros_clear_inherited_git_env`. The rule is
about repository side effects, not about `bash` — a Python gate that builds a
throwaway repository is the same hazard in a different language, and
`check-hook-repo-side-effects` credits the mitigation by grepping for that one
identifier in either language.

WHAT GOES WRONG (measured here, not reasoned about)

`GIT_DIR` and its family override BOTH a path argument and `git -C`, so under an
inherited one a fixture-building script does not build a fixture: every command
lands in the repository the environment names. Measured on 2026-09-08, running
each Python gate that calls `git init` under the two environment shapes
`check-hook-repo-side-effects` uses, against a victim repo compared byte for
byte:

    check-test-scripts-have-callers.py       DIRTY / DIRTY
    check-board-descriptor-single-source.py  DIRTY / DIRTY
    gen-issue-index.py --self-test           DIRTY / DIRTY
    check-box-sync-covers-tracked-source.py  clean / DIRTY

The fourth is the interesting one, and it is why this module clears the list git
gives rather than a list someone types. That gate ALREADY cleared the
environment — by hand, `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`,
`GIT_COMMON_DIR`, four of the sixteen names git 2.34 reports. It therefore
leaked `GIT_OBJECT_DIRECTORY`, and its fixture's `git add` wrote two loose
objects into the victim's object store. That is exactly the divergence issue
0986 records for shell (four hand-written copies of a subset, none of them
clearing `GIT_CONFIG`), reproduced one language over — so "the file pops some
GIT_ variables" cannot be what satisfies the rule. Asking git is.

USE

    import sys
    from pathlib import Path
    sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
    from git_hook_env import nros_clear_inherited_git_env

    nros_clear_inherited_git_env()               # clears os.environ, in place
    env = nros_clear_inherited_git_env(dict(os.environ))   # a cleaned copy

Call it BEFORE the first git invocation. Do NOT call it from a script that is
SUPPOSED to act on the repository the environment names; nothing in this tree
is.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

SHELL_TWIN = Path(__file__).resolve().with_name("git-hook-env.sh")

# The shell twin's fallback block is the ONE written-down list in the tree. It
# is parsed rather than retyped here: a second copy is the very thing 0986 is
# about, and this module would be the fifth.
_FALLBACK_BLOCK = re.compile(r"\bunset\s+\\\n((?:\s*GIT_[A-Z_]+\s*\\?\n)+)")


def _fallback_vars():
    """The shell twin's fallback list, or `[]` if it cannot be read."""
    try:
        text = SHELL_TWIN.read_text(encoding="utf8")
    except OSError:
        return []
    m = _FALLBACK_BLOCK.search(text)
    if not m:
        return []
    return re.findall(r"GIT_[A-Z_]+", m.group(1))


def local_env_vars():
    """Every repository-local git variable name, git's own answer first.

    `git rev-parse --local-env-vars` needs no repository to answer and grows
    when git grows. The fallback is for a git too old or too absent to answer,
    and it is LOUD when empty: a helper that silently clears nothing leaves the
    environment armed, which is worse than the hazard it was called about.
    """
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--local-env-vars"],
            capture_output=True,
            text=True,
        )
        names = out.stdout.split()
        if out.returncode == 0 and names:
            return names
    except OSError:
        pass
    names = _fallback_vars()
    if not names:
        raise RuntimeError(
            f"git could not answer `rev-parse --local-env-vars` and the fallback "
            f"list in {SHELL_TWIN} could not be parsed. Refusing to report a "
            f"cleared environment that is still armed (issue 0986)."
        )
    return names


def nros_clear_inherited_git_env(env=None):
    """Remove every repository-local git variable from `env`.

    `env=None` clears the live process environment, which is the shell twin's
    semantics and what a script wants when nothing it does is meant to touch
    the repository the environment names. Pass a mapping (typically
    `dict(os.environ)`) to get a cleaned copy for a `subprocess` call instead.

    Returns the mapping that was cleaned.
    """
    target = os.environ if env is None else env
    for name in local_env_vars():
        target.pop(name, None)
    return target


def _self_test():
    """The fallback list must agree with the git on this host.

    A parse that quietly returns a SHORTER list is the failure mode with no
    symptom — it clears something, so nothing looks broken, and the variable it
    misses is the one that does the damage.
    """
    parsed = set(_fallback_vars())
    if not parsed:
        print(
            f"git_hook_env self-test FAILED: no fallback list parsed out of "
            f"{SHELL_TWIN}",
            file=sys.stderr,
        )
        return 1
    live = set()
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--local-env-vars"], capture_output=True, text=True
        )
        if out.returncode == 0:
            live = set(out.stdout.split())
    except OSError:
        pass
    missing = live - parsed
    if missing:
        print(
            f"git_hook_env self-test FAILED: this git reports "
            f"{sorted(missing)} as repository-local and the fallback list in "
            f"{SHELL_TWIN.name} does not name them. The fallback is what runs "
            f"when git cannot answer, so it must not be a SUBSET (issue 0986).",
            file=sys.stderr,
        )
        return 1

    # The clearing itself, both forms, in both directions.
    probe = {"GIT_DIR": "/x", "GIT_OBJECT_DIRECTORY": "/y", "PATH": "/bin"}
    got = nros_clear_inherited_git_env(dict(probe))
    if "GIT_DIR" in got or "GIT_OBJECT_DIRECTORY" in got:
        print(
            f"git_hook_env self-test FAILED: the mapping form left {sorted(got)}",
            file=sys.stderr,
        )
        return 1
    if got.get("PATH") != "/bin":
        print("git_hook_env self-test FAILED: a non-git variable was dropped", file=sys.stderr)
        return 1
    os.environ["GIT_DIR"] = "/x"
    nros_clear_inherited_git_env()
    if "GIT_DIR" in os.environ:
        print("git_hook_env self-test FAILED: os.environ was not cleared", file=sys.stderr)
        return 1
    print(
        f"git_hook_env self-test: OK ({len(parsed)} name(s) in the shell twin's "
        f"fallback, {len(live)} reported by this git)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(_self_test())
