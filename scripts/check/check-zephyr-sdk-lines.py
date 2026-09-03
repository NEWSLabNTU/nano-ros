#!/usr/bin/env python3
"""The CI image must bake an SDK for every Zephyr line `setup.sh` can select.

`scripts/zephyr/setup.sh` says it in caps — "THE SDK IS A PER-LINE FACT, NOT A
CONSTANT" — and dispatches per manifest: 3.7 LTS wants 0.16.8, 4.4 wants 1.0.1.
`nros-sdk-index.toml` carries a second entry for the same reason.

The zephyr CI image baked ONE SDK (0.17.4) on the theory that it served both:
newer-satisfies-older, and 3.7 sets no upper bound. That held until the 4.4
line moved to the 1.x series, and then it failed the only way a version
mismatch can:

    Could not find a configuration file for package "Zephyr-sdk" that is
    compatible with requested version "1.0".
      considered: /opt/zephyr-sdk/cmake/Zephyr-sdkConfig.cmake, version: 0.17.4

Every `zephyr 4.4 /*` nightly cell died there, and the image's own comment
explained why it was fine — the 4.4 line "must fall through to its own
west-managed SDK" — while every CI job runs `just zephyr setup --skip-sdk`, so
no such SDK existed. Two documents, each locally consistent, describing
different worlds.

THE RULE. For each version `setup.sh` can select, the image must bake a version
in the SAME MAJOR SERIES that is not older. Same-major because that is what the
SDK's own `Zephyr-sdkConfigVersion.cmake` compares: it declares compatibility
with any request no newer than itself, and the 0.x -> 1.x move is exactly where
"newer satisfies older" stopped being enough to reason about by eye.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SETUP = REPO / "scripts/zephyr/setup.sh"
DOCKERFILE = REPO / "ci/docker/zephyr-ros/Dockerfile"


def versions(text: str, pattern: str) -> list[str]:
    return re.findall(pattern, text)


def parse(v: str) -> tuple[int, ...]:
    return tuple(int(p) for p in v.split("."))


def self_test() -> None:
    """Both directions, on synthetic input — `check-gate-selftests` requires it."""
    assert versions('ZEPHYR_SDK_VERSION="1.0.1"', r'ZEPHYR_SDK_VERSION="([0-9.]+)"') == [
        "1.0.1"
    ], "selftest: setup.sh pattern missed an assignment"
    assert versions("ARG ZEPHYR_SDK_VERSION=0.17.4", r"ARG ZEPHYR_SDK_VERSION[A-Z_0-9]*=([0-9.]+)") == [
        "0.17.4"
    ], "selftest: Dockerfile pattern missed an ARG"
    # The comparison itself: same major, not older.
    assert parse("0.17.4") >= parse("0.16.8")
    assert not (parse("0.17.4")[0] == parse("1.0.1")[0]), (
        "selftest: 0.x must not be treated as satisfying 1.x — that is the bug"
    )


def main() -> int:
    required = versions(SETUP.read_text(), r'ZEPHYR_SDK_VERSION="([0-9.]+)"')
    baked = versions(
        DOCKERFILE.read_text(), r"ARG ZEPHYR_SDK_VERSION[A-Z_0-9]*=([0-9.]+)"
    )
    if not required or not baked:
        print(
            "check-zephyr-sdk-lines: read no versions "
            f"(setup.sh: {required}, Dockerfile: {baked}). A pattern stopped "
            "matching; that is a broken gate, not a passing one.",
            file=sys.stderr,
        )
        return 1

    missing = []
    for want in sorted(set(required)):
        ok = any(
            parse(have)[0] == parse(want)[0] and parse(have) >= parse(want)
            for have in baked
        )
        if not ok:
            missing.append(want)

    if missing:
        print("check-zephyr-sdk-lines: the CI image cannot serve every Zephyr line\n")
        for want in missing:
            print(
                f"  setup.sh can select SDK {want}, and the image bakes "
                f"{', '.join(baked)} — none in the {parse(want)[0]}.x series at "
                f"or above it."
            )
        print(
            "\nAdd the SDK to ci/docker/zephyr-ros/Dockerfile. Every CI job runs\n"
            "`just zephyr setup --skip-sdk`, so the image is the ONLY source of\n"
            "an SDK there — a line whose SDK is missing fails at cmake configure\n"
            "with a message naming neither the line nor this file."
        )
        return 1

    print(
        f"check-zephyr-sdk-lines OK — {len(set(required))} line(s) "
        f"({', '.join(sorted(set(required)))}), image bakes {', '.join(baked)}."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
