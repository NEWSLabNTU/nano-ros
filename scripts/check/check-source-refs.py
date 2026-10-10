#!/usr/bin/env python3
"""check-source-refs — a submodule source states its pin, and git agrees.

RFC-0103 D5 / phase-484 W3. Every `[source.*]` row in `nros-sdk-index.toml`
that names a `submodule` also states where it comes from (`git`) and at which
commit (`ref`). The index is then the one place a pin can be read WITHOUT a
checkout: the store keys a tree by `<version>+<sha8>`, and an installed SDK
root (a `git archive`, no gitlinks) clones `git` at `ref`. Before this the
commit had two owners — the superproject gitlink in a checkout, and
`nros-submodule-pins.toml`, written by `stage-sdk-root.sh`, in an install.

Issue 0602 took `git`/`ref` OFF these rows because nothing compared them with
git, and one had drifted to upstream's commit and URL. This gate is what makes
putting them back safe: for every such row,

  * `ref` equals the gitlink STAGED for `submodule` (`git ls-files -s`), which
    is the commit the next `git commit` records, and
  * `git` equals the `.gitmodules` URL for that path.

A submodule bump is therefore a two-line change, and the remedy is mechanical:

    python3 scripts/check/check-source-refs.py --write

rewrites both keys from git (it never moves a gitlink).

`--rev <commit>` checks a COMMIT instead of the worktree — its index,
`.gitmodules` and gitlinks, all read from that commit. `stage-sdk-root.sh`
runs it with `HEAD`, because an installed SDK root reads its pins from the
index it ships (`sdk_store::recorded_pin`) and has no gitlink to fall back on.

Buildless, ~0.1 s. Self-tests the row rewriter on every run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
INDEX = ROOT / "nros-sdk-index.toml"

HEADER = re.compile(r"^\[source\.([A-Za-z0-9_-]+)\]\s*$")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=True
    ).stdout


def gitmodules_urls(rev: str | None) -> dict[str, str]:
    """`.gitmodules` path -> url: the worktree file (what will be committed), or REV's."""
    if rev:
        src = ["--blob", f"{rev}:.gitmodules"]
    else:
        src = ["-f", str(ROOT / ".gitmodules")]
    out = subprocess.run(
        ["git", "-C", str(ROOT), "config", *src, "--get-regexp", r"^submodule\..*\.(path|url)$"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    paths: dict[str, str] = {}
    urls: dict[str, str] = {}
    for line in out.splitlines():
        key, _, value = line.partition(" ")
        name, field = key[len("submodule.") :].rsplit(".", 1)
        (paths if field == "path" else urls)[name] = value
    return {p: urls.get(n, "") for n, p in paths.items()}


def gitlink(path: str, rev: str | None) -> str | None:
    """The commit recorded for PATH: STAGED (the next commit's) or in REV."""
    if rev:
        out = git("ls-tree", rev, "--", path).split()
        # `<mode> <type> <sha>\t<path>`
        return out[2] if len(out) >= 3 and out[0] == "160000" else None
    out = git("ls-files", "-s", "--", path).split()
    # `<mode> <sha> <stage>\t<path>`; a gitlink's mode is 160000.
    return out[1] if len(out) >= 2 and out[0] == "160000" else None


def index_text(rev: str | None) -> str:
    return git("show", f"{rev}:nros-sdk-index.toml") if rev else INDEX.read_text()


def expected(text: str, rev: str | None) -> tuple[dict[str, tuple[str, str]], list[str]]:
    """Row name -> (git, ref) as git states them, plus problems git itself has."""
    index = tomllib.loads(text)
    urls = gitmodules_urls(rev)
    want: dict[str, tuple[str, str]] = {}
    problems: list[str] = []
    for name, row in index.get("source", {}).items():
        sub = row.get("submodule")
        if not sub:
            continue
        url = urls.get(sub)
        sha = gitlink(sub, rev)
        if not url:
            problems.append(f"[source.{name}] submodule = \"{sub}\" has no `.gitmodules` entry")
            continue
        if not sha:
            problems.append(f"[source.{name}] submodule = \"{sub}\" is not a gitlink")
            continue
        want[name] = (url, sha)
    return want, problems


def rewrite(text: str, want: dict[str, tuple[str, str]]) -> str:
    """Set `git`/`ref` in each named `[source.*]` block, right after `submodule`.

    Existing `git =` / `ref =` lines in the block are dropped and re-emitted, so
    the rewrite is idempotent and the keys always sit in one place.
    """
    out: list[str] = []
    current: str | None = None
    for line in text.splitlines(keepends=True):
        m = HEADER.match(line)
        if m:
            current = m.group(1)
        elif line.startswith("["):
            current = None
        if current in want and re.match(r"^(git|ref)\s*=", line):
            continue
        out.append(line)
        if current in want and re.match(r"^submodule\s*=", line):
            url, sha = want[current]
            out.append(f'git = "{url}"\n')
            out.append(f'ref = "{sha}"\n')
    return "".join(out)


def self_test() -> None:
    src = (
        "[source.a]\nversion = \"1\"\ngit = \"old\"\nsubmodule = \"p\"\nref = \"0\"\n\n"
        "[source.b]\nsubmodule = \"q\"\n# keep\n\n[tool.x]\nref = \"untouched\"\n"
    )
    want = {"a": ("U", "S"), "b": ("V", "T")}
    once = rewrite(src, want)
    assert once == rewrite(once, want), "rewrite is not idempotent"
    assert 'git = "old"' not in once and 'ref = "0"' not in once, once
    assert 'submodule = "p"\ngit = "U"\nref = "S"\n' in once, once
    assert 'submodule = "q"\ngit = "V"\nref = "T"\n# keep' in once, once
    assert 'ref = "untouched"' in once, "a non-source table was rewritten"


def main() -> int:
    self_test()
    argv = sys.argv[1:]
    rev = argv[argv.index("--rev") + 1] if "--rev" in argv else None
    text = index_text(rev)
    want, problems = expected(text, rev)
    if "--write" in argv:
        if rev:
            print("check-source-refs: --write rewrites the worktree; it takes no --rev", file=sys.stderr)
            return 2
        new = rewrite(text, want)
        if new != text:
            INDEX.write_text(new)
            print(f"check-source-refs: rewrote git/ref for {len(want)} submodule source(s)")
        for p in problems:
            print(f"check-source-refs: {p}", file=sys.stderr)
        return 1 if problems else 0

    where = f"at {rev}" if rev else "staged"
    rows = tomllib.loads(text).get("source", {})
    for name, (url, sha) in sorted(want.items()):
        row = rows[name]
        if row.get("ref") != sha:
            problems.append(
                f"[source.{name}] ref = {row.get('ref')!r}, but the gitlink {where} "
                f"for {row['submodule']} is {sha}"
            )
        if row.get("git") != url:
            problems.append(
                f"[source.{name}] git = {row.get('git')!r}, but .gitmodules says {url}"
            )
    if problems:
        print("check-source-refs: FAIL — a submodule source's pin disagrees with git:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print(
            "  A submodule bump moves the gitlink AND the row (RFC-0103 D5). Fix:\n"
            "    python3 scripts/check/check-source-refs.py --write",
            file=sys.stderr,
        )
        return 1
    print(f"check-source-refs: OK — {len(want)} submodule source(s) state the pin git records ({where})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
