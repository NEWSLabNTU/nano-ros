#!/usr/bin/env python3
"""Phase 157.C.16 — stage + build per-package C++ FFI staticlib crates.

cmake's `nros_generate_interfaces(<pkg> ... LANGUAGE CPP)` creates
a sibling Rust crate `nano_ros_cpp_ffi_<pkg>` that:
  * `crate-type = ["staticlib"]`.
  * `#[no_std]` + custom `#[panic_handler]`.
  * `include!()`s every `_ffi.rs` from the package + its deps so all
    types land in a single compilation unit.
  * Compiles to `lib<crate>.a` that the cpp example links via
    `target_link_libraries(<app> PRIVATE nano_ros_cpp_ffi_<pkg>)`.

The NuttX make-build path needs the equivalent: stage the crate at
`<example>/generated/ffi/nano_ros_cpp_ffi_<pkg>/`, build it with
the same nightly + `-Zbuild-std=core` + cross-compile flags
NROS_CARGO_BUILD uses for nros-c, then expose the resulting
`lib<crate>.a` paths via stdout so the staging script can echo
them into a Makefile fragment the example's Makefile includes
(extending EXTRA_LIBS).

Usage:
  gen-cpp-ffi-crates.py <example-dir>

Output to stdout: one absolute path per line, one per built staticlib.
"""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TARGET_TRIPLE = "armv7a-nuttx-eabihf"
TOOLCHAIN = "nightly-2026-04-11"
TEMPLATE_FFI_RS = REPO_ROOT / "cmake" / "ffi_lib_rs.in"

SERDES_DIR = REPO_ROOT / "packages" / "core" / "nros-serdes"
LIBC_DIR = REPO_ROOT / "third-party" / "nuttx" / "libc"
DEFAULT_CODEGEN = (
    REPO_ROOT / "packages" / "codegen" / "packages" / "target" / "release" / "nros"
)
CODEGEN = DEFAULT_CODEGEN

RESOLVED_PKG_LIST_RE = re.compile(r'set\(_NROS_RESOLVED_PACKAGES "([^"]*)"\)')

CARGO_TOML_TEMPLATE = """[workspace]

[package]
name = "{ffi_name}"
version = "0.1.0"
edition = "2024"

[lib]
name = "{ffi_lib_name}"
crate-type = ["staticlib"]

[dependencies]
nros-serdes = {{ path = "{serdes_dir}", default-features = false }}

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
"""

CARGO_CONFIG_TOML = """[build]
target = "{triple}"

[unstable]
build-std = ["core"]

[target.{triple}]
linker = "arm-none-eabi-gcc"
rustflags = [
    "-C", "link-arg=-mcpu=cortex-a7",
    "-C", "link-arg=-mfloat-abi=hard",
    "-C", "link-arg=-mfpu=neon-vfpv4",
]
"""


def resolved_pkg_order(example_dir: Path) -> list[str] | None:
    """Run `nros-codegen resolve-deps` to get topological pkg order.

    Cross-package type references (`action_msgs` ↔
    `unique_identifier_msgs` etc.) require deps' FFI .rs files to
    be `include!()`'d BEFORE the dependent. Filesystem name-sort
    doesn't satisfy that — explicit topological resolution does.
    """
    package_xml = example_dir / "package.xml"
    if not package_xml.exists():
        return None
    with tempfile.NamedTemporaryFile(
        mode="w+", suffix=".cmake", delete=False
    ) as f:
        out_path = Path(f.name)
    try:
        rc = subprocess.run(
            [
                str(CODEGEN),
                "codegen",
                "resolve-deps",
                "--package-xml",
                str(package_xml),
                "--output-cmake",
                str(out_path),
            ],
            capture_output=True,
        )
        if rc.returncode != 0:
            return None
        content = out_path.read_text()
    finally:
        out_path.unlink(missing_ok=True)
    match = RESOLVED_PKG_LIST_RE.search(content)
    if not match:
        return None
    return [p for p in match.group(1).split(";") if p]


def find_packages(example_dir: Path) -> list[Path]:
    """Return `generated/cpp/<pkg>/` dirs in topological order.

    Falls back to filesystem name-sort when resolve-deps fails
    (mismatch warning surfaced; the build may still succeed if the
    name-sort happens to match topological order — true for our
    talker/listener with std_msgs).
    """
    cpp_root = example_dir / "generated" / "cpp"
    if not cpp_root.is_dir():
        return []
    order = resolved_pkg_order(example_dir)
    if order is None:
        return sorted(p for p in cpp_root.iterdir() if p.is_dir())
    result: list[Path] = []
    for pkg in order:
        pkg_dir = cpp_root / pkg
        if pkg_dir.is_dir():
            result.append(pkg_dir)
    return result


def gather_ffi_rs_files(pkg_dirs: list[Path]) -> dict[str, list[Path]]:
    """Per-package list of _ffi.rs files (recursive under msg/srv/action)."""
    by_pkg: dict[str, list[Path]] = {}
    for pkg_dir in pkg_dirs:
        files = sorted(
            f
            for subdir in ("msg", "srv", "action")
            for f in (pkg_dir / subdir).glob("*_ffi.rs")
            if f.is_file()
        )
        by_pkg[pkg_dir.name] = files
    return by_pkg


def stage_crate(
    ffi_root: Path,
    pkg_name: str,
    own_ffi_files: list[Path],
    dep_ffi_files: list[Path],
) -> Path:
    """Render Cargo.toml + lib.rs + .cargo/config.toml for one FFI crate."""
    ffi_name = f"nano-ros-cpp-ffi-{pkg_name}"
    ffi_lib_name = ffi_name.replace("-", "_")
    crate_dir = ffi_root / ffi_name
    src_dir = crate_dir / "src"
    cargo_dir = crate_dir / ".cargo"
    src_dir.mkdir(parents=True, exist_ok=True)
    cargo_dir.mkdir(parents=True, exist_ok=True)

    (crate_dir / "Cargo.toml").write_text(
        CARGO_TOML_TEMPLATE.format(
            ffi_name=ffi_name,
            ffi_lib_name=ffi_lib_name,
            serdes_dir=str(SERDES_DIR),
        )
    )

    includes_block = "\n".join(
        f'include!("{p}");' for p in dep_ffi_files + own_ffi_files
    )
    template = TEMPLATE_FFI_RS.read_text()
    lib_rs = template.replace("@NROS_CPP_FFI_INCLUDES@", includes_block)
    (src_dir / "lib.rs").write_text(lib_rs)

    (cargo_dir / "config.toml").write_text(
        CARGO_CONFIG_TOML.format(triple=TARGET_TRIPLE)
    )
    return crate_dir


def build_crate(crate_dir: Path) -> Path:
    """Run cargo build + return the path to the resulting staticlib."""
    env = {
        **__import__("os").environ,
        "RUSTUP_TOOLCHAIN": TOOLCHAIN,
        "CC_armv7a_nuttx_eabihf": "arm-none-eabi-gcc",
        "AR_armv7a_nuttx_eabihf": "arm-none-eabi-ar",
        "CFLAGS_armv7a_nuttx_eabihf": "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4",
    }
    libc_patch = f'patch.crates-io.libc.path="{LIBC_DIR}"'
    rc = subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--config",
            libc_patch,
        ],
        cwd=crate_dir,
        env=env,
    )
    if rc.returncode != 0:
        raise SystemExit(
            f"cargo build failed for {crate_dir.name} (rc={rc.returncode})"
        )
    pkg_name = crate_dir.name.replace("nano-ros-cpp-ffi-", "")
    lib_name = f"libnano_ros_cpp_ffi_{pkg_name}.a"
    lib_path = crate_dir / "target" / TARGET_TRIPLE / "release" / lib_name
    if not lib_path.exists():
        raise SystemExit(
            f"cargo build reported success but {lib_path} doesn't exist"
        )
    return lib_path


def main(argv: list[str]) -> int:
    global CODEGEN
    if len(argv) < 2:
        print(
            "usage: gen-cpp-ffi-crates.py <example-dir> [nros-codegen-binary]",
            file=sys.stderr,
        )
        return 2
    example_dir = Path(argv[1]).resolve()
    if len(argv) >= 3:
        CODEGEN = Path(argv[2]).resolve()
    if not CODEGEN.exists():
        print(
            f"warning: nros-codegen not at {CODEGEN}; skipping {example_dir.name}",
            file=sys.stderr,
        )
        return 0
    pkg_dirs = find_packages(example_dir)
    if not pkg_dirs:
        # Not a CPP example or codegen hasn't run; nothing to do.
        return 0
    by_pkg = gather_ffi_rs_files(pkg_dirs)
    ffi_root = example_dir / "generated" / "ffi"
    # Walk packages in topological order — pkg_dirs is sorted, which
    # incidentally matches the codegen output order (builtin_interfaces
    # before std_msgs, etc.). Accumulate dep files so each crate
    # include!()s every preceding pkg's FFI .rs files (covers the
    # superset of the transitive closure).
    dep_acc: list[Path] = []
    libs: list[Path] = []
    for pkg_dir in pkg_dirs:
        pkg_name = pkg_dir.name
        own = by_pkg[pkg_name]
        crate_dir = stage_crate(ffi_root, pkg_name, own, dep_acc)
        lib_path = build_crate(crate_dir)
        libs.append(lib_path)
        dep_acc.extend(own)
    for lib in libs:
        print(lib)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
