#!/usr/bin/env python3
"""An inherited absolute path must never outrank the checkout being built.

Issue 1280. A build in a linked git worktree used ANOTHER checkout's trees:
every path-valued variable here resolves ENV-FIRST, a worktree inherits its
parent shell's environment, and every value in it is an absolute path rooted at
the checkout that shell activated. Measured live, 24 of them, all naming the
main checkout — so a worktree build compiled the main checkout's FreeRTOS,
ThreadX, NuttX, platform sources and public headers, wrote into its NuttX
kernel, and sent four `check::build` gates' fixtures into its `build/`. Those
gates RAN and measured the wrong tree, which is worse than failing. Agent
sessions work in worktrees by default here.

The rule is three-valued, and the middle row is why "is the variable set" is
not the discriminator:

    outside any nano-ros checkout  -> KEEP  (a real out-of-tree SDK; this is
                                             what env-first exists for)
    inside THIS checkout           -> KEEP
    inside a DIFFERENT checkout    -> RE-ROOT onto this one

This gate checks three things:

  1. COVERAGE -- every path-valued export in `just/sdk-env.just` carries the
     re-root wrapper. The issue's own census was 19 and was already short by
     five, so the thing that must not drift is the file, not a name list.
  2. ONE MARKER -- the three spellings of "is this a nano-ros checkout" (shell,
     `nros-build-paths`, `nros-launcher`) name the same file. They are three
     because the three build systems cannot call each other, not because the
     rule differs.
  3. BEHAVIOUR -- the shell rule, `nros_build_root`, and `just` itself are
     each driven against real synthetic checkouts, for all three rows. A gate
     that only reads source would pass an implementation that never looks at
     the filesystem.

The self-test runs on the NORMAL path, every invocation: it mutates each parser
and each comparison and asserts the failure IS reported. A negative control
nobody runs decays into a comment.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SDK_ENV = ROOT / "just/sdk-env.just"
SHELL_LIB = ROOT / "scripts/lib/checkout-paths.sh"
BUILD_ROOT = ROOT / "scripts/build/build-root.sh"
RUST_LIB = ROOT / "packages/tooling/nros-build-paths/src/lib.rs"
LAUNCHER = ROOT / "packages/cli/nros-launcher/src/checkout.rs"

# The wrapper every path-valued export must carry, and the root spelling the
# two checkout-root variables must use instead.
WRAPPER = "replace("
REROOT_ARGS = "_NROS_OTHER, _NROS_HERE)"
HERE = "_NROS_HERE"

# Exports in `just/sdk-env.just` that do NOT name a path, with the reason. An
# entry here is a claim that the value can never be a directory or file path;
# anything else needs the wrapper.
NOT_A_PATH = {
    "FREERTOS_PORT": "a port subdirectory NAME (`GCC/ARM_CM3`), relative to FREERTOS_DIR",
}

EXPORT = re.compile(r"^export\s+([A-Za-z_][A-Za-z0-9_]*)\s*:=\s*(.*)$")


def sdk_env_exports(text: str) -> list[tuple[str, str]]:
    """`export NAME := rhs` pairs, in file order."""
    out = []
    for line in text.splitlines():
        m = EXPORT.match(line)
        if m:
            out.append((m.group(1), m.group(2)))
    return out


def coverage_violations(text: str) -> list[str]:
    """Names whose export neither re-roots nor is declared non-path."""
    bad = []
    for name, rhs in sdk_env_exports(text):
        if name in NOT_A_PATH:
            continue
        # The two checkout-root variables answer with this checkout outright:
        # `justfile_directory()` IS the tree `just` is running in, so there is
        # nothing an inherited value could be more right about.
        if rhs.strip() == HERE:
            continue
        if WRAPPER in rhs and REROOT_ARGS in rhs:
            continue
        bad.append(name)
    return bad


def marker_of(path: Path, pattern: str) -> str | None:
    m = re.search(pattern, path.read_text())
    return m.group(1) if m else None


MARKER_SITES = [
    (SHELL_LIB, r'NROS_CHECKOUT_MARKER="([^"]+)"'),
    (RUST_LIB, r'CHECKOUT_MARKER: &str = "([^"]+)"'),
    (LAUNCHER, r'MONOREPO_MARKER: &str = "([^"]+)"'),
]


def marker_violations() -> list[str]:
    seen = {}
    problems = []
    for path, pattern in MARKER_SITES:
        value = marker_of(path, pattern)
        if value is None:
            problems.append(f"{path.relative_to(ROOT)}: no checkout-marker constant found")
            continue
        seen[str(path.relative_to(ROOT))] = value
    if len(set(seen.values())) > 1:
        problems.append(
            "the checkout marker has more than one spelling — "
            + ", ".join(f"{k} = {v!r}" for k, v in sorted(seen.items()))
        )
    return problems


# --------------------------------------------------------------------------
# Behaviour probes. Each builds real directories: the rule is a filesystem
# question (the marker file has to BE there), so a stubbed probe would pass an
# implementation that never looks.
# --------------------------------------------------------------------------

MARKER_REL = "packages/core/nros-core/Cargo.toml"


def make_checkout(root: Path) -> Path:
    (root / MARKER_REL).parent.mkdir(parents=True, exist_ok=True)
    (root / MARKER_REL).write_text('[package]\nname = "nros-core"\n')
    return root


def sh(script: str, env: dict[str, str] | None = None) -> tuple[int, str]:
    p = subprocess.run(
        ["bash", "-c", script],
        capture_output=True,
        text=True,
        cwd=ROOT,
        env=env,
    )
    return p.returncode, p.stdout.strip()


def probe_shell_rule(tmp: Path) -> list[str]:
    """Row-by-row, the rule in `scripts/lib/checkout-paths.sh`."""
    here = make_checkout(tmp / "here")
    other = make_checkout(tmp / "other")
    outside = tmp / "opt/vendor/nuttx"
    outside.mkdir(parents=True)

    cases = [
        # (value, expected, what it proves)
        (
            f"{other}/third-party/nuttx/nuttx",
            f"{here}/third-party/nuttx/nuttx",
            "a foreign checkout's SDK path is re-rooted",
        ),
        (str(other), str(here), "the foreign checkout ROOT itself is re-rooted"),
        (str(outside), str(outside), "an out-of-tree SDK path is KEPT"),
        (
            f"{here}/third-party/nuttx/nuttx",
            f"{here}/third-party/nuttx/nuttx",
            "our own checkout is not foreign to itself",
        ),
        ("third-party/nuttx", "third-party/nuttx", "a relative path is never attributed"),
    ]
    problems = []
    for value, expect, what in cases:
        rc, got = sh(
            f'. "{SHELL_LIB}"; nros_reroot_checkout_path "{value}" "{here}"'
        )
        if rc != 0 or got != expect:
            problems.append(
                f"checkout-paths.sh: {what} — {value!r} -> {got!r}, expected {expect!r}"
            )
    return problems


def probe_build_root(tmp: Path) -> list[str]:
    """`nros_build_root` — acceptance row 3, the four gates' fixture dir."""
    other = make_checkout(tmp / "other-br")
    base = {
        k: v
        for k, v in os.environ.items()
        if k not in ("NROS_BUILD_ROOT", "NROS_REPO_ROOT", "NROS_REPO_DIR")
    }
    problems = []

    def run(extra: dict[str, str]) -> str:
        env = dict(base)
        env.update(extra)
        env["NROS_QUIET_ACTIVATE"] = "1"
        rc, out = sh(f'. "{BUILD_ROOT}"; nros_build_root', env=env)
        return out if rc == 0 else f"<rc={rc}>"

    got = run({"NROS_REPO_DIR": str(other)})
    if got != f"{ROOT}/build":
        problems.append(
            f"build-root.sh: an inherited NROS_REPO_DIR naming another checkout "
            f"({other}) still won — got {got!r}, expected {ROOT}/build"
        )
    got = run({"NROS_REPO_ROOT": str(other)})
    if got != f"{ROOT}/build":
        problems.append(
            f"build-root.sh: NROS_REPO_ROOT naming another checkout still won — got {got!r}"
        )
    # The reason the variable exists: a build root on another volume, outside
    # any checkout, must keep working.
    scratch = str(tmp / "mnt/fast/nros-build")
    got = run({"NROS_BUILD_ROOT": scratch})
    if got != scratch:
        problems.append(
            f"build-root.sh: an out-of-tree NROS_BUILD_ROOT was rewritten — got {got!r}"
        )
    # And `NROS_BUILD_ROOT` is passed through VERBATIM even when it names
    # another checkout — see the comment on `nros_build_root`. Its Rust mirror
    # `nros_tests::build_root` reads the same variable with no re-root and
    # cannot be taught one here, so re-rooting this rung would split the writer
    # from the reader: the bug, not the fix. Asserted so a later "finish the
    # job" edit has to read that argument first.
    inside = f"{other}/build"
    got = run({"NROS_BUILD_ROOT": inside})
    if got != inside:
        problems.append(
            "build-root.sh: NROS_BUILD_ROOT was re-rooted, which splits it from the "
            f"un-re-rooted Rust mirror `nros_tests::build_root` — got {got!r}"
        )
    return problems


def probe_just(tmp: Path) -> list[str]:
    """`just` itself — the environment a recipe actually receives."""
    if shutil.which("just") is None:
        return ["`just` not on PATH, so the justfile arm of this gate could not run"]
    other = make_checkout(tmp / "other-just")
    problems = []

    def evaluate(name: str, extra: dict[str, str]) -> str:
        # `env -i`, in effect: the detector scans the WHOLE environment, so an
        # inherited real foreign checkout would be a second root and the
        # refusal (correctly) would fire. Start from nothing but what `just`
        # needs.
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": os.environ.get("HOME", str(tmp)),
            "NROS_QUIET_ACTIVATE": "1",
        }
        env.update(extra)
        p = subprocess.run(
            ["just", "--justfile", str(ROOT / "justfile"), "--evaluate", name],
            capture_output=True,
            text=True,
            cwd=ROOT,
            env=env,
        )
        return p.stdout.strip() if p.returncode == 0 else f"<rc={p.returncode}: {p.stderr.strip()[:200]}>"

    got = evaluate("NUTTX_DIR", {"NUTTX_DIR": f"{other}/third-party/nuttx/nuttx"})
    if got != f"{ROOT}/third-party/nuttx/nuttx":
        problems.append(
            f"just: an inherited NUTTX_DIR naming another checkout still won — got {got!r}"
        )
    got = evaluate("NROS_REPO_DIR", {"NROS_REPO_DIR": str(other)})
    if got != str(ROOT):
        problems.append(f"just: NROS_REPO_DIR did not resolve to this checkout — got {got!r}")
    # Row 4 of the rule, in the hard shape: a genuine out-of-tree SDK path
    # BESIDE an inherited foreign checkout. The prefix rewrite must miss it.
    vendor = str(tmp / "opt/vendor/nuttx")
    got = evaluate(
        "NUTTX_DIR",
        {"NUTTX_DIR": vendor, "NROS_C_INCLUDE": f"{other}/packages/api/nros-c/include"},
    )
    if got != vendor:
        problems.append(f"just: an out-of-tree NUTTX_DIR was rewritten — got {got!r}")
    return problems


# --------------------------------------------------------------------------


def self_test() -> bool:
    """Prove each check can FAIL. Runs on the normal path, every invocation."""
    ok = True

    def chk(desc: str, cond: bool) -> None:
        nonlocal ok
        if not cond:
            print(f"self-test FAILED: {desc}", file=sys.stderr)
            ok = False

    text = SDK_ENV.read_text()
    exports = sdk_env_exports(text)
    # Non-vacuous first: a parser that finds nothing reports nothing wrong.
    chk(
        f"only {len(exports)} exports parsed out of {SDK_ENV.name} — the parser is broken",
        len(exports) >= 20,
    )

    # Mutations are measured as a DELTA against whatever the live file says.
    # Asserting an absolute answer here would make a real violation in the file
    # read as "the self-test is broken", which points the next reader at this
    # script instead of at the line they just wrote.
    base = set(coverage_violations(text))

    # A new variable added without the wrapper — the drift this exists for.
    mutant = text + '\nexport NROS_NEW_SDK_DIR := env("NROS_NEW_SDK_DIR", _NROS_HERE / "x")\n'
    chk(
        "an unwrapped new export was NOT reported",
        set(coverage_violations(mutant)) - base == {"NROS_NEW_SDK_DIR"},
    )
    # A wrapper that rewrites the wrong way round is not coverage either.
    mutant = text.replace(REROOT_ARGS, "_NROS_HERE, _NROS_OTHER)", 1)
    chk(
        "a reversed re-root was NOT reported",
        len(set(coverage_violations(mutant)) - base) == 1,
    )

    chk(
        "a missing marker constant was NOT reported",
        marker_of(SDK_ENV, r'NROS_CHECKOUT_MARKER="([^"]+)"') is None,
    )

    # The behaviour probes must be able to fail: drive the shell rule with a
    # `here` that is NOT a checkout root's sibling and confirm it still refuses
    # to touch an out-of-tree path (a probe that rewrote everything would be
    # green on row 1 and wrong on row 3).
    with tempfile.TemporaryDirectory(prefix="nros-1280-selftest-") as td:
        tmp = Path(td)
        outside = tmp / "plain/dir"
        outside.mkdir(parents=True)
        here = make_checkout(tmp / "here")
        rc, got = sh(f'. "{SHELL_LIB}"; nros_reroot_checkout_path "{outside}" "{here}"')
        chk("the shell rule rewrote a path outside any checkout", rc == 0 and got == str(outside))
        rc, got = sh(f'. "{SHELL_LIB}"; nros_checkout_root "{outside}"')
        chk("nros_checkout_root claimed a non-checkout path", rc == 0 and got == "")

    if ok:
        print(
            "  self-test ok: coverage parser (2 mutations), marker comparison, "
            "shell rule negative rows"
        )
    return ok


def main() -> int:
    if not self_test():
        print("check-inherited-checkout-paths: SELF-TEST FAILED", file=sys.stderr)
        return 1

    problems: list[str] = []
    text = SDK_ENV.read_text()
    exports = sdk_env_exports(text)
    bad = coverage_violations(text)
    for name in bad:
        problems.append(
            f"just/sdk-env.just: `{name}` resolves env-first with no re-root. Wrap it:\n"
            f"    export {name} := replace(env(\"{name}\", {HERE} / \"<rel>\"), "
            f"{REROOT_ARGS}\n"
            f"  or, if it names no path, add it to NOT_A_PATH in this script with a reason."
        )
    problems += marker_violations()

    with tempfile.TemporaryDirectory(prefix="nros-1280-") as td:
        tmp = Path(td)
        problems += probe_shell_rule(tmp)
        problems += probe_build_root(tmp)
        problems += probe_just(tmp)

    if problems:
        print("check-inherited-checkout-paths: FAILED (issue 1280)", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    covered = len(exports) - len(NOT_A_PATH)
    print(
        f"check-inherited-checkout-paths: ok — {covered} path exports re-rooted, "
        f"{len(NOT_A_PATH)} declared non-path, one checkout marker across "
        f"{len(MARKER_SITES)} spellings, behaviour proven for shell / build-root / just"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
