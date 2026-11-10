#!/usr/bin/env python3
"""nano-ros's uORB backend compiles the way PX4 compiles a NuttX module -- issue 2419.

Issue 2403's fix reached main with three files that did not compile for PX4
NuttX: PX4 builds NuttX C++ `-nostdinc++`, so `<atomic>` did not exist for
`subscriber.cpp` / `session.cpp`, and `-Wcast-align -Werror` refused the glue's
slot cast. No merge-gating lane compiled `packages/rmw/uorb` for a PX4 target
(`check rmw-uorb` is a host build of the mock-ABI half; the glue is compiled
only inside a real `px4_add_module()`), so a manual Renode harness found it.

This compiles every TU `nros_px4_add_module(BACKENDS uorb)` compiles -- the
backend list from `NrosRmwUorbSources.cmake`, glue included, with the module's
own `-D`s and include dirs -- using PX4's compile options for a NuttX module
against PX4's headers. CI has no PX4 tree, so both come from a COMMITTED
snapshot generated from the PINNED PX4 (`packages/testing/
nros-px4-nuttx-compile/`, `scripts/gen-px4-nuttx-compile-snapshot.py`); this
gate also holds that snapshot to the pin and to its recorded hashes.

The compiler is `arm-none-eabi-g++`: `$NROS_ARM_NONE_EABI_PREFIX-g++` if set,
else the one on PATH -- the SDK store's pinned 13.2 when `activate.sh` found
it, else the system package (the CI image's, which is also what PX4 v1.17's
own `Tools/setup/ubuntu.sh` installs). It is printed, never guessed.

Before trusting a green, the gate runs its negative controls on the same
command line: `#include <atomic>` must fail as missing, an
`reinterpret_cast` from `alignas` byte storage must fail `-Wcast-align`, and
the `void *` spelling of that cast must pass.

Usage:
    scripts/check/check-px4-uorb-nuttx-compile.py
"""

from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import check_skip  # noqa: E402
import px4_nuttx_compile as pnc  # noqa: E402

NAME = "px4-uorb-nuttx-compile"

# Each control: (name, source, must_pass, text the failure must contain).
CONTROLS = (
    ("std header present", "#include <cstdint>\n#include <uORB/uORB.h>\n"
     "uint32_t nros_probe_ok(void) { return 0; }\n", True, None),
    ("<atomic> absent under -nostdinc++", "#include <atomic>\n", False, "atomic"),
    ("reinterpret_cast from byte storage refused", (
        "struct T { long long v; };\n"
        "struct S { alignas(T) unsigned char storage[sizeof(T)];\n"
        "  T *get() { return reinterpret_cast<T *>(storage); } };\n"
        "S g_s;\nT *nros_probe(void) { return g_s.get(); }\n"), False, "cast-align"),
    ("the void * spelling of that cast accepted", (
        "struct T { long long v; };\n"
        "struct S { alignas(T) unsigned char storage[sizeof(T)];\n"
        "  T *get() { return static_cast<T *>(static_cast<void *>(storage)); } };\n"
        "S g_s;\nT *nros_probe(void) { return g_s.get(); }\n"), True, None),
)


def resolve_cxx() -> tuple[str | None, str]:
    prefix = os.environ.get("NROS_ARM_NONE_EABI_PREFIX")
    if prefix:
        return prefix + "-g++", "NROS_ARM_NONE_EABI_PREFIX"
    found = shutil.which("arm-none-eabi-g++")
    return found, "PATH"


def snapshot_problems(manifest: dict, tar_sha: str, members: int) -> list[str]:
    out = []
    pin = pnc.pinned_px4_ref()
    if manifest["px4"]["ref"] != pin:
        out.append(f"snapshot is PX4 {manifest['px4']['ref']}, the pin is {pin} -- regenerate "
                   "with scripts/gen-px4-nuttx-compile-snapshot.py")
    recorded = manifest.get("licenses", {})
    for name in pnc.LICENSES:
        path = pnc.SNAPSHOT_DIR / name
        if name not in recorded:
            out.append(f"{name}: not recorded in snapshot.toml [licenses] -- regenerate")
        elif not path.is_file():
            out.append(f"{name}: missing -- the archived headers ship under it; regenerate")
        elif pnc.sha256(path) != recorded[name]:
            out.append(f"{name}: differs from snapshot.toml -- copied from the pinned tree, "
                       "never edited")
    if tar_sha != manifest["headers"]["sha256"] or members != manifest["headers"]["members"]:
        out.append(f"headers.tar.xz ({tar_sha}, {members} members) is not the archive "
                   "snapshot.toml records -- the snapshot is generated, never edited")
    return out


def compile_one(cxx: str, manifest: dict, src: Path, tmp: Path,
                bases: dict[str, Path]) -> tuple[Path, int, str]:
    obj = tmp / (src.name + ".o")
    argv = pnc.compile_argv(cxx, manifest, bases, src, ["-c", "-o", str(obj)])
    r = subprocess.run(argv, capture_output=True, text=True)
    return src, r.returncode, r.stderr if r.returncode == 0 else f"{shlex.join(argv)}\n{r.stderr}"


def self_test(cxx: str, manifest: dict, tar_sha: str, members: int, tmp: Path,
              bases: dict[str, Path], pool: ThreadPoolExecutor) -> list[str]:
    """The negative controls, on the normal path: each failure means the gate
    cannot tell a good tree from a bad one."""
    out = []
    jobs = []
    for i, (name, text, must_pass, needle) in enumerate(CONTROLS):
        src = tmp / f"control_{i}.cpp"
        src.write_text(text)
        jobs.append((name, src, must_pass, needle))
    ctl = list(pool.map(lambda j: compile_one(cxx, manifest, j[1], tmp, bases), jobs))
    for (name, _, must_pass, needle), (_, rc, err) in zip(jobs, ctl):
        ok = (rc == 0) if must_pass else (rc != 0 and needle in err)
        if not ok:
            out.append(f"negative control broken -- {name}: rc={rc}, the check cannot "
                       f"tell good from bad\n{err}")
    # The staleness half: a moved pin and an edited archive are both caught.
    moved = dict(manifest, px4=dict(manifest["px4"], ref="0" * 40))
    if not any("the pin is" in p for p in snapshot_problems(moved, tar_sha, members)):
        out.append("negative control broken -- a snapshot at another PX4 ref passed")
    if not any("never edited" in p for p in snapshot_problems(manifest, "0" * 64, members)):
        out.append("negative control broken -- an archive with a wrong hash passed")
    unlicensed = dict(manifest, licenses={})
    if not any("[licenses]" in p for p in snapshot_problems(unlicensed, tar_sha, members)):
        out.append("negative control broken -- a snapshot with no licence record passed")
    return out


def main() -> int:
    manifest = pnc.load_manifest()
    headers = pnc.read_headers_tar()
    tar_sha = pnc.sha256(pnc.HEADERS_TAR)
    problems = snapshot_problems(manifest, tar_sha, len(headers))
    if problems:
        print(f"check-{NAME}: the PX4 snapshot does not match its pin or its record:",
              file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1

    cxx, origin = resolve_cxx()
    if cxx is None:
        return check_skip.unverified(
            NAME, "arm-none-eabi-g++ not found (nros setup --tool arm-none-eabi-gcc, "
                  "or set NROS_ARM_NONE_EABI_PREFIX)")
    ver = subprocess.run([cxx, "--version"], capture_output=True, text=True)
    if ver.returncode != 0:
        print(f"check-{NAME}: {cxx} --version failed:\n{ver.stderr}", file=sys.stderr)
        return 1
    print(f"  compiler: {ver.stdout.splitlines()[0]} via {origin} - {cxx}")
    print(f"  PX4 {manifest['px4']['version']} {manifest['px4']['config']}: "
          f"{manifest['flags']['derived_from']}")

    failed = []
    with tempfile.TemporaryDirectory(prefix="px4-uorb-nuttx-") as tmpdir:
        tmp = Path(tmpdir)
        for rel, data in headers.items():
            dst = tmp / "headers" / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(data)
        bases = {name: tmp / "headers" / name for name in pnc.BASES}
        with ThreadPoolExecutor(max_workers=min(8, os.cpu_count() or 1)) as pool:
            failed += self_test(cxx, manifest, tar_sha, len(headers), tmp, bases, pool)
            sources = pnc.backend_sources()
            res = list(pool.map(lambda s: compile_one(cxx, manifest, s, tmp, bases), sources))
        for src, rc, err in res:
            rel = src.relative_to(pnc.REPO)
            if rc != 0:
                failed.append(f"{rel} does not compile for PX4 NuttX:\n{err}")
            else:
                print(f"  ok  {rel}")

    if failed:
        for f in failed:
            print(f"check-{NAME}: {f}", file=sys.stderr)
        print(f"check-{NAME}: a header the snapshot lacks but NuttX has means the snapshot "
              "needs regenerating (scripts/gen-px4-nuttx-compile-snapshot.py); one NuttX "
              "lacks is the bug this gate exists for (issue 2403).", file=sys.stderr)
        return 1
    print(f"check-{NAME}: {len(res)} TUs compile with PX4's NuttX flags; "
          f"{len(CONTROLS)} controls hold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
