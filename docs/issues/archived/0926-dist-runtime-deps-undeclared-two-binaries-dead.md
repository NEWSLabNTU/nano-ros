---
id: 926
title: "6 of 10 SDK dists have undeclared runtime system deps — and two shipped
  binaries cannot run on this host at all"
status: resolved
type: bug
area: cli, tooling
related: [0368, phase-327, phase-398, rfc-0062]
---

## Problem

phase-327 W4 shipped `[tool.qemu] system = ["libslirp"]` and left "the ldd audit
of the other dists" as its one open item. Running that audit finds the same
class in five more dists, and two binaries that are dead on this host today:

    $ ~/.nros/sdk/openocd/0.12.0-nros1/bin/openocd --version
    error while loading shared libraries: libftdi.so.1:
      cannot open shared object file: No such file or directory

    $ ldd .../arm-none-eabi-gcc/*/bin/arm-none-eabi-gdb | grep 'not found'
    libncursesw.so.5 => not found
    libtinfo.so.5 => not found

A bare loader error at the point of use is exactly the symptom RFC-0062 was
written to eliminate, and openocd declares no `system = [...]` at all.

## Measured

External runtime deps per dist — sonames resolving OUTSIDE the dist, minus the
base glibc/gcc runtime (libc, libm, libdl, libpthread, librt, libstdc++,
libgcc_s, libutil, libresolv, ld-linux, linux-vdso):

| dist | external sonames | declared? |
| --- | --- | --- |
| `openocd` | `libftdi.so.1`, `libhidapi-hidraw.so.0`, `libudev.so.1`, `libusb-1.0.so.0` | **none** — and the first two are ABSENT here, so the binary is dead |
| `arm-none-eabi-gcc` | `libcrypt.so.1`, `libncursesw.so.5`, `libtinfo.so.5` | **none** — the `.so.5` pair is absent on 22.04+, so `arm-none-eabi-gdb` is dead (gcc itself runs) |
| `qemu` | `libslirp.so.0` + 19 more (glib/gobject/gio/gmodule, pixman, gcrypt, gpg-error, png16, zstd, bz2, blkid, mount, selinux, ffi, pcre/pcre2, ncursesw.so.6, tinfo.so.6, z) | only `libslirp` |
| `cyclonedds` | `libssl.so.3`, `libcrypto.so.3` | **none** |
| `play_launch_parser` | `libpython3.10.so.1.0`, `libexpat.so.1`, `libz.so.1` | **none** — and the CPython **minor version** is pinned into the soname |
| `xrce-agent` | `libssl.so.3`, `libcrypto.so.3` | **none** — OpenSSL 3, so 20.04 (1.1) cannot load it |

Clean: `corrosion`, `espflash`, `riscv-none-elf-gcc`, `zenohd`.

**CORRECTION to the first measurement.** The cyclonedds row originally also
listed `libacl.so.1` and four `libiceoryx_*` libs, "NOT inside the dist". Wrong,
and wrong in an instructive way: the audit ran with ROS's `LD_LIBRARY_PATH`
active, so `libddsc.so.0` resolved to **ROS's** cyclonedds build — which does
link iceoryx — instead of the dist's own copy behind `RUNPATH=$ORIGIN/../lib`.
`objdump -p` says `idlc` needs only `libcycloneddsidl`, `libddsc`, `libc`.

That is issue 0774's class exactly (a binary loading whatever `libzenohc.so` the
loader finds), which makes it a REQUIREMENT on the gate below rather than a
footnote: an audit that inherits the caller's environment measures the caller,
not the dist. Re-run under `env -u LD_LIBRARY_PATH`; only cyclonedds changed.

Reproduce (the `env -u` is load-bearing):

    cd ~/.nros/sdk && for d in */; do d=${d%/}
      find "$d" -type f \( -perm -u+x -o -name '*.so*' \) | while read -r f; do
        file -b "$f" | grep -q ELF || continue
        env -u LD_LIBRARY_PATH ldd "$f" 2>/dev/null | grep -E '=> */|not found'
      done | awk '{print $1}' | sed 's#.*/##' | sort -u
    done

## Why the existing machinery did not catch it

Nothing is wrong with `[prereq.*]` — phase-398 built it and it works. The gap is
that **nothing derives a dist's declaration from the dist**. `system = [...]` is
hand-authored, so it is only as complete as whoever wrote it, and the audit that
would have caught the omission is a sentence in a roadmap doc rather than a
gate. That is the issue-0196 shape: a rule enforced narrower than it is stated.

Two of these are worse than "undeclared". `libpython3.10.so.1.0` and
`libssl.so.3` name a **specific minor/major version** in the soname, so they are
not merely prerequisites — they are host-distro constraints that no `apt install`
satisfies on the wrong release. A declaration can warn about them; only a re-cut
can remove them.

## Fix

1. Declare what the audit found — `system = [...]` on the six dists plus the
   `[prereq.*]` entries for the new keys. Package names differ per provider, so
   the mapping is deliberate work, not a mechanical soname-to-package rewrite.
2. Make the audit a GATE rather than a roadmap sentence: a dist's declared
   `system` set must cover its measured external closure. It needs a provisioned
   store, so it belongs in the tier that has one, not on the fast line.
3. Re-cut the worst offenders with `$ORIGIN` rpath so the dep vanishes — the
   already-planned phase-327 W4 `-nros3` qemu work, which needs the sdk repo,
   and now openocd (libftdi/libhidapi) has the stronger case.

Until (1), `nros setup --tool openocd` reports success and hands the user a
binary that cannot start.

## Resolution (2026-08-30)

**1. Declared.** 26 new `[prereq.*]` keys and `system = [..]` on the six dists,
derived from the measured closure rather than hand-listed. apt names verified
against the archive; dnf/pacman/brew best-effort per this file's convention.
A key covering several sonames lists them in a new `provides = [..]` field
(`libssl3` provides `libssl.so.3` + `libcrypto.so.3`; `libglib2` four), which
exists so the gate needs no soname table of its own — a second mapping beside
the index is the drift that let `["libslirp"]` stand while 19 went undeclared.

    nros setup --system --check
      [MISSING] libftdi1 — runtime dep of the openocd dist(s) (issue 0926)
      [MISSING] libhidapi — ...
      [MISSING] libncursesw5, libtinfo5 — runtime dep of the arm-none-eabi-gcc dist(s)
    Install with:  sudo apt-get install -y libftdi1 libhidapi-hidraw0 libncursesw5 libtinfo5

**2. Reported at the point of use.** `nros setup --tool <x> --check` said `[OK]`
for a dist that is pinned, present and unable to start: the `system` probe
existed but sat in the INSTALL path, behind the already-present short-circuit.
That is issue 0368 F3's own complaint one path over — "nothing consulted it on
the path where the tool is used". It now reports:

    [BROKEN]  tool    openocd 0.12.0-nros1 — installed, but 2 system package(s)
              it needs are missing: libftdi1, libhidapi
              Install with:  sudo apt-get install -y libftdi1 libhidapi-hidraw0

and qemu / cyclonedds / xrce-agent still report `[OK]`, so it discriminates.

**3. Gated.** `check-dist-runtime-deps` re-measures every provisioned dist and
fails when a `system = [..]` does not cover it. Not on the fast line and not in
an affordability tier — it needs a store, which no such lane builds — so it runs
from `just doctor`, beside the check that asks the complementary question. It
SKIPS loudly with no store. Negative controls: dropping `libftdi1` from
openocd's list is caught; dropping qemu's whole list yields 20 findings, which
is the original gap exactly.

## Residue

The `$ORIGIN`-rpath re-cut still stands as phase-327 W4's open item, and now has
a second candidate: openocd's `libftdi.so.1` is provided by `libftdi1`
**0.20** — libftdi 0.x, whose successor ships `libftdi1.so.2` under the
confusingly-named `libftdi1-2`. Depending on a soname whose provider is a decade
old is worth removing rather than declaring. Needs the sdk repo.

Declaring does not INSTALL: openocd and `arm-none-eabi-gdb` remain unusable on
this host until those four packages are installed. What changed is that both are
now said out loud, with the command, instead of surfacing as a loader error.
