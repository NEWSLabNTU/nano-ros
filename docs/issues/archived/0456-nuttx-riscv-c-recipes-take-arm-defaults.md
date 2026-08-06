---
id: 456
title: "Two of the three NuttX riscv recipes never export the riscv env, so the C lane links an arm vector table into a riscv image"
status: resolved
type: bug
area: build
related: [issue-0433, issue-0439, issue-0443, phase-285, phase-339]
---

## Symptom

`just build-test-fixtures lane=tier2` — the whole point of issues 0439 and 0443
— gets seven of eight modules green and dies in `nuttx`:

```
ld: skipping incompatible …/riscv32imac-unknown-nuttx-elf/release/build/
    nros-board-nuttx-qemu-0f7e25cca06190a9/out/libnros_nuttx_boot.a
ld: cannot find -lnros_nuttx_boot: No such file or directory
error: could not compile `nros-nuttx-riscv-ffi` (bin "nros-nuttx-ffi")
error: recipe `build-riscv-c` failed with exit code 101
```

Read the two lines together. The archive is not missing; it is *rejected*, and
the second line then reports the rejection as absence. The archive is the riscv
build's OWN `OUT_DIR` artifact:

```
$ ar t …/riscv32imac-unknown-nuttx-elf/…/out/libnros_nuttx_boot.a
arm_vectortab.o
nuttx_builtins_stub.o
```

A riscv image's boot archive with an **arm** vector table in it.

## Root cause — one env block, written once, needed three times

`run_image_link` (`nuttx_image_link.rs`) defaults every NuttX knob to
**qemu-arm**, by design: `NUTTX_ARCH_INCLUDES`, `NUTTX_LD_SCRIPT`,
`NUTTX_BOARD_LIB_DIR`, and

```rust
let vectortab_rel =
    env::var("NUTTX_VECTORTAB").unwrap_or_else(|_| "arch/arm/src/arm_vectortab.o".to_string());
```

Phase-285 W4 gave riscv the opt-out an arch without a vector-table head object
needs — `NUTTX_VECTORTAB=""` means "there is none" — and `just/nuttx.just`
exports it, along with the other five riscv values, in **`build-riscv-rust`**.

`build-riscv-c` and `build-riscv-c-workspaces` do not. All three provision the
same rv-virt kernel with the same three-line preamble; only one of them then
says what arch it is building for. So the C lanes take the ARM defaults, the
`""` opt-out never happens, and `run_image_link` archives whatever
`arch/arm/src/arm_vectortab.o` resolves to.

## Why it resolves to something rather than failing

`snapshot_or_tree` (phase-339) tries the per-arch export snapshot first and
falls back to the live-tree spelling. The riscv snapshot has no
`startup/arm_vectortab.o` — correctly, `build-nuttx.sh` only copies one when the
build produced one:

```
$ ls third-party/nuttx/nuttx/nros-nuttx-export-riscv/startup/
crt0.o
$ ls third-party/nuttx/nuttx/nros-nuttx-export-arm/startup/
arm_vectortab.o  crt0.o
```

So it falls back to `$NUTTX_DIR/arch/arm/src/arm_vectortab.o` — which exists,
left in the shared in-tree checkout by the last ARM build. The fallback hands
one architecture another architecture's object, and `ar` will happily archive
it: `ar` does not check arch, and neither does anything between the env default
and the link.

Phase-339 did not introduce this — the arm default and the live-tree fallback
both predate it — but it made the ingredients reliably co-present. Before,
whether `arch/arm/src/arm_vectortab.o` survived a riscv reconfigure was luck;
now both arches build in sequence in one lane and the arm object is simply
always there.

## The shape, not the site

Three recipes share a kernel-provisioning preamble and diverge on the six lines
that say which arch it is for. That is the same shape as issue 0439 (two guards
each right alone) and 0443 (one fact spelled as two env vars): a value that must
be the same in N places, kept in sync by whoever remembers. The site fix is six
lines of copy-paste into two recipes; the class fix is that the riscv env has
ONE spelling that every riscv recipe sources.

There is also a missing failure: nothing between "resolve a path" and "archive
it" checks that the object matches the target. A wrong-arch object should be a
loud error naming both arches, not a `cannot find -l…` three steps later that
reads as a missing file.

## Fix

1. One `scripts/nuttx/riscv-env.sh`, sourced by all three riscv recipes.
2. `run_image_link` verifies the vector-table object's machine type against the
   build's target arch before archiving, and fails naming both.

## Scope

`lane=tier2` and any `just nuttx build-riscv-c` / `build-riscv-c-workspaces`
run. `build-riscv-rust` is unaffected (it is the recipe that has the env), which
is why the rust riscv fixtures build clean in the same sweep.

## Resolution (2026-08-06)

**One spelling.** `scripts/nuttx/riscv-env.sh` holds the six arch-describing
values; all three riscv recipes source it. The file says why it exists, so the
next person adding a seventh variable adds it once.

**The wrong arch is now an error, not a silent archive.** `run_image_link`
checks the vector table's ELF `e_machine` against `CARGO_CFG_TARGET_ARCH` before
archiving and fails naming both, pointing at the env. It reads the two bytes at
offset 18 directly rather than shelling out to `readelf`, so it works in a build
script on any host; unreadable or non-ELF input is deliberately NOT an error,
because `ar` and `ld` describe that better than a guess would.

The phase-285 W5 accommodation immediately above it is the same class,
encountered before and absorbed rather than named: it skips the image link when
a CONFIGURED vectortab does not exist, which happened to cover the riscv C lane
while a riscv reconfigure was still wiping the arm object. It tolerates the
wrong arch exactly as long as the file is missing. That is why this surfaced
now — not because phase-339 broke it, but because once both arches build in one
lane the arm object is always present, `exists()` is true, and the tolerance
turns into an incompatible archive.

## Verification

```
$ ar t …/riscv32imac-unknown-nuttx-elf/…/out/libnros_nuttx_boot.a
nuttx_builtins_stub.o          # was: arm_vectortab.o + the stub
$ just nuttx build-fixtures-riscv                       # all three recipes
RC=0
$ just build-test-fixtures lane=tier2
== zephyr == OK  == qemu == OK  == threadx_linux == OK  == nuttx == OK
== esp32 == OK   == freertos == OK  == threadx_riscv64 == OK  == native == OK
RC=0, stamp lane=tier2
$ NROS_FIXTURE_LANE=tier2 just _require-fixtures         # RC=0
$ NROS_FIXTURE_LANE=tier2 just _check-fixtures-stale     # RC=0
check-fixtures-stale: scope=coords (lane:tier2) … (13 coordinate(s))
```

Tier 2 builds and both its gates pass — the acceptance issues 0439 and 0443
were blocking, with this as the third and last obstacle.
