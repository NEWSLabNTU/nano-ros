#!/usr/bin/env python3
"""check-no-build-time-fetch — nano-ros's own build never fetches.

RFC-0104 D10 / phase-485 W8. Every tool and source comes from the SDK store
(`nros setup`), where it is sha256-verified and recorded in `nros-sdk.lock`. A
`FetchContent_Declare` or `ExternalProject_Add` in nano-ros's OWN build files
would put a dependency on the build that no lock records and that needs the
network at configure time. Corrosion's fallback was the last one; it now
refuses and names `nros setup --tool corrosion`.

Scope: nano-ros's own build machinery: `cmake/`, `zephyr/`, and the build
files under `packages/`. NOT a user's packages. RFC-0087 D5 lets a user's
vendor package fetch with a digest, and `check-vendor-fetch-pinned` holds that
rule. A comment that NAMES the call is not a call.

Buildless, ~0.1 s; self-tests its matcher on every run.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CALL = re.compile(r"^\s*(FetchContent_Declare|ExternalProject_Add|FetchContent_MakeAvailable)\s*\(",
                  re.I)
SCOPES = ("cmake/", "zephyr/", "packages/")


def is_build_file(rel: str) -> bool:
    name = rel.rsplit("/", 1)[-1]
    return rel.startswith(SCOPES) and (name == "CMakeLists.txt" or name.endswith(".cmake"))


def calls(text: str) -> list[int]:
    return [i for i, line in enumerate(text.splitlines(), 1) if CALL.match(line)]


def self_test() -> None:
    assert calls("FetchContent_Declare(x GIT_REPOSITORY y)") == [1]
    assert calls("  ExternalProject_Add (x)") == [1]
    assert calls("# FetchContent_Declare(x) is gone") == [], "a comment is not a call"
    assert calls("include(FetchContent)") == [], "including the module fetches nothing"
    assert is_build_file("cmake/NanoRosCorrosion.cmake")
    assert is_build_file("packages/api/nros-c/CMakeLists.txt")
    assert not is_build_file("examples/x/CMakeLists.txt"), "a user's package is D5's"


def main() -> int:
    self_test()
    files = subprocess.run(["git", "-C", str(ROOT), "ls-files"], capture_output=True,
                           text=True, check=True).stdout.splitlines()
    bad = []
    for rel in filter(is_build_file, files):
        try:
            text = (ROOT / rel).read_text(errors="ignore")
        except FileNotFoundError:
            continue
        bad += [f"{rel}:{n}" for n in calls(text)]
    if bad:
        print("check-no-build-time-fetch: nano-ros's own build fetches at configure time "
              "(RFC-0104 D10):\n  " + "\n  ".join(bad) +
              "\nProvision it as a `[tool.*]`/`[source.*]` and refuse when it is absent, "
              "naming `nros setup`.", file=sys.stderr)
        return 1
    print("check-no-build-time-fetch: OK (no FetchContent/ExternalProject in nano-ros's own build)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
