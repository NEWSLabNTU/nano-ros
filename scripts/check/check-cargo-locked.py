#!/usr/bin/env python3
"""check-cargo-locked — the `--locked` PATH shim stays retired, and the
recipes that build a closed-graph root pass `--locked` themselves.

Phase-485 W4 (RFC-0104 D9). Issues 0359/0378 injected `--locked` everywhere
through `scripts/bin/cargo` and an environment variable. Every enforcement leak
after that (0384, 0386, 0676, 0873, 1307, 1356, 1358) was a new way of
invoking cargo the shim did not see. The replacement is a SET comparison after
resolution, which no invocation path can bypass:

  * a leaf lock is seeded from the registry pin set and checked by `nros
    build` after it resolves (`orchestration::drift`);
  * each locked root's lock is held to the pin set by
    `check-cargo-registry-pins`, whoever rewrote it.

What this gate holds, so the old mechanism cannot creep back half-alive:

  1. no tracked `scripts/bin/cargo`;
  2. the retired variable is named nowhere outside `docs/` (a reader, a
     setter or an escape-hatch prefix would each be dead code that reads as
     live);
  3. the recipes that build a closed-graph root carry `--locked`: the CLI and
     the launch resolver (`just setup-cli`, `just setup-launch-resolve`), and
     every repo-root nextest run (`nros_cargo_nextest_args`).

Rule 3 is asked of EVERY cargo invocation in those recipes, not of named
lines (issue 1863). It used to match two authored lines, so the second build
in `setup-launch-resolve` — the parser's Python half, in play_launch's own
workspace — ran unlocked and rewrote that TRACKED lock on every setup
(dropping its `[[patch.unused]]` table). In a persistent CI workspace that
left the submodule dirty, which is what made issue 1841 refuse.

Buildless, ~0.1 s.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RETIRED = "NROS_" + "CARGO_FLAGS"  # spelled split so this file is not a hit

# Recipes whose every cargo invocation builds a closed-graph root.
CLOSED_ROOT_RECIPES = ("setup-cli", "setup-launch-resolve")
NEXTEST_SITE = ("scripts/build/cargo.sh",
                re.compile(r'nros_cargo_nextest_args\(\) \{[^}]*echo "--locked"', re.S))
CARGO_CALL = re.compile(r"(?:^|[\s;&|(])cargo\s+(?:build|run|test|check|metadata|install|rustc)\b")


def recipe_commands(justfile: str, name: str) -> list[str] | None:
    """The command lines of recipe `name` (comments dropped, `\\`
    continuations joined), or None if the recipe is absent."""
    lines = justfile.splitlines()
    head = re.compile(rf"^{re.escape(name)}(?:\s[^:]*)?:")
    try:
        start = next(i for i, l in enumerate(lines) if head.match(l))
    except StopIteration:
        return None
    body = []
    for l in lines[start + 1:]:
        if l and not l[0].isspace():
            break
        body.append(l)
    cmds, cur = [], ""
    for l in body:
        t = l.strip()
        if not t or t.startswith("#"):
            continue
        cur += " " + t.rstrip("\\")
        if not t.endswith("\\"):
            cmds.append(cur.strip())
            cur = ""
    if cur:
        cmds.append(cur.strip())
    return cmds


def violations(files: dict[str, str]) -> list[str]:
    """Every rule broken by `files` (path -> text). Pure, so the self-test can
    plant each failure without touching the tree."""
    bad = []
    if "scripts/bin/cargo" in files:
        bad.append("scripts/bin/cargo is tracked again — the --locked PATH shim is retired "
                   "(phase-485 W4); the drift check replaces it.")
    hits = [rel for rel, text in files.items()
            if not rel.startswith(("docs/", "third-party/")) and RETIRED in text]
    if hits:
        bad.append(f"{RETIRED} is retired (phase-485 W4) but named in:\n    "
                   + "\n    ".join(sorted(hits))
                   + "\n  Drop the reader/setter; a closed-root build passes `--locked` itself.")
    for name in CLOSED_ROOT_RECIPES:
        cmds = recipe_commands(files.get("justfile", ""), name)
        calls = [c for c in (cmds or []) if CARGO_CALL.search(c)]
        if not calls:
            bad.append(f"justfile: recipe `{name}` has no cargo invocation to check "
                       "(renamed or moved? update CLOSED_ROOT_RECIPES)")
        for c in calls:
            if "--locked" not in c:
                bad.append(f"justfile: `{name}` runs cargo without `--locked` — it may rewrite a "
                           f"tracked lock (issue 1863):\n      {c}")
    rel, pat = NEXTEST_SITE
    if not pat.search(files.get(rel, "")):
        bad.append(f"{rel}: repo-root nextest lost its `--locked` ({pat.pattern})")
    return bad


def self_test() -> None:
    good = {
        "justfile": 'setup-cli:\n'
                    '    cargo build --release --locked --manifest-path "$root/packages/cli/Cargo.toml"\n'
                    'setup-launch-resolve:\n'
                    '    # cargo build here in a comment is not a call\n'
                    '    cargo build --release --locked --manifest-path "$crate/Cargo.toml"\n'
                    '    cargo build --release --locked --manifest-path "$ws/Cargo.toml" \\\n'
                    '        -p parser --lib\n'
                    'other:\n'
                    '    cargo build --release\n',
        "scripts/build/cargo.sh": 'nros_cargo_nextest_args() {\n    echo "--locked"\n}\n',
        "docs/x.md": RETIRED,
    }
    assert violations(good) == [], violations(good)
    assert len(violations({**good, "scripts/bin/cargo": ""})) == 1, "a revived shim passed"
    assert len(violations({**good, "a.sh": f"{RETIRED}= cargo build"})) == 1, "a setter passed"
    dropped = {**good, "justfile": good["justfile"].replace("--release --locked", "--release", 1)}
    assert len(violations(dropped)) == 1, "a lost --locked passed"
    # issue 1863's shape: the SECOND build of a recipe, continued over lines.
    second = {**good, "justfile": good["justfile"].replace(
        '--locked --manifest-path "$ws', '--manifest-path "$ws')}
    assert len(violations(second)) == 1, "an unlocked second build passed"
    gone = {**good, "justfile": good["justfile"].replace("setup-cli:", "setup-kli:")}
    assert len(violations(gone)) == 1, "a renamed recipe passed vacuously"


def tracked() -> list[str]:
    return subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"], capture_output=True, text=True, check=True
    ).stdout.splitlines()


def main() -> int:
    self_test()
    files = {}
    for rel in tracked():
        try:
            files[rel] = (ROOT / rel).read_text(errors="ignore")
        except (IsADirectoryError, FileNotFoundError):
            continue
    bad = violations(files)
    if bad:
        print("check-cargo-locked FAILED:\n  " + "\n  ".join(bad), file=sys.stderr)
        return 1
    n = sum(len([c for c in recipe_commands(files["justfile"], r) or [] if CARGO_CALL.search(c)])
            for r in CLOSED_ROOT_RECIPES)
    print(f"check-cargo-locked: shim retired; {n} cargo calls in {len(CLOSED_ROOT_RECIPES)} "
          "closed-root recipes + repo-root nextest pass --locked")
    return 0


if __name__ == "__main__":
    sys.exit(main())
