#!/usr/bin/env python3
"""Populations by FILE KIND, from the git index — phase-472 W5.

A gate's rule is about a kind of file ("every C/C++ header", "every CMake
file", "every Rust source that emits a message"). Its population was a
DIRECTORY LIST that matched where the rule's subject lived the day the gate was
written — `cmake/`, `packages/core/`, `*/src/**` — and the subject has since
spread: `zephyr/`, `packages/**/cmake`, `integrations/`, `examples/`, the root
`justfile`. The 2026-09-28 audit found 17 gates whose reach stopped short that
way (e.g. `check-cpp-no-std-stdio` read 73 of 285 C/C++ files and none of the
62 public `nros-cpp` headers).

So: name the KIND, get every tracked file of it, and state any narrowing at the
call site with its reason (`exclude_parts=`, `exclude_prefixes=`). Never a
directory list as the population.

Defaults: vendored `third-party/` trees and `generated/` code are excluded (a
rule about OUR code); submodule contents are never listed (the index holds a
gitlink, not the files). The `just` kind is the justfile GRAPH
(`check_just_sources.just_sources`), the `ci` kind is workflows + composite
actions (`workflow_commands.ci_files`) — one definition each.

API
    files_of_kind(*kinds, repo=None, exclude_parts=(...), exclude_prefixes=(),
                  include_generated=False) -> [repo-relative str]
    kind_of(path) -> set of kinds

CLI (for shell gates; NUL-separated with -z)
    python3 scripts/lib/file_kinds.py [-z] [--exclude-part P]... [--exclude-prefix P]... KIND...
    python3 scripts/lib/file_kinds.py                 self-test
"""

from __future__ import annotations

import functools
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

C_SUFFIXES = (".c", ".h")
CPP_SUFFIXES = (".cpp", ".cc", ".cxx", ".hpp", ".hh", ".hxx", ".h")

KINDS = {
    # kind: (suffixes, exact basenames)
    "c": (C_SUFFIXES, ()),
    "cpp": (CPP_SUFFIXES, ()),
    "c-family": (tuple(sorted(set(C_SUFFIXES) | set(CPP_SUFFIXES))), ()),
    "cmake": ((".cmake", ".cmake.in"), ("CMakeLists.txt",)),
    "rust": ((".rs",), ()),
    "shell": ((".sh", ".bash"), ()),
    "python": ((".py",), ()),
    # GNU make reads `GNUmakefile`, `makefile` and `Makefile`, and an IDE's
    # generated makefile includes its own lowercase fragments (S32DS reads
    # `makefile.defs` / `makefile.init` / `makefile.targets` from the project
    # root). Issue 1618: `integrations/s32ds/makefile.defs` carried a live
    # `--allow-multiple-definition` that the `Makefile`-only spelling never read.
    "make": ((".mk",), ("Makefile", "Make.defs", "Makefile.in", "GNUmakefile", "makefile",
                        "makefile.defs", "makefile.init", "makefile.targets")),
    "toml": ((".toml",), ()),
    "markdown": ((".md",), ()),
    "yaml": ((".yml", ".yaml"), ()),
    # A codegen template. Its text ships into USER sources (`entry.c.jinja`
    # becomes a C TU, the `boot_wrapper` packs a header), so a rule about what
    # those sources may say is a rule about these too (2026-10-01 re-run:
    # `check-ret-code-citations` read no template).
    "jinja": ((".jinja", ".j2"), ()),
    # A board descriptor, wherever the board lives — `packages/boards/*/` and
    # the nested `nros-board-zephyr/boards/<b>/` alike.
    "board-descriptor": ((), ("nros-board.toml",)),
}
# Kinds whose population is a GRAPH, owned by another helper.
DERIVED = ("just", "ci")
DEFAULT_EXCLUDE_PARTS = ("third-party",)


@functools.lru_cache(maxsize=4)
def _index(repo: str) -> tuple:
    # A cleaned env: an inherited GIT_DIR (a hook in a linked worktree) would
    # override `-C` and list ANOTHER repository's index (issue 0986).
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from git_hook_env import nros_clear_inherited_git_env
    out = subprocess.run(
        ["git", "-C", repo, "ls-files", "-z"], capture_output=True, text=True, check=True,
        env=nros_clear_inherited_git_env(dict(os.environ)),
    ).stdout
    return tuple(p for p in out.split("\0") if p)


def kind_of(path: str) -> set:
    name = os.path.basename(path)
    return {k for k, (sfx, names) in KINDS.items() if name in names or name.endswith(sfx)}


def files_of_kind(*kinds, repo=None, exclude_parts=DEFAULT_EXCLUDE_PARTS,
                  exclude_prefixes=(), include_generated=False) -> list:
    unknown = [k for k in kinds if k not in KINDS and k not in DERIVED]
    if unknown or not kinds:
        raise ValueError(f"file_kinds: unknown kind(s) {unknown or '(none given)'}")
    repo = str(repo or REPO)
    parts = set(exclude_parts) | (set() if include_generated else {"generated"})
    want = set()
    for k in kinds:
        if k == "just":
            sys.path.insert(0, str(Path(__file__).resolve().parent))
            from check_just_sources import just_sources
            want |= {os.path.relpath(p, repo) for p in just_sources(repo)}
        elif k == "ci":
            sys.path.insert(0, str(Path(__file__).resolve().parent))
            import workflow_commands
            want |= {os.path.relpath(p, repo) for p in workflow_commands.ci_files(repo=repo)}
    typed = [k for k in kinds if k in KINDS]
    if typed:
        sfx = tuple(s for k in typed for s in KINDS[k][0])
        names = {n for k in typed for n in KINDS[k][1]}
        want |= {p for p in _index(repo)
                 if os.path.basename(p) in names or p.endswith(sfx)}
    return sorted(
        p for p in want
        if not parts.intersection(p.split("/")[:-1])
        and not p.startswith(tuple(exclude_prefixes))
    )


_SH_SHEBANG = re.compile(rb"^#!\s*(?:/usr)?/bin/(?:env\s+)?(?:ba|da|z)?sh\b")


def shebang_shell(repo=None, exclude_parts=DEFAULT_EXCLUDE_PARTS) -> list:
    """Tracked EXTENSION-LESS files whose shebang names a POSIX shell.

    A shell script needs no `.sh`: `scripts/bin/cargo` (the `--locked` shim every
    cargo call goes through) is bash, and a suffix-keyed population never read it
    (issue 1614, `check-set-e-bare-assignment`). The shebang is the kind.
    """
    repo = str(repo or REPO)
    out = []
    for p in _index(repo):
        name = os.path.basename(p)
        if "." in name or set(exclude_parts).intersection(p.split("/")[:-1]):
            continue
        try:
            with open(os.path.join(repo, p), "rb") as fh:
                head = fh.read(64)
        except OSError:
            continue
        if _SH_SHEBANG.match(head):
            out.append(p)
    return sorted(out)


def self_test() -> None:
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        def w(rel, text="x\n"):
            p = Path(tmp) / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        for rel in ("cmake/a.cmake", "zephyr/CMakeLists.txt", "packages/x/cmake/b.cmake",
                    "packages/x/src/lib.rs", "third-party/v/CMakeLists.txt",
                    "packages/i/generated/g.rs", "packages/api/nros-cpp/include/nros/x.hpp",
                    "justfile", "just/m.just", "packages/cli/p/t.c.jinja",
                    "integrations/n/Make.defs", "integrations/s/makefile.defs"):
            w(rel)
        w("justfile", "mod m 'just/m.just'\n")
        # A git hook can reach this (via a gate); never let an inherited GIT_DIR
        # aim `git init` at the caller's repository (issues 0986/0988).
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from git_hook_env import nros_clear_inherited_git_env
        env = nros_clear_inherited_git_env(dict(os.environ))
        subprocess.run(["git", "-C", tmp, "init", "-q"], check=True, env=env)
        subprocess.run(["git", "-C", tmp, "add", "-A"], check=True, env=env)
        _index.cache_clear()
        got = files_of_kind("cmake", repo=tmp)
        # The subject spread beyond `cmake/`: every CMake file is in, vendored is out.
        assert got == ["cmake/a.cmake", "packages/x/cmake/b.cmake", "zephyr/CMakeLists.txt"], got
        assert files_of_kind("rust", repo=tmp) == ["packages/x/src/lib.rs"]
        assert "packages/i/generated/g.rs" in files_of_kind("rust", repo=tmp, include_generated=True)
        assert files_of_kind("cpp", repo=tmp) == ["packages/api/nros-cpp/include/nros/x.hpp"]
        assert files_of_kind("jinja", repo=tmp) == ["packages/cli/p/t.c.jinja"]
        w("scripts/bin/tool", "#!/usr/bin/env bash\nset -e\n")
        w("scripts/bin/py", "#!/usr/bin/env python3\n")
        subprocess.run(["git", "-C", tmp, "add", "-A"], check=True, env=env)
        _index.cache_clear()
        assert shebang_shell(repo=tmp) == ["scripts/bin/tool"], shebang_shell(repo=tmp)
        # Both make spellings: NuttX's `Make.defs` and an IDE's lowercase fragment.
        assert files_of_kind("make", repo=tmp) == [
            "integrations/n/Make.defs", "integrations/s/makefile.defs"], files_of_kind("make", repo=tmp)
        assert files_of_kind("just", repo=tmp) == ["just/m.just", "justfile"]
        assert files_of_kind("cmake", repo=tmp, exclude_prefixes=("zephyr/",)) == [
            "cmake/a.cmake", "packages/x/cmake/b.cmake"]
        try:
            files_of_kind("nope", repo=tmp)
        except ValueError:
            pass
        else:
            raise AssertionError("an unknown kind must raise, never return []")
    _index.cache_clear()


def main(argv) -> int:
    self_test()
    if not argv:
        print("file_kinds self-test: OK")
        return 0
    sep, kinds, parts, prefixes = "\n", [], list(DEFAULT_EXCLUDE_PARTS), []
    it = iter(argv)
    for a in it:
        if a == "-z":
            sep = "\0"
        elif a == "--exclude-part":
            parts.append(next(it))
        elif a == "--exclude-prefix":
            prefixes.append(next(it))
        else:
            kinds.append(a)
    try:
        files = files_of_kind(*kinds, exclude_parts=tuple(parts), exclude_prefixes=tuple(prefixes))
    except ValueError as e:
        print(e, file=sys.stderr)
        return 2
    if not files:
        print(f"file_kinds: no tracked file of kind {kinds}", file=sys.stderr)
        return 1
    sys.stdout.write(sep.join(files) + sep)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
