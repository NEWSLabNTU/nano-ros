#!/usr/bin/env python3
"""check-retired-store-vars — one variable per root (RFC-0103 D6).

The store root is `NROS_HOME`; the nano-ros root's env name is `NROS_REPO_DIR`.

`NROS_HOME` names the nano-ros store root. Two more names used to answer the
same question in different orders: the CLI read `NROS_STORE` first, and cmake's
cross toolchain plus the riscv64 helpers read `NROS_SDK_STORE` (which meant
`<root>/sdk`) and ignored `NROS_HOME`, so one host could resolve two stores
(issue 1767). Both are retired; every resolver refuses a set one.

This gate keeps a READ of either name from coming back anywhere outside the
sites that exist to refuse it. It keys on read SHAPES — a shell expansion,
`$ENV{}` / `DEFINED ENV{}` in cmake, `env::var*("…")` in Rust,
`os.environ` / `getenv` in Python, `set -q` in fish — so prose that says the
name is retired is not a finding, and a new reader in any of the four
languages is. Buildless, tracked files only; self-tests every shape on each run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

NAMES = r"NROS_(?:SDK_)?STORE"
READ = re.compile(
    r"|".join(
        [
            rf"\$\{{?{NAMES}\b",  # shell / just / cmake $NAME, ${NAME…}
            rf"ENV\{{{NAMES}\}}",  # cmake $ENV{NAME}, DEFINED ENV{NAME}
            rf"env::var(?:_os)?\(\s*\"{NAMES}\"",  # Rust
            rf"environ(?:\.get)?\s*[\[(]\s*[\"']{NAMES}[\"']",  # Python
            rf"getenv\(\s*[\"']{NAMES}[\"']",  # Python / C
            rf"set\s+-q\s+{NAMES}\b",  # fish
            rf"\.env\(\s*\"{NAMES}\"",  # Command::env in tests
            rf"(?:^|[\s;]){NAMES}=",  # an assignment that sets it
        ]
    )
)

# `NANO_ROS_ROOT` is retired only as an ENVIRONMENT variable (RFC-0103 D6:
# `NROS_REPO_DIR` is the one env name for the root). As a cmake variable and
# `-D` argument it is the explicit-argument rung and stays, so only env-read
# shapes count for it.
ENV_ONLY = r"NANO_ROS_ROOT"
ENV_READ = re.compile(
    r"|".join(
        [
            rf"ENV\{{{ENV_ONLY}\}}",
            rf"env::var(?:_os)?\(\s*\"{ENV_ONLY}\"",
            rf"environ(?:\.get)?\s*[\[(]\s*[\"']{ENV_ONLY}[\"']",
            rf"\bexport\s+{ENV_ONLY}=",
            rf"\.env\(\s*\"{ENV_ONLY}\"",
        ]
    )
)

# The sites whose job is to REFUSE the retired names, plus this gate and the
# store-enumeration gate whose fixtures replay historical shapes on purpose.
ALLOWED = {
    "packages/tooling/nros-build-paths/src/lib.rs",
    "scripts/lib/store-root.sh",
    "cmake/NanoRosStoreRoot.cmake",
    "activate.fish",
    "scripts/check/check-retired-store-vars.py",
    "scripts/check-sdk-store-not-enumerated.py",
    "cmake/NanoRosWorkspace.cmake",  # refuses $ENV{NANO_ROS_ROOT}
}

SELF_TESTS = [
    ('x="${NROS_SDK_STORE:-$HOME/.nros/sdk}"', True),
    ('printf "%s" "$NROS_STORE"', True),
    ("if(DEFINED ENV{NROS_SDK_STORE})", True),
    ('std::env::var("NROS_STORE")', True),
    ('os.environ.get("NROS_SDK_STORE")', True),
    ("if set -q NROS_STORE", True),
    ('.env("NROS_STORE", store)', True),
    ('HOME="$1/home" NROS_STORE="$1/store" cmd', True),
    ("# NROS_SDK_STORE is retired (RFC-0103 D6)", False),
    ('x="${NROS_HOME:-$HOME/.nros}"', False),
    ("`NROS_STORE` (which the CLI alone read)", False),
]
ENV_SELF_TESTS = [
    ('if(DEFINED ENV{NANO_ROS_ROOT})', True),
    ('export NANO_ROS_ROOT="$PROJECT_ROOT"', True),
    ('std::env::var_os("NANO_ROS_ROOT")', True),
    ('add_subdirectory("${NANO_ROS_ROOT}" nano_ros)', False),
    ('cmake -DNANO_ROS_ROOT="$root" ..', False),
    ('NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"', False),
]


def self_test() -> int:
    bad = [(t, want) for t, want in SELF_TESTS if bool(READ.search(t)) != want]
    bad += [(t, want) for t, want in ENV_SELF_TESTS if bool(ENV_READ.search(t)) != want]
    for t, want in bad:
        print(f"  self-test: {'missed' if want else 'false hit on'}: {t}")
    return len(bad)


def main() -> int:
    if (n := self_test()):
        print(f"check-retired-store-vars --self-test: {n} case(s) FAILED")
        return 1
    files = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True
    ).stdout.decode().split("\0")
    findings = []
    for rel in files:
        if not rel or rel in ALLOWED or rel.startswith("docs/") or rel.endswith(".md"):
            continue
        p = ROOT / rel
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        if "STORE" not in text and "NANO_ROS_ROOT" not in text:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if READ.search(line) or ENV_READ.search(line):
                findings.append(f"  {rel}:{i}: {line.strip()[:120]}")
    if findings:
        print("check-retired-store-vars: a retired store/root variable is READ:\n")
        print("\n".join(findings))
        print(
            "\nThe store root has one variable, NROS_HOME; the nano-ros root's one env name is"
            "\nNROS_REPO_DIR (RFC-0103 D6). Resolve the store through nros_build_paths::store,"
            "\nscripts/lib/store-root.sh or nros_store_root(); the root through the one walk."
        )
        return 1
    print(
        f"check-retired-store-vars: OK ({len(SELF_TESTS) + len(ENV_SELF_TESTS)} self-test cases; "
        f"no read of NROS_STORE / NROS_SDK_STORE / env NANO_ROS_ROOT outside "
        f"{len(ALLOWED)} refusal sites)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
