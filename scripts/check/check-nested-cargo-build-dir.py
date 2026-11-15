#!/usr/bin/env python3
"""check-nested-cargo-build-dir: a cargo run from INSIDE a build goes through
`nros_sizes_build::nested_cargo()`, the constructor that drops every variable
naming a lock the outer build holds (phase-559 W10).

THE DEADLOCK. Cargo takes an exclusive lock on its `build.build-dir` for
`build`, `check` and `rustc`, and holds it for the whole build. That includes
the time it spends waiting on a build script. If a build script runs a nested
`cargo build` that inherited `CARGO_BUILD_BUILD_DIR`, the nested cargo blocks
on the lock ("Blocking waiting for file lock on build directory"). The outer
cargo is waiting on the build script, so neither one ever moves. Measured on
2026-11-15: the metadata probe of `examples/workspaces/c`, sharing one
build-dir, hung 3 h. nros-c's size probe was in `locks_lock_inode_wait`. No
error, no timeout.

WHERE IT CAN HAPPEN, measured on cargo 1.99 rather than assumed:
  * `build`/`check` from a build script block, even with their own
    `--target-dir`.
  * `metadata` and `locate-project` do not take the lock.
  * `cargo test` RELEASES the lock while its test binaries run: a nested
    `cargo build` finished in 1 s during a 20 s test. So a test helper that
    spawns cargo is not this hazard, and neither is the CLI, which no cargo
    build runs.
So the scope is code that runs INSIDE a build: every tracked `build.rs`, plus
the sources of every in-repo crate reachable as a build-dependency (a
build-dependency's own normal dependencies run in the build script too). The
scope is DERIVED from the tracked manifests, never listed, and a scope that
does not reach `nros-sizes-build` is a failure. A list would miss the next
build-dependency crate. `nros-sizes-build` is the one crate known to nest
cargo.

The rule covers EVERY cargo spawn in scope, `metadata` included, because the
helper costs nothing. A call site must never need to know which subcommands
lock.

Detector: `check-nested-cargo-lock-discipline`'s, reused (one spelling of
"this function runs cargo"), so the two gates cannot disagree on what a site
is. Not covered: a build script that runs a SHELL or PYTHON that then runs
cargo. Every tracked `build.rs` spawns only `git`, `cc` and `idlc` today
(swept 2026-11-15). The real-cargo end of this is `just check
shared-build-dir`'s `nested` scenario, which runs a nested build under a shared
build-dir with a timeout and keeps a negative control that deadlocks.
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
HELPER_CRATE = "packages/tooling/nros-sizes-build"
# What a disciplined site says: a call to the helper, or the variable removed
# by hand.
SANCTIONED = ("nested_cargo()", 'env_remove("CARGO_BUILD_BUILD_DIR")')
# The helper's own body removes its list, and the list must name the lock.
HELPER_BODY = "NESTED_CARGO_LOCK_ENV"
LOCK_CONST = re.compile(r'NESTED_CARGO_LOCK_ENV\s*:[^=]*=\s*\[[^\]]*"CARGO_BUILD_BUILD_DIR"')

_spec = importlib.util.spec_from_file_location(
    "nested_lock", Path(__file__).with_name("check-nested-cargo-lock-discipline.py")
)
assert _spec is not None and _spec.loader is not None
nested_lock = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nested_lock)


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", pattern],
        capture_output=True, text=True, check=True,
    ).stdout
    return [p for p in out.split("\0") if p and "/third-party/" not in f"/{p}"]


def path_deps(table: dict) -> list[str]:
    return [v["path"] for v in table.values() if isinstance(v, dict) and "path" in v]


def build_context_crates() -> set[Path]:
    """Every in-repo crate dir whose code runs inside a build script."""
    manifests: dict[Path, dict] = {}
    for rel in tracked("*Cargo.toml"):
        try:
            manifests[(REPO / rel).parent.resolve()] = tomllib.loads(
                (REPO / rel).read_text(encoding="utf-8")
            )
        except (OSError, tomllib.TOMLDecodeError):
            continue
    ws_paths: dict[str, str] = {}
    root = manifests.get(REPO.resolve(), {})
    for name, v in root.get("workspace", {}).get("dependencies", {}).items():
        if isinstance(v, dict) and "path" in v:
            ws_paths[name] = str((REPO / v["path"]).resolve())

    def deps_of(crate: Path, doc: dict, kinds: tuple[str, ...]) -> set[Path]:
        tabs = [doc.get(k, {}) for k in kinds]
        for t in doc.get("target", {}).values():
            tabs += [t.get(k, {}) for k in kinds]
        out = set()
        for t in tabs:
            for name, v in t.items():
                if isinstance(v, dict) and "path" in v:
                    out.add((crate / v["path"]).resolve())
                elif isinstance(v, dict) and v.get("workspace") and name in ws_paths:
                    out.add(Path(ws_paths[name]))
        return out

    seeds: set[Path] = set()
    for crate, doc in manifests.items():
        seeds |= deps_of(crate, doc, ("build-dependencies",))
    closure, todo = set(), list(seeds)
    while todo:
        c = todo.pop()
        if c in closure or c not in manifests:
            continue
        closure.add(c)
        todo += deps_of(c, manifests[c], ("dependencies",))
    return closure


def scope() -> list[Path]:
    files = {(REPO / p).resolve() for p in tracked("*build.rs")}
    crates = build_context_crates()
    for rel in tracked("*.rs"):
        p = (REPO / rel).resolve()
        if any(c in p.parents for c in crates) and "/tests/" not in str(p) and "/benches/" not in str(p):
            files.add(p)
    return sorted(files)


def offenders(paths: list[Path]) -> tuple[list[tuple[str, int, str]], int]:
    bad, sites = [], 0
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "Command::new(" not in text and "nested_cargo(" not in text:
            continue
        for lineno, sig, body in nested_lock.functions(text):
            if not nested_lock.names_cargo(body):
                continue
            sites += 1
            if any(s in body for s in SANCTIONED):
                continue
            if nested_lock.HELPER_DEF.search(sig) and HELPER_BODY in body and LOCK_CONST.search(text):
                continue
            try:
                shown = str(path.relative_to(REPO))
            except ValueError:
                shown = str(path)
            bad.append((shown, lineno, sig.strip().split("\n")[0]))
    return bad, sites


SELF_TEST_BAD = '''
fn probe() {
    let cargo = std::env::var_os("CARGO").unwrap();
    std::process::Command::new(&cargo).arg("build").status();
}
'''
SELF_TEST_GOOD = '''
pub const NESTED_CARGO_LOCK_ENV: [&str; 1] = ["CARGO_BUILD_BUILD_DIR"];
pub fn nested_cargo() -> Command {
    let mut cmd = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    for var in NESTED_CARGO_LOCK_ENV { cmd.env_remove(var); }
    cmd
}
fn probe() {
    nested_cargo().arg("build").status();
}
fn not_cargo() {
    Command::new("git").arg("describe").status();
}
'''


SELF_TEST_HOLLOW = SELF_TEST_GOOD.replace('"CARGO_BUILD_BUILD_DIR"', '"SOMETHING_ELSE"')


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        bad = Path(tmp) / "bad.rs"
        bad.write_text(SELF_TEST_BAD, encoding="utf-8")
        good = Path(tmp) / "good.rs"
        good.write_text(SELF_TEST_GOOD, encoding="utf-8")
        found, seen = offenders([bad])
        if len(found) != 1 or seen != 1:
            raise SystemExit(
                "check-nested-cargo-build-dir: SELF-TEST FAILED -- a raw nested "
                f"`$CARGO build` was not refused (found {found}, {seen} site(s))"
            )
        found, seen = offenders([good])
        if found or seen != 2:
            raise SystemExit(
                "check-nested-cargo-build-dir: SELF-TEST FAILED -- the helper "
                f"and its caller must pass as 2 sites (found {found}, {seen})"
            )
        hollow = Path(tmp) / "hollow.rs"
        hollow.write_text(SELF_TEST_HOLLOW, encoding="utf-8")
        found, _ = offenders([hollow])
        if len(found) != 1 or "nested_cargo" not in found[0][2]:
            raise SystemExit(
                "check-nested-cargo-build-dir: SELF-TEST FAILED -- a helper whose "
                f"lock list does not name CARGO_BUILD_BUILD_DIR passed ({found})"
            )


def main() -> int:
    self_test()
    paths = scope()
    helper = (REPO / HELPER_CRATE / "src" / "lib.rs").resolve()
    if helper not in paths:
        print(
            "check-nested-cargo-build-dir: FAIL -- the derived build-context "
            f"scope does not reach {HELPER_CRATE}, the crate known to nest "
            "cargo, so the derivation is broken, not the tree clean",
            file=sys.stderr,
        )
        return 1
    bad, sites = offenders(paths)
    if bad:
        print(
            "check-nested-cargo-build-dir: FAIL -- a cargo run from inside a "
            "build that may inherit CARGO_BUILD_BUILD_DIR, the lock the outer "
            "build holds while it waits (a silent deadlock):\n",
            file=sys.stderr,
        )
        for shown, line, sig in bad:
            print(f"  {shown}:{line}: {sig}", file=sys.stderr)
        print(
            "\n  Build it with `nros_sizes_build::nested_cargo()` (add "
            "nros-sizes-build as a build-dependency), not Command::new($CARGO).",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-nested-cargo-build-dir: OK ({sites} cargo spawn(s) in "
        f"{len(paths)} build-context source(s); each goes through nested_cargo())"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
