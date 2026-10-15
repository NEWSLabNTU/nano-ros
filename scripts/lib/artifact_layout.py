"""artifact_layout — the per-build header directory names (issue 1852).

The ONE definition is `nros_build_paths::artifact::{C,CPP}_GENERATED_DIR` in
`packages/tooling/nros-build-paths/src/artifact.rs`. This module READS them
from there rather than restating them, the same way
`cmake/NanoRosArtifactLayout.cmake` does, so a rename of the Rust constant
moves every reader. A Python script that needs a cargo TARGET DIR's copy asks
`nros locate artifact` instead; this is for scripts that recognise or build
the directories themselves (an `OUT_DIR` copy, a scan over target dirs).
"""

from __future__ import annotations

import re
from pathlib import Path

ARTIFACT_RS = (
    Path(__file__).resolve().parents[2]
    / "packages" / "tooling" / "nros-build-paths" / "src" / "artifact.rs"
)


def _const(name: str) -> str:
    text = ARTIFACT_RS.read_text()
    m = re.search(rf'^pub const {name}: &str = "([^"]+)";', text, re.M)
    if m is None:
        raise SystemExit(
            f"artifact_layout: no `pub const {name}: &str = \"…\";` in {ARTIFACT_RS} "
            "— update this reader with the constant (issue 1852)."
        )
    return m.group(1)


C_GENERATED_DIR = _const("C_GENERATED_DIR")
CPP_GENERATED_DIR = _const("CPP_GENERATED_DIR")
GEN_DIRS = (C_GENERATED_DIR, CPP_GENERATED_DIR)
# Where under an include root each language's generated headers sit (phase-501
# W2): `rclc/detail` for C, `nros` for C++.
C_HEADER_SUBDIR = _const("C_HEADER_SUBDIR")
CPP_HEADER_SUBDIR = _const("CPP_HEADER_SUBDIR")
