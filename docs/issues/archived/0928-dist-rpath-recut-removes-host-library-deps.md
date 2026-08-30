---
id: 928
title: "Re-cut the qemu and openocd dists with $ORIGIN rpath so their host
  library deps vanish instead of being declared"
status: resolved
type: enhancement
area: tooling
related: [0926, 0368, phase-327, rfc-0062]
---

## Why this exists separately

phase-327 W4 had two halves. The DECLARATION half is done (issue 0926: every
dist's runtime closure measured, declared, reported at the point of use, and
gated by `check-dist-runtime-deps`). This is the other half, split out because
it cannot be done in this repository: the dists are built and released from
**nano-ros-sdk**, so the work is a re-cut plus an index bump, not a code change
here.

Declaring a dependency and removing it are different goods. Declaring makes the
failure legible — `nros setup --tool openocd --check` now says which packages
are missing and how to install them, instead of handing over a binary that dies
on a bare loader error. Removing means the user never needs the package at all,
which is what a prebuilt dist is FOR.

## Candidates, in priority order

**1. `openocd` — `libftdi.so.1`.** The strongest case, and it is not merely
"undeclared": the soname is provided by Ubuntu's `libftdi1` package at version
**0.20**, i.e. libftdi *0.x*. Its successor ships `libftdi1.so.2` under the
confusingly-named `libftdi1-2`, which IS installed on a normal developer host
and does NOT satisfy the dist. So this dist depends on a decade-old library that
a modern host is unlikely to have and that no amount of "install the obvious
package" fixes — `libftdi1-2` being present is exactly what makes the failure
confusing. `libhidapi-hidraw.so.0` rides along.

**2. `arm-none-eabi-gcc` — `libncursesw.so.5` / `libtinfo.so.5`.** Same shape:
ncurses **5**, which 22.04 and later do not ship by default. Only
`arm-none-eabi-gdb` needs them, so the compiler works and the debugger does not
— a partial breakage that reads as "the toolchain installed fine".

**3. `qemu` — `libslirp.so.0` + 19 more.** The original case (phase-327 W4's
`-nros3` note). Lower priority than it looks: these are glib/pixman/png/zstd
class libraries that a desktop host almost always has, which is why the dist
appeared to work for a year with a one-entry declaration.

## Acceptance

* The dist links its private copies through `RUNPATH=$ORIGIN/../lib`, the shape
  the cyclonedds dist already uses successfully (measured in 0926: its own
  `libddsc.so.0` resolves from the dist, and the only reason it looked
  ROS-dependent was the caller's `LD_LIBRARY_PATH` winning over RUNPATH).
* `check-dist-runtime-deps` reports a SMALLER closure for the re-cut dist, and
  the corresponding `system = [..]` entries are deleted in the same commit as
  the index version bump. The gate is what proves the dep is gone rather than
  merely undeclared again.
* Index suffix bumped (`-nros3` for qemu, per W4's note).

## Note on RUNPATH vs LD_LIBRARY_PATH

`$ORIGIN` rpath does not win against a caller's `LD_LIBRARY_PATH` — `RUNPATH` is
searched AFTER it. So a re-cut removes the need for a host package; it does not
protect against a host that puts a conflicting library ahead of it. That is
issue 0774's class and is not what this issue is about.

## Resolution (2026-08-30)

All three candidates addressed, in the ranked order.

**qemu — no build needed.** `11.0.0-nros6` had been RELEASED on 2026-08-29 with
the bundling and the index still pinned `-nros2` from May, so the fixed dist
existed and nano-ros was not using it. Verified on the released tarball before
bumping: 18 libs bundled, external closure **20 -> 2**, `qemu-system-arm
--version` runs with `LD_LIBRARY_PATH` unset. The two that remain are
deliberate — `libselinux` is on the bundler's host-only list and `libpcre2` is
reachable only through it.

**openocd -> 0.12.0-nros2.** External closure **4 -> 0**; the binary that died
with `libftdi.so.1` now runs on the very host that could not start it.

**arm-none-eabi-gcc -> 13.2-nros2.** ncurses 5 bundled on x86_64; gdb no longer
dies at the loader. NOT fully fixed — its embedded Python still fails to
initialise, which is issue 0929 and is not an rpath problem.

**The bundler is now shared** (`scripts/lib/bundle.sh` in nano-ros-sdk). It had
lived inside `build-qemu.sh`, which is the whole reason qemu shipped
self-contained for months while every other dist linked the host. Three defects
surfaced only once it was applied to something other than qemu:

* every `ldd` now runs under `env -u LD_LIBRARY_PATH` — the function COPIES what
  the loader resolved, so a stray path made it bundle ROS's `libddsc` instead of
  the dist's own;
* a library the dist already ships is no longer copied onto itself;
* non-ELF arguments are filtered, so a caller may hand over a whole `bin/`
  (ARM's has wrappers and symlinks among 31 binaries).

**Net on the index: 19 of 0926's 26 prereq keys are gone**, and
`nros setup --system --check` reports **0 missing** on a host that previously
needed four packages installed. Declaring was never the goal — it was how the
problem became visible; re-cutting removed the need.

**The gate got sharper too.** It now measures the PINNED version rather than the
whole tool directory: the store accumulates (issue 0500), and with 13.2-nros1
and -nros2 both present, nros2's bundled ncurses masked nros1's missing one — a
false negative in which a re-cut hides the release it replaced.
