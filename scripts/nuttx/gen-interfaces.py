#!/usr/bin/env python3
"""Phase 157.C.9 + .C.14 — run nros-codegen for an example's message deps.

CLI equivalent of cmake's `nros_generate_interfaces()` /
`nros_find_interfaces()`. The NuttX make-build path doesn't run
cmake; this script picks the right codegen path based on the
example's CMakeLists.txt + `package.xml`:

  * `nros_generate_interfaces(<pkg> "<file>" ...)` — explicit per-
    package codegen, used by C examples. Parsed via regex.
  * `nros_find_interfaces(LANGUAGE CPP ...)` — auto-discovery from
    `package.xml`, used by C++ examples. Delegated to
    `nros-codegen resolve-deps` which returns the resolved package
    list + interface files.

For CPP examples we run BOTH `--language c` (typesupport sources
that the cpp wrapper headers reference) AND `--language cpp` (the
`<pkg>.hpp` umbrella + per-message C++ wrappers).

The example's wrapper Makefile (Phase 157.A) globs
`generated/c/*/<msg|srv|action>/*.c` into CSRCS,
`generated/cpp/*/<msg|srv|action>/*.cpp` into CXXSRCS, and adds
both `generated/{c,cpp}/<pkg>/` to CFLAGS / CXXFLAGS.

Usage:
  gen-interfaces.py <example-dir> [nros-codegen-binary]
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

GEN_CALL_RE = re.compile(
    r'nros_generate_interfaces\s*\(\s*(\w+)\s+"([^"]+)"',
    re.MULTILINE,
)
FIND_CALL_RE = re.compile(r"nros_find_interfaces\s*\(", re.MULTILINE)
RESOLVED_PKG_LIST_RE = re.compile(r'set\(_NROS_RESOLVED_PACKAGES "([^"]*)"\)')
RESOLVED_PKG_FILES_RE = re.compile(
    r'set\(_NROS_RESOLVED_(\w+)_FILES "([^"]*)"\)'
)
RESOLVED_PKG_DEPS_RE = re.compile(
    r'set\(_NROS_RESOLVED_(\w+)_DEPS "([^"]*)"\)'
)


def resolve_interface(pkg: str, relpath: str) -> Path | None:
    """Mirror cmake's `_nros_resolve_interface` — local → AMENT → bundled."""
    ament = os.environ.get("AMENT_PREFIX_PATH", "")
    for prefix in ament.split(":"):
        if not prefix:
            continue
        candidate = Path(prefix) / "share" / pkg / relpath
        if candidate.exists():
            return candidate
    bundled = REPO_ROOT / "share" / "nano-ros" / "interfaces" / pkg / relpath
    if bundled.exists():
        return bundled
    return None


def run_codegen(
    codegen: Path,
    example_dir: Path,
    pkg: str,
    interface_files: list[Path],
    language: str,
    dependencies: list[str],
) -> int:
    out_dir = example_dir / "generated" / language / pkg
    out_dir.mkdir(parents=True, exist_ok=True)
    args = {
        "package_name": pkg,
        "output_dir": str(out_dir),
        "interface_files": [str(f) for f in interface_files],
        "dependencies": dependencies,
        "ros_edition": "humble",
    }
    args_file = out_dir / "codegen-args.json"
    args_file.write_text(json.dumps(args, indent=2))
    rc = subprocess.run(
        [str(codegen), "codegen", "--args-file", str(args_file), "--language", language],
        check=False,
    ).returncode
    return rc


def parse_resolve_deps(
    codegen: Path, package_xml: Path
) -> dict[str, dict] | None:
    """Run `nros-codegen resolve-deps` + parse the emitted cmake."""
    with tempfile.NamedTemporaryFile(
        mode="w+", suffix=".cmake", delete=False
    ) as f:
        out_path = Path(f.name)
    try:
        rc = subprocess.run(
            [
                str(codegen),
                "codegen",
                "resolve-deps",
                "--package-xml",
                str(package_xml),
                "--output-cmake",
                str(out_path),
            ],
            check=False,
            capture_output=True,
        )
        if rc.returncode != 0:
            print(
                f"warning: resolve-deps failed for {package_xml}: "
                f"{rc.stderr.decode(errors='replace')}",
                file=sys.stderr,
            )
            return None
        content = out_path.read_text()
    finally:
        out_path.unlink(missing_ok=True)

    pkg_list_match = RESOLVED_PKG_LIST_RE.search(content)
    if not pkg_list_match:
        return None
    pkg_list = [p for p in pkg_list_match.group(1).split(";") if p]
    pkgs: dict[str, dict] = {}
    for pkg in pkg_list:
        pkgs[pkg] = {"files": [], "deps": []}
    for match in RESOLVED_PKG_FILES_RE.finditer(content):
        pkg, files_str = match.group(1), match.group(2)
        if pkg in pkgs:
            pkgs[pkg]["files"] = [Path(f) for f in files_str.split(";") if f]
    for match in RESOLVED_PKG_DEPS_RE.finditer(content):
        pkg, deps_str = match.group(1), match.group(2)
        if pkg in pkgs:
            pkgs[pkg]["deps"] = [d for d in deps_str.split(";") if d]
    return pkgs


def codegen_c_examples(
    codegen: Path, example_dir: Path, cmake_text: str
) -> int:
    """Phase 157.C.9 — explicit per-call codegen for C examples."""
    by_pkg: dict[str, list[Path]] = {}
    for pkg, relpath in GEN_CALL_RE.findall(cmake_text):
        resolved = resolve_interface(pkg, relpath)
        if resolved is None:
            print(
                f"warning: {example_dir.name}: cannot resolve {pkg}/{relpath} "
                "(no AMENT_PREFIX_PATH match + no bundled file); skipping codegen "
                "for this example. Source ROS env "
                "(`source /opt/ros/humble/setup.bash`) to unblock.",
                file=sys.stderr,
            )
            return 0
        by_pkg.setdefault(pkg, []).append(resolved)
    for pkg, files in by_pkg.items():
        rc = run_codegen(codegen, example_dir, pkg, files, "c", [])
        if rc != 0:
            print(
                f"error: nros-codegen --language c failed for {pkg} in "
                f"{example_dir.name} (rc={rc})",
                file=sys.stderr,
            )
            return rc
    return 0


def codegen_cpp_example(codegen: Path, example_dir: Path) -> int:
    """Phase 157.C.14 — package.xml-driven codegen for C++ examples.

    CPP wrapper headers (`<pkg>.hpp`) reference C-side typesupport
    structs, so we run both `--language c` and `--language cpp`
    against every resolved package. Order matters: package deps
    must be codegen'd before their dependents (resolve-deps
    returns topo-order).
    """
    package_xml = example_dir / "package.xml"
    if not package_xml.exists():
        print(
            f"warning: {example_dir.name}: package.xml missing for CPP "
            "codegen; skipping",
            file=sys.stderr,
        )
        return 0
    pkgs = parse_resolve_deps(codegen, package_xml)
    if not pkgs:
        print(
            f"warning: {example_dir.name}: resolve-deps returned no "
            "packages; skipping codegen",
            file=sys.stderr,
        )
        return 0
    for pkg, meta in pkgs.items():
        for language in ("c", "cpp"):
            rc = run_codegen(
                codegen,
                example_dir,
                pkg,
                meta["files"],
                language,
                meta["deps"],
            )
            if rc != 0:
                print(
                    f"error: nros-codegen --language {language} failed for "
                    f"{pkg} in {example_dir.name} (rc={rc})",
                    file=sys.stderr,
                )
                return rc
    return 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(
            "usage: gen-interfaces.py <example-dir> [nros-codegen-binary]",
            file=sys.stderr,
        )
        return 2
    example_dir = Path(argv[1]).resolve()
    codegen = (
        Path(argv[2])
        if len(argv) >= 3
        else (REPO_ROOT / "packages/codegen/packages/target/release/nros")
    )
    if not codegen.exists():
        print(
            f"warning: nros-codegen not at {codegen}; skipping {example_dir.name}",
            file=sys.stderr,
        )
        return 0

    cmake_file = example_dir / "CMakeLists.txt"
    if not cmake_file.exists():
        print(f"warning: {cmake_file} missing; skipping", file=sys.stderr)
        return 0

    cmake_text = cmake_file.read_text()
    if FIND_CALL_RE.search(cmake_text):
        return codegen_cpp_example(codegen, example_dir)
    return codegen_c_examples(codegen, example_dir, cmake_text)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
