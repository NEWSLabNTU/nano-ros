#!/usr/bin/env python3
"""Generate the sizes probe's orchestration-knob exclusions (issue 2840).

`nros-sizes-build` keys its probe directory on every `NROS_*` variable in the
environment except the ones argued not to change a probed size
(`KNOBS_THAT_CANNOT_CHANGE_A_SIZE`, phase-353 W4). An unknown name keys the
probe on purpose: a knob that changes a size and does not key it hands one image
another's sizes (issue 0528). The cost of that default is a wasted directory
for every variable the build ORCHESTRATION sets, and the orchestration keeps
gaining them. Measured on the 118 runner's persistent fixture tree, 2026-11-24:
about 28 such names had reached the probe keys (`NROS_FIXTURE_FAILED_ROWS`
differs per lane, `NROS_SUBMODULE_PIN_BASELINE` per base commit, the
`NROS_RUNNER_*` per runner), and they split 22 of 271 probe keys. The hand list
was written from one 2026-08-15 measurement and could not keep up.

So the orchestration half of the list is HARVESTED here, never typed:

  set   every `NROS_*` name the orchestration ASSIGNS -- tracked workflows,
        shell scripts, justfiles and the activate scripts (`NAME=`,
        `export NAME=`, `export NAME :=` in a justfile, a YAML `env:`
        key);
  read  every `NROS_*` name any source that can reach a compile NAMES -- every
        tracked file under `packages/`, `cmake/` and `zephyr/` (Rust, C, CMake,
        Kconfig, TOML), plus every `NROS_*` spelling the knob registry
        derives, live or retired (`scripts/lib/knob_registry.py`; a row states
        a key, so its env name appears in no file);
  out   set minus read.

A name in `out` is assigned by a script and read by nothing that compiles, so
it cannot change a probed size: whatever effect it has reaches a compile only
through some OTHER name or file, which still keys the probe. The read set is
deliberately wider than what a probe compiles (every board, every port, every
tool crate), because over-reading only keeps a name keyed -- the safe
direction. Two trees are left out, and why:

* `packages/testing/` -- the test harness and the fixture leaves. The probe
  builds the nano-ros crates a leaf depends on, never the harness or a leaf's
  own crate, so what the harness reads (`NROS_TEST_COORDS`, the lane files)
  never reaches a probed compile.
* `packages/cli/` -- the `nros` CLI is not compiled by a probe, and no probed
  build script runs it. That second half is checked, not assumed: a build
  script in the read set that names the CLI fails the harvest. What the CLI
  hands a build (a descriptor, a board file) reaches the probe through a name
  that keys it by content.

Three rules about spellings:

* A read token that ends in `_` and is followed by `{`, `$`, a quote or `%`
  (`format!("NROS_KNOB_SOURCE_{knob}")`, `starts_with("NROS_KIND_")`) is a
  PREFIX: every name it starts is read. The same token followed by `*` or
  prose (`the NROS_ZEPHYR_* knobs`) is a comment describing a family, not a
  read, and is ignored, so a doc comment cannot shield a whole family.
* The bare `NROS_` is not counted. It appears where a forwarder passes the
  whole family on (`nros-cli-core`'s cmake configure) or a name is built from
  a registry id (`format!("NROS_{id}")`, which the registry rows cover). The
  reader downstream of a forwarder names its variable, and that name is in the
  read set.

`nros-sizes-build` itself is not read: it computes the key, and its own list
of excluded names would otherwise read as reads.

Usage:
  gen-probe-orchestration-knobs.py            regenerate the Rust table
  gen-probe-orchestration-knobs.py --check    fail when the table excludes a
                                              name a probed compile reads
  gen-probe-orchestration-knobs.py --self-test
Both modes run the self-test first.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
import knob_registry  # noqa: E402  the one reader of the knob registry

REPO = Path(__file__).resolve().parent.parent
OUT = "packages/tooling/nros-sizes-build/src/orchestration_knobs.rs"
KEY_CRATE = "packages/tooling/nros-sizes-build/"
NOT_PROBED = ("packages/testing/", "packages/cli/")

SET_PATHSPECS = [
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    ".github/actions/**",
    "*.sh",
    "**/*.sh",
    "justfile",
    "**/*.just",
    "activate.sh",
    "activate.fish",
]
READ_PATHSPECS = ["packages/**", "cmake/**", "zephyr/**"]
READ_SUFFIXES = (
    ".rs", ".c", ".h", ".hpp", ".cpp", ".cmake", ".txt", ".toml", ".conf",
    ".py", ".in", ".ld", ".defconfig",
)
READ_NAMES = ("CMakeLists.txt", "Kconfig", "build.rs")

# Never excluded, whatever a harvest says: each names a FILE whose content
# carries sizing knobs, or is the descriptor itself (issue 0528 by a new route).
# `nros-sizes-build`'s own test holds the same set.
PROTECTED = {
    "NROS_BOARD_TOML",
    "NROS_PLATFORMS_DIR",
    "NROS_MODEL_DIR",
    "NROS_HOME",
    "NROS_SIZING_DESCRIPTOR",
}

TOKEN = re.compile(r"NROS_[A-Z0-9_]*")
READ_TOKEN = re.compile(r"(NROS_[A-Z0-9_]*)(.?)")
PREFIX_FOLLOWERS = set("{$\"'%")
ASSIGN = re.compile(
    r"(?:^|[\s;(`'\"]|\bexport\s+|\benv\s+)(NROS_[A-Z0-9_]+)\+?="
    r"|^\s*(NROS_[A-Z0-9_]+):\s"
    r"|^\s*(?:export\s+)?(NROS_[A-Z0-9_]+)\s*:=",
    re.M,
)
CLI_RUN = re.compile(r'Command::new\(\s*"nros"|\bNROS_CLI(?:_BIN)?\b')


def tracked(root: Path, specs: list[str]) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--", *specs],
        capture_output=True,
        check=True,
    ).stdout
    return [p for p in out.decode().split("\0") if p]


def read_text(root: Path, rel: str) -> str:
    try:
        return (root / rel).read_text(errors="ignore")
    except (OSError, IsADirectoryError):
        return ""


def registry_names() -> set[str]:
    reg = knob_registry.registry()
    parts = (reg["knob"], reg["statement"], reg["env"], reg["kconfig"])
    return {s for s, _ in knob_registry.spellings(parts) if s.startswith("NROS_")}


def harvest(root: Path, registry: set[str]) -> list[str]:
    assigned, is_read = scan(root, registry)
    return sorted(
        n
        for n in assigned
        if not is_read(n) and n not in PROTECTED and not n.startswith("NROS_SIZES_")
    )


def scan(root: Path, registry: set[str]):
    """(every assigned name, a predicate: does a probed compile read this?)"""
    assigned: set[str] = set()
    for rel in tracked(root, SET_PATHSPECS):
        for m in ASSIGN.finditer(read_text(root, rel)):
            assigned.add(m.group(1) or m.group(2) or m.group(3))

    names: set[str] = set()
    prefixes: set[str] = set()
    for rel in tracked(root, READ_PATHSPECS):
        if rel.startswith(KEY_CRATE) or rel.startswith(NOT_PROBED):
            continue
        base = rel.rsplit("/", 1)[-1]
        if not (base.endswith(READ_SUFFIXES) or base in READ_NAMES):
            continue
        text = read_text(root, rel)
        if base == "build.rs" and CLI_RUN.search(text):
            sys.exit(
                f"gen-probe-orchestration-knobs: {rel} runs the nros CLI, so the CLI's\n"
                "  reads reach a probed compile and packages/cli/ can no longer be left\n"
                "  out of the read set. Take it out of NOT_PROBED and regenerate."
            )
        for tok, nxt in READ_TOKEN.findall(text):
            if tok == "NROS_":
                continue
            if tok.endswith("_"):
                if nxt in PREFIX_FOLLOWERS:
                    prefixes.add(tok)
            else:
                names.add(tok)
    names |= registry

    def is_read(n: str) -> bool:
        return n in names or any(n.startswith(p) for p in prefixes)

    return assigned, is_read


def render(names: list[str]) -> str:
    rows = "".join(f'    "{n}",\n' for n in names)
    return (
        "// @generated by scripts/gen-probe-orchestration-knobs.py -- do not edit.\n"
        "// Regenerate with `python3 scripts/gen-probe-orchestration-knobs.py`;\n"
        "// `just check probe-orchestration-knobs` fails when this is stale.\n"
        "//\n"
        "// Every `NROS_*` name the build orchestration assigns and no source that\n"
        "// can reach a compile reads (issue 2840). None can change a probed size,\n"
        "// so none keys the sizes probe. The argument is in the generator.\n"
        "\n"
        "pub(crate) const ORCHESTRATION_KNOBS: &[&str] = &[\n"
        f"{rows}"
        "];\n"
    )


def self_test() -> None:
    """A synthetic repository with one of each case, so the rule is checked,
    not asserted. Runs before every real use."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        files = {
            "scripts/a.sh": (
                "export NROS_ORCH_ONLY=1\n"
                "NROS_READ_BY_CRATE=2 cargo build\n"
                "NROS_PREFIXED_THING=3\n"
                "NROS_KNOB_IN_REGISTRY=4\n"
                "NROS_BOARD_TOML=/x\n"
                "NROS_READ_BY_CMAKE=5\n"
                "NROS_SIZES_PROBE_X=6\n"
                "NROS_ONLY_KEY_CRATE=7\n"
                "NROS_PROSE_FAMILY_X=8\n"
                "NROS_READ_BY_HARNESS=9\n"
            ),
            "justfile": "export NROS_JUST_EXPORT := `true`\n",
            "packages/core/x/src/lib.rs": "//! The NROS_PROSE_FAMILY_* knobs are prose, not a read.\n",
            "packages/testing/t/src/lib.rs": 'const T: &str = "NROS_READ_BY_HARNESS";\n',
            ".github/workflows/w.yml": (
                "env:\n  NROS_YAML_ONLY: 1\n"
                "steps:\n  - run: echo \"NROS_ENV_FILE_ONLY=1\" >> \"$GITHUB_ENV\"\n"
            ),
            "packages/core/x/build.rs": (
                'fn main() { let _ = std::env::var("NROS_READ_BY_CRATE");\n'
                '  let _ = std::env::var(format!("NROS_PREFIXED_{}", 1));\n'
                '  let _ = std::env::var(format!("NROS_{}", 2)); }\n'
            ),
            "cmake/y.cmake": "if(DEFINED ENV{NROS_READ_BY_CMAKE})\nendif()\n",
            "packages/tooling/nros-sizes-build/src/lib.rs": (
                'const X: &str = "NROS_ONLY_KEY_CRATE";\n'
            ),
        }
        for rel, text in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        # Issue 0986: a hook's inherited GIT_DIR would aim this at the caller.
        env = nros_clear_inherited_git_env(dict(os.environ))
        run = lambda *a: subprocess.run(  # noqa: E731
            ["git", "-C", str(root), *a], check=True, capture_output=True, env=env
        )
        run("init", "-q")
        run("add", ".")
        got = harvest(root, {"NROS_KNOB_IN_REGISTRY"})
    want = [
        "NROS_ENV_FILE_ONLY",
        "NROS_JUST_EXPORT",
        "NROS_ONLY_KEY_CRATE",
        "NROS_ORCH_ONLY",
        "NROS_PROSE_FAMILY_X",
        "NROS_READ_BY_HARNESS",
        "NROS_YAML_ONLY",
    ]
    if got != want:
        sys.exit(
            "gen-probe-orchestration-knobs: SELF-TEST FAILED\n"
            f"  harvested {got}\n  expected  {want}\n"
            "  (a crate read, a prefix read, a registry knob, a cmake read, a\n"
            "  protected name and the NROS_SIZES_ family must all stay keyed;\n"
            "  the bare NROS_ forwarder, a prose `NROS_X_*` and a harness read\n"
            "  must not shield anything)"
        )


def main() -> int:
    self_test()
    if "--self-test" in sys.argv[1:]:
        print("gen-probe-orchestration-knobs: self-test OK")
        return 0
    registry = registry_names()
    text = render(harvest(REPO, registry))
    path = REPO / OUT
    if "--check" in sys.argv[1:]:
        # The two directions cost differently, so only one fails. A table
        # entry that a probed compile now READS is issue 0528: that knob no
        # longer keys the probe, and one image can be handed another's sizes.
        # A new orchestration name the table lacks only costs a probe
        # directory, the state every name was in before this table, so it is a
        # note. Failing on it would make every change that adds a script
        # variable regenerate this file, and two such changes in one merge
        # group would eject each other.
        have = new_names(path.read_text()) if path.exists() else []
        want = new_names(text)
        _, is_read = scan(REPO, registry)
        dangerous = sorted(n for n in have if is_read(n) or n in PROTECTED)
        if dangerous:
            print(f"{OUT} excludes a name a probed compile reads:", file=sys.stderr)
            for n in dangerous:
                print(f"  - {n} (must key the probe again, issue 0528)", file=sys.stderr)
            print("Regenerate: python3 scripts/gen-probe-orchestration-knobs.py", file=sys.stderr)
            return 1
        added = sorted(set(want) - set(have))
        gone = sorted(set(have) - set(want))
        if added or gone:
            print(
                f"probe-orchestration-knobs: NOTE {len(added)} new orchestration name(s) still "
                f"key the probe, {len(gone)} entry(ies) no longer assigned; regenerate to "
                "share those probe keys: " + " ".join([f"+{n}" for n in added] + [f"-{n}" for n in gone])
            )
        print(f"probe-orchestration-knobs: OK ({len(have)} names, none read by a compile, self-test OK)")
        return 0
    path.write_text(text)
    print(f"wrote {OUT} ({len(new_names(text))} names)")
    return 0


def new_names(text: str) -> list[str]:
    return re.findall(r'^\s+"(NROS_[A-Z0-9_]+)",$', text, re.M)


if __name__ == "__main__":
    sys.exit(main())
