#!/usr/bin/env python3
"""Issue 0570 — drift gate for the vendored NuttX `libc` fork's mirrored types.

`third-party/nuttx/libc` hand-mirrors NuttX's opaque C types as byte blobs
(`pthread_attr_t = [usize; __PTHREAD_ATTR_SIZE__]`, …). NuttX sizes several of
them from **Kconfig**, so there is no single correct constant: `pthread_attr_t`
is 20 bytes with `CONFIG_SCHED_SPORADIC=n` and 56 with `=y` (two `struct
timespec` appended, 16 B each under `CONFIG_SYSTEM_TIME64`), and `CONFIG_SMP`
appends an `affinity` on top of that.

When the mirror is SMALLER than the real struct the corruption is silent and
remote: `pthread_attr_init`/`pthread_attr_destroy` memcpy/memset the kernel's
full `sizeof` into the caller's object, so the overflow lands on whatever the
compiler put after it. Twice now that was a saved return address:

  * #167 — 24-byte kernel `struct pollfd` vs the 8-byte POSIX one; `poll()`
    smashed `sanitize_standard_fds`'s frame. Fixed with `-Wl,--wrap=poll`.
  * #570 — 56-byte `pthread_attr_t` vs a 20-byte mirror; `pthread_attr_destroy`
    zeroed `std::sys::thread::unix::Thread::new`'s saved `ra`/`s0`-`s4` and the
    epilogue returned to address 0.

Both cost a kernel-dump bisect. #167 fixed its struct and left the RULE
unenforced, which is why #570 survived in the same file. This gate is the rule:
measure every mirrored type against the configured NuttX headers and fail when
a mirror is too small. Oversizing is fine — NuttX only ever touches its own
`sizeof`; undersizing corrupts the caller.

Run:  scripts/check-nuttx-libc-struct-sizes.py

Needs a CONFIGURED NuttX tree (`include/nuttx/config.h`, i.e. a kernel that has
been built at least once) and a cross compiler. Without them it cannot measure,
and says so — a skip here is "not checked", never "checked and fine".
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NoReturn

REPO = Path(__file__).resolve().parent.parent
LIBC_MOD = REPO / "third-party/nuttx/libc/src/unix/nuttx/mod.rs"
NUTTX_DIR = Path(os.environ.get("NUTTX_DIR", REPO / "third-party/nuttx/nuttx"))

# Mirrored type -> (C spelling, `mod.rs` size constant, element width in bytes).
#
# The element width is the mirror's array ELEMENT: `[usize; N]` scales with the
# pointer width, `[u32; N]` does not. `usize` is resolved from the probe target
# below, so this stays honest on an ilp32 kernel and on a 64-bit one.
MIRRORS = [
    ("pthread_attr_t", "pthread_attr_t", "__PTHREAD_ATTR_SIZE__", "usize"),
    ("pthread_mutex_t", "pthread_mutex_t", "__PTHREAD_MUTEX_SIZE__", "usize"),
    ("pthread_cond_t", "pthread_cond_t", "__PTHREAD_COND_SIZE__", "usize"),
    ("pthread_condattr_t", "pthread_condattr_t", "__PTHREAD_CONDATTR_SIZE__", "usize"),
    ("pthread_rwlock_t", "pthread_rwlock_t", "__PTHREAD_RWLOCK_SIZE__", "usize"),
    ("sem_t", "sem_t", "__SEM_SIZE__", "usize"),
    ("fd_set", "fd_set", "__FDSET_SIZE__", "u32"),
    ("sigset_t", "sigset_t", "__SIGSET_SIZE__", "u32"),
]

PROBE_HEADERS = """\
#include <nuttx/config.h>
#include <pthread.h>
#include <semaphore.h>
#include <signal.h>
#include <sys/select.h>
"""

# (triple prefix, compiler, flags) — first one whose compiler is on PATH wins.
# Both NuttX QEMU boards are 32-bit, so either measures the same layout; the
# probe records which one ran so a failure names it.
TOOLCHAINS = [
    ("riscv", "riscv-none-elf-gcc", ["-march=rv32imac", "-mabi=ilp32"]),
    ("arm", "arm-none-eabi-gcc", ["-mcpu=cortex-a7", "-mfloat-abi=soft"]),
]


def skip(reason: str) -> NoReturn:
    """A precondition is missing — say what, and do not claim a verdict."""
    print(f"check-nuttx-libc-struct-sizes: NOT CHECKED — {reason}")
    print(
        "  This gate measures the NuttX headers, so it only runs where a kernel "
        "has been configured\n"
        "  (`just nuttx build`). Not a pass: nothing was compared."
    )
    sys.exit(0)


def mirror_constants() -> dict[str, int]:
    """Read the `const __X_SIZE__: usize = N;` block out of the fork."""
    src = LIBC_MOD.read_text()
    found = {}
    for m in re.finditer(r"^const (__[A-Z_]+__): usize = (\d+);", src, re.M):
        found[m.group(1)] = int(m.group(2))
    return found


def probe_sizes(cc: str, flags: list[str]) -> dict[str, int]:
    """`sizeof` every mirrored type, straight from the configured headers.

    Compiled, not parsed: the sizes depend on Kconfig, alignment and the ABI,
    and a regex over `pthread.h` would re-derive all three wrongly. A `char
    probe_<name>[sizeof(T)]` in .bss carries its size in the symbol table.
    """
    body = [PROBE_HEADERS]
    for name, c_type, _, _ in MIRRORS:
        body.append(f"char probe_{name}[sizeof({c_type})];")
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "sizeprobe.c"
        obj = Path(tmp) / "sizeprobe.o"
        src.write_text("\n".join(body) + "\n")
        cmd = [
            cc, *flags, "-c", "-o", str(obj), str(src),
            "-D__NuttX__", "-nostdinc", "-isystem", str(NUTTX_DIR / "include"),
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            print("check-nuttx-libc-struct-sizes: the size probe did not compile:")
            print(proc.stderr.rstrip())
            sys.exit(1)
        nm = shutil.which(cc.replace("-gcc", "-nm")) or "nm"
        out = subprocess.run(
            [nm, "-S", str(obj)], capture_output=True, text=True, check=True
        ).stdout

    sizes = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[3].startswith("probe_"):
            sizes[parts[3][len("probe_"):]] = int(parts[1], 16)
    return sizes


def main() -> int:
    if not LIBC_MOD.exists():
        skip(f"{LIBC_MOD.relative_to(REPO)} is absent (libc submodule not init'd)")
    if not (NUTTX_DIR / "include/nuttx/config.h").exists():
        skip(f"{NUTTX_DIR}/include/nuttx/config.h is absent (kernel never configured)")

    toolchain = next((t for t in TOOLCHAINS if shutil.which(t[1])), None)
    if toolchain is None:
        names = ", ".join(t[1] for t in TOOLCHAINS)
        skip(f"no NuttX cross compiler on PATH (looked for {names}; `source ./activate.sh`?)")
    arch, cc, flags = toolchain

    consts = mirror_constants()
    sizes = probe_sizes(cc, flags)
    # `usize` is the probe target's pointer width; every NuttX board here is
    # 32-bit, but read it rather than assume it.
    usize_bytes = 8 if "64" in subprocess.run(
        [cc, *flags, "-dumpmachine"], capture_output=True, text=True
    ).stdout else 4
    width = {"usize": usize_bytes, "u32": 4}

    failures = []
    print(f"check-nuttx-libc-struct-sizes: measured with {cc} ({arch}), {NUTTX_DIR}")
    for name, _, const, elem in MIRRORS:
        real = sizes.get(name)
        n = consts.get(const)
        if real is None:
            failures.append(f"  {name}: the probe produced no size — headers changed?")
            continue
        if n is None:
            failures.append(f"  {name}: {const} not found in {LIBC_MOD.name}")
            continue
        mirrored = n * width[elem]
        verdict = "ok" if mirrored >= real else "TOO SMALL"
        print(
            f"  {name:<20} nuttx {real:>4} B   mirror {mirrored:>4} B "
            f"({const} = {n})   {verdict}"
        )
        if mirrored < real:
            need = -(-real // width[elem])  # ceil
            failures.append(
                f"  {name}: NuttX says {real} B, the mirror reserves {mirrored} B — "
                f"{real - mirrored} bytes of every init/destroy land past the end of "
                f"the caller's object.\n"
                f"    Fix: {const} >= {need} in {LIBC_MOD.relative_to(REPO)} "
                f"(size the LARGEST Kconfig layout, not this one — oversizing is inert)."
            )

    if failures:
        print("\ncheck-nuttx-libc-struct-sizes: FAILED")
        print("\n".join(failures))
        print(
            "\n  This is the #167 / #570 class: a mirror smaller than the kernel struct "
            "is a\n  silent stack smash at every libc call that writes the whole thing."
        )
        return 1
    print("check-nuttx-libc-struct-sizes: OK — every mirror covers its NuttX struct")
    return 0


if __name__ == "__main__":
    sys.exit(main())
