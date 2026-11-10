"""What compiling nano-ros's uORB backend for a PX4 NuttX target means (issue 2419).

Shared by the generator (`scripts/gen-px4-nuttx-compile-snapshot.py`, which
reads a configured PX4 tree) and the gate (`scripts/check/
check-px4-uorb-nuttx-compile.py`, which reads the committed snapshot), so the
two build their compiler command lines the SAME way and only the header roots
differ: a real PX4 tree + build dir for the generator, the snapshot's copies
for the gate.

Two halves, each with one source:

  * PX4's half -- the compile options and include roots of a stock
    `px4_add_module()` C++ translation unit, recorded in `snapshot.toml` by the
    generator from PX4's own `compile_commands.json`. Never hand-written.
  * nano-ros's half -- the backend's translation units and what
    `nros_px4_add_module(BACKENDS uorb)` adds to them. Read from
    `NrosRmwUorbSources.cmake` (the ONE source list, issue 1437) and from
    `integrations/px4/NanoRosPx4Module.cmake`, never restated here.
"""

from __future__ import annotations

import functools
import hashlib
import io
import lzma
import tarfile
import re
import subprocess
import tempfile
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # pragma: no cover - 3.10 hosts
    import tomli as tomllib  # type: ignore[no-redef]

REPO = Path(__file__).resolve().parents[2]
SNAPSHOT_DIR = REPO / "packages" / "testing" / "nros-px4-nuttx-compile"
MANIFEST = SNAPSHOT_DIR / "snapshot.toml"
# One archive, not a header tree: 173 verbatim PX4 / NuttX headers in the
# working tree are text every repo-wide C scanner then reads as ours (weak
# symbols, condvar waits, array pools, comment names), and clang-format would
# reflow them. snapshot.toml records the archive's sha256.
HEADERS_TAR = SNAPSHOT_DIR / "headers.tar.xz"
# The licences the archived headers ship under, copied beside it from the
# pinned trees: snapshot name -> (base, path in that tree).
LICENSES = {
    "LICENSE-PX4": ("px4", "LICENSE"),
    "LICENSE-NuttX": ("px4", "platforms/nuttx/NuttX/nuttx/LICENSE"),
    "NOTICE-NuttX": ("px4", "platforms/nuttx/NuttX/nuttx/NOTICE"),
}
SDK_INDEX = REPO / "nros-sdk-index.toml"
UORB_SOURCES_CMAKE = (REPO / "packages" / "rmw" / "uorb" / "nros-rmw-uorb" / "cmake"
                      / "NrosRmwUorbSources.cmake")
PX4_MODULE_CMAKE = REPO / "integrations" / "px4" / "NanoRosPx4Module.cmake"

# The two trees a snapshot root can name. A root is stored as `<base>/<rel>`.
BASES = ("px4", "build")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_headers_tar(files: dict[str, bytes]) -> None:
    """A byte-reproducible archive: sorted members, zeroed owner and time."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.PAX_FORMAT) as tar:
        for rel in sorted(files):
            info = tarfile.TarInfo(rel)
            info.size = len(files[rel])
            info.mode = 0o644
            info.mtime = 0
            tar.addfile(info, io.BytesIO(files[rel]))
    HEADERS_TAR.write_bytes(lzma.compress(buf.getvalue(), preset=9))


def read_headers_tar() -> dict[str, bytes]:
    out = {}
    with tarfile.open(HEADERS_TAR, mode="r:xz") as tar:
        for m in tar.getmembers():
            if not m.isfile() or m.name.startswith("/") or ".." in m.name.split("/"):
                raise SystemExit(f"{HEADERS_TAR}: unexpected member {m.name!r}")
            out[m.name] = tar.extractfile(m).read()
    return out


def pinned_px4_ref() -> str:
    """`[source.px4-autopilot] ref` -- the PX4 the snapshot must be read from."""
    with SDK_INDEX.open("rb") as fh:
        index = tomllib.load(fh)
    return index["source"]["px4-autopilot"]["ref"]


def load_manifest() -> dict:
    with MANIFEST.open("rb") as fh:
        return tomllib.load(fh)


def backend_sources() -> list[Path]:
    """The TUs `nros_px4_add_module(BACKENDS uorb)` compiles, from cmake itself.

    `NROS_RMW_UORB_SOURCES` plus `NROS_RMW_UORB_PX4_GLUE_SOURCES` -- exactly the
    two lists NanoRosPx4Module.cmake appends -- evaluated by `cmake -P`, so a
    file added to the list is compiled here with no edit to this script.
    """
    with tempfile.TemporaryDirectory() as tmp:
        script = Path(tmp) / "list.cmake"
        script.write_text(
            f'include("{UORB_SOURCES_CMAKE}")\n'
            'foreach(_s IN LISTS NROS_RMW_UORB_SOURCES NROS_RMW_UORB_PX4_GLUE_SOURCES)\n'
            '  message("${_s}")\n'
            'endforeach()\n')
        r = subprocess.run(["cmake", "-P", str(script)], capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"cmake -P over {UORB_SOURCES_CMAKE} failed:\n{r.stderr}")
    srcs = [Path(line) for line in r.stderr.splitlines() if line.strip()]
    if not srcs:
        raise SystemExit(f"{UORB_SOURCES_CMAKE} listed no sources")
    return srcs


def _cmake_words(text: str) -> list[str]:
    return [w.strip('"') for w in text.split()]


@functools.lru_cache(maxsize=None)
def module_flags() -> tuple[tuple[str, ...], tuple[Path, ...]]:
    """What `nros_px4_add_module(BACKENDS uorb)` adds: (-D flags, include dirs).

    Harvested from NanoRosPx4Module.cmake: the uORB arm's `COMPILE_FLAGS` and
    `INCLUDES`, plus the in-repo `_NROS_PX4_INCLUDES` (the per-build generated
    nros-c/nros-cpp dirs it prepends are not reached by this backend, which
    includes no nros-c/nros-cpp header).
    """
    text = PX4_MODULE_CMAKE.read_text()
    arm = re.search(r'if\(_b STREQUAL "uorb"\)(.*?)\n\s*elseif\(', text, re.S)
    if arm is None:
        raise SystemExit(f"{PX4_MODULE_CMAKE}: no `if(_b STREQUAL \"uorb\")` arm")
    body = arm.group(1)
    uorb = re.search(r'set\(_uorb\s+"\$\{NROS_REPO_DIR\}/([^"]+)"\)', body)
    flags = re.search(r'^\s*COMPILE_FLAGS\s+(.+)$', body, re.M)
    incs = re.search(r'^\s*INCLUDES\s+(.+)$', body, re.M)
    if not (uorb and flags and incs):
        raise SystemExit(f"{PX4_MODULE_CMAKE}: the uORB arm no longer has the "
                         "set(_uorb ...) / INCLUDES / COMPILE_FLAGS lines this reads")
    uorb_dir = REPO / uorb.group(1)
    defines = _cmake_words(flags.group(1))
    dirs = [Path(w.replace("${_uorb}", str(uorb_dir))) for w in _cmake_words(incs.group(1))]
    block = re.search(r'set\(_NROS_PX4_INCLUDES\n(.*?)\)', text, re.S)
    if block is None:
        raise SystemExit(f"{PX4_MODULE_CMAKE}: no `set(_NROS_PX4_INCLUDES` block")
    for rel in re.findall(r'"\$\{NROS_REPO_DIR\}/([^"]+)"', block.group(1)):
        dirs.append(REPO / rel)
    for d in dirs:
        if not d.is_dir():
            raise SystemExit(f"{PX4_MODULE_CMAKE} names an include dir that does not exist: {d}")
    return tuple(defines), tuple(dirs)


def compile_argv(cxx: str, manifest: dict, bases: dict[str, Path], src: Path,
                 extra: list[str]) -> list[str]:
    """One TU's command line: PX4's options and roots, then nano-ros's."""
    argv = [cxx, *manifest["flags"]["cxx"]]
    for root in manifest["flags"]["include"]:
        base, _, rel = root["dir"].partition("/")
        argv += [root["kind"], str(bases[base] / rel)]
    defines, dirs = module_flags()
    argv += defines
    for d in dirs:
        argv += ["-I", str(d)]
    return argv + extra + [str(src)]
