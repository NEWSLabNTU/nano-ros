#!/usr/bin/env python3
"""Phase 157.C.9 — run nros-codegen for an example's message deps.

CLI equivalent of cmake's `nros_generate_interfaces(<pkg> <file>
LANGUAGE C SKIP_INSTALL)`. The NuttX make-build path doesn't run
cmake; this script parses the example's `CMakeLists.txt` for the
codegen calls, resolves each interface file via `AMENT_PREFIX_PATH`
or the bundled `share/nano-ros/interfaces/` tree, and invokes
`nros-codegen --args-file <json>` to produce the C sources +
headers under `<example>/generated/c/`.

The example's wrapper Makefile (Phase 157.A) globs
`generated/c/*.c` into `CSRCS` and adds `generated/c/` to
`CFLAGS` so user `main.c` can `#include "std_msgs.h"` /
`#include "example_interfaces.h"`.

Usage:
  gen-interfaces.py <example-dir> [nros-codegen-binary]
      example-dir: e.g. examples/qemu-arm-nuttx/c/zenoh/talker
      nros-codegen-binary: defaults to packages/codegen/packages/target/release/nros-codegen
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

CODEGEN_CALL_RE = re.compile(
    r'nros_generate_interfaces\s*\(\s*(\w+)\s+"([^"]+)"',
    re.MULTILINE,
)


def resolve_interface(pkg: str, relpath: str) -> Path | None:
    """Mirror cmake's `_nros_resolve_interface` — local → AMENT → bundled."""
    # Local file inside the example dir is uncommon for cross-package
    # interfaces; skip it. (Cmake also checks but the std_msgs /
    # example_interfaces examples never carry local copies.)

    # AMENT_PREFIX_PATH
    ament = os.environ.get("AMENT_PREFIX_PATH", "")
    for prefix in ament.split(":"):
        if not prefix:
            continue
        candidate = Path(prefix) / "share" / pkg / relpath
        if candidate.exists():
            return candidate

    # Bundled
    bundled = REPO_ROOT / "share" / "nano-ros" / "interfaces" / pkg / relpath
    if bundled.exists():
        return bundled

    return None


def run_codegen(
    codegen: Path,
    example_dir: Path,
    pkg: str,
    interface_files: list[Path],
) -> int:
    out_dir = example_dir / "generated" / "c" / pkg
    out_dir.mkdir(parents=True, exist_ok=True)
    args = {
        "package_name": pkg,
        "output_dir": str(out_dir),
        "interface_files": [str(f) for f in interface_files],
        "dependencies": [],
        "ros_edition": "humble",
    }
    args_file = out_dir / "codegen-args.json"
    args_file.write_text(json.dumps(args, indent=2))
    rc = subprocess.run(
        [str(codegen), "--args-file", str(args_file), "--language", "c"],
        check=False,
    ).returncode
    return rc


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: gen-interfaces.py <example-dir> [nros-codegen-binary]", file=sys.stderr)
        return 2
    example_dir = Path(argv[1]).resolve()
    codegen = Path(argv[2]) if len(argv) >= 3 else (
        REPO_ROOT / "packages/codegen/packages/target/release/nros-codegen"
    )
    if not codegen.exists():
        print(f"warning: nros-codegen not at {codegen}; skipping {example_dir.name}",
              file=sys.stderr)
        return 0

    cmake_file = example_dir / "CMakeLists.txt"
    if not cmake_file.exists():
        print(f"warning: {cmake_file} missing; skipping", file=sys.stderr)
        return 0

    # Group calls by package; one nros-codegen invocation per package.
    by_pkg: dict[str, list[Path]] = {}
    for pkg, relpath in CODEGEN_CALL_RE.findall(cmake_file.read_text()):
        resolved = resolve_interface(pkg, relpath)
        if resolved is None:
            print(
                f"warning: {example_dir.name}: cannot resolve {pkg}/{relpath} "
                "(no AMENT_PREFIX_PATH match + no bundled file); skipping codegen "
                "for this example. Source ROS env (`source /opt/ros/humble/setup.bash`) "
                "to unblock.",
                file=sys.stderr,
            )
            return 0
        by_pkg.setdefault(pkg, []).append(resolved)

    for pkg, files in by_pkg.items():
        rc = run_codegen(codegen, example_dir, pkg, files)
        if rc != 0:
            print(f"error: nros-codegen failed for {pkg} in {example_dir.name} (rc={rc})",
                  file=sys.stderr)
            return rc

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
