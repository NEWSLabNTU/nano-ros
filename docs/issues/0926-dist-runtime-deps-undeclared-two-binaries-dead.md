---
id: 926
title: "6 of 10 SDK dists have undeclared runtime system deps — and two shipped
  binaries cannot run on this host at all"
status: open
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
| `cyclonedds` | `libssl.so.3`, `libcrypto.so.3`, `libacl.so.1`, `libiceoryx_{binding_c,hoofs,platform,posh}.so` | **none** — iceoryx is NOT inside the dist (checked) |
| `play_launch_parser` | `libpython3.10.so.1.0`, `libexpat.so.1`, `libz.so.1` | **none** — and the CPython **minor version** is pinned into the soname |
| `xrce-agent` | `libssl.so.3`, `libcrypto.so.3` | **none** — OpenSSL 3, so 20.04 (1.1) cannot load it |

Clean: `corrosion`, `espflash`, `riscv-none-elf-gcc`, `zenohd`.

Reproduce:

    cd ~/.nros/sdk && for d in */; do d=${d%/}
      find "$d" -type f \( -perm -u+x -o -name '*.so*' \) | while read -r f; do
        file -b "$f" | grep -q ELF || continue
        ldd "$f" 2>/dev/null | grep -E '=> */|not found'
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
