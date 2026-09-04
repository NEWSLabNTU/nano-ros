---
id: 1034
title: "The provisioned QEMU 11 spends ~19.6 s materialising a NuttX image's
  `.bss` before the guest runs, and that stall is the whole C-vs-C++ asymmetry
  in issue 0870"
status: open
type: bug
area: testing, tooling
severity: high
found: 2026-09-04
related: [issue-0870, issue-0930, phase-414]
---

## Symptom

Every NuttX arm C/C++ cell pays ~19.6 s of dead time per image, before the
guest prints its first byte. It is invisible as a stall because it looks like
a slow boot, and it is charged to the cell's wall clock.

MEASURED, on an idle host, same image, two emulators:

| | Debian `qemu-system-arm` 6.2.0 | `~/.nros/sdk/qemu/11.0.0-nros2` |
| --- | ---: | ---: |
| `cpp/action-server` first console byte | 0.05 s | **19.60 s** |
| `c/action-server` | 0.05 s | **19.89 s** |
| `c/action-client` | 0.05 s | 0.05 s |

The harness uses the second one: `qemu_system_arm_path()` prefers the `nros
setup` store over `PATH`, so a developer timing an image by hand with
`qemu-system-arm` measures a different emulator than the test does. That is how
this went unnoticed — the hand-run and the cell disagree by 400x and neither
says which binary it used.

During the stall QEMU burns a full core (`utime+stime` tracks wall clock 1:1
across the whole 19.6 s), so it is emulation/device work, not a wait.

## What it explains: issue 0870's C-vs-C++ asymmetry, quantitatively

Issue 0870 recorded "C: 26.4-27.2 s, C++: 44-50 s -- a measurement, not a
mechanism", and treated the ~20 s gap as evidence about the C++ binding. It is
not about the binding at all. Instrumented cell timings, this tree:

    nuttx C   action: server banner 20.14 s | client collect  3.51 s | cell 26.1 s
    nuttx C++ action: server banner 19.94 s | client collect 23.10 s | cell 45.7 s

Both servers stall. The C **client** does not; the C++ client does. So the C
cell pays the stall once and the C++ cell pays it twice, and the difference is
one stall: **20 s, matching the reported 18-23 s gap.** With the stall removed
the round trips are the same length -- hand-run on QEMU 6.2.0, C++ completes
goal -> accept -> feedback -> result in **2.2 s**.

`nros-tests` charges this to the cell either way, which is why the asymmetry
looked like a property of the C++ image.

## The discriminator is the ELF program headers, and the correlation is exact

19 of 19 NuttX arm images, both emulators:

| shape | images | QEMU 6.2.0 | QEMU 11.0.0-nros2 |
| --- | --- | ---: | ---: |
| 2 `PT_LOAD`, second has `memsz` >> `filesz` (`.bss` folded in with `.data`) | 10 | 0.05 s | **19.5-19.9 s** |
| 3+ `PT_LOAD`, `.bss` in its own `filesz == 0` segment | 9 | 0.05 s | 0.05 s |

    c/action-server    2 LOADs   0x00db0/0x97000   -> 19.89 s
    cpp/talker         2 LOADs   0x00ed0/0x95000   -> 19.65 s
    c/action-client    3 LOADs   0x00db8/0x00db8   ->  0.05 s
    rust:*/talker      3 LOADs   0x00d88/0x00d88   ->  0.05 s

Every Rust NuttX image is in the fast set (they link with `.bss` split out), and
so are exactly two C ones -- `c/action-client` and `c/service-client`. Nothing
else about those two is special: `c/action-client` and `c/action-server` differ
by **16 bytes of `.bss` and 8 bytes of `.data`**, and one boots in 0.05 s while
the other takes 19.9 s.

**And the cost is linear in the zero-fill size**, which is the second
independent check:

| merged segment `memsz` | stall |
| ---: | ---: |
| 0x97000 (618 496) | 19.89 s |
| 0x96000 (614 400) | 19.64 / 19.69 / 19.74 s |
| 0x95000 (610 304) | 19.50-19.65 s (mean 19.56) |

618496/610304 = 1.0134 against 19.89/19.56 = 1.0169. The implied rate is
**~31 KB/s**.

## Hypothesis for the mechanism, and how to settle it

NuttX's `virt` images are linked to run from the machine's FLASH aperture
(`0x0-0x08000000`): the text segment sits at paddr `0x00600000` and the data
segment at paddr `0x00688000`, with `.bss` folded into it in the slow images.
QEMU's `-kernel` ELF loader zero-pads a `PT_LOAD` out to `memsz` and writes the
whole thing at the segment's **paddr** -- so on the slow images it pushes ~600 KB
of zeros into the pflash device, and ~31 KB/s is the shape of a device write
path that invalidates translated code per store. A `filesz == 0` segment gives
the loader nothing to load, which is why the split-`.bss` images escape entirely.

That last step is INFERRED: `third-party/qemu/qemu` is not initialised in this
checkout, so `hw/core/loader.c` was not read. Settle it by initialising the
submodule and checking how `load_elf_ram_sym` treats `p_filesz == 0` and where it
routes the padded write; or by comparing against a stock upstream QEMU 11, which
would also answer whether this is a regression from 6.2.0 or something our patch
line introduced.

## Why it is worth fixing rather than tolerating

* It costs ~20 s per affected image. 10 of the 12 NuttX arm C/C++ fixtures are
  affected, and a full NuttX arm C/C++ e2e sweep boots twelve of them: **~3.3
  minutes of pure emulator overhead per sweep**.
* It makes hand-runs and cells disagree by 400x on the same image, which is a
  trap for exactly the timing-sensitive investigations that keep landing on
  NuttX (0870, 0891, 0902).
* Issue 0870 says its fault "is timing-sensitive and moves with image CONTENT".
  This is a mechanism by which image content moves timing by 20 s -- 16 bytes of
  `.bss` decide it. That does not prove it is 0870's cause and is not offered as
  one; it does mean any 0870 experiment that compares two builds is comparing two
  different emulation regimes unless the segment shape is checked.

## Directions

1. **Split `.bss` into its own `PT_LOAD` in the NuttX arm link.** Nine images
   already have this shape, so it is a linker-script/`--no-rosegment`-class
   change, not a new capability, and it removes the cost outright.
2. **Fix or drop the provisioned QEMU for this board.** 6.2.0 does not have the
   problem. Note the store QEMU is patched for other boards (mps2/an536, lan9118)
   so it cannot simply be discarded -- and issue 0930 already records that
   nothing checks the built QEMU against the pin.
3. **Say which emulator ran.** A one-line print of the resolved
   `qemu_system_arm_path()` in the cell's output would have made this visible the
   first time anyone compared a hand-run against a cell.

## Reproduce

    # slow (2 PT_LOADs) vs fast (3 PT_LOADs), same emulator
    ~/.nros/sdk/qemu/11.0.0-nros2/bin/qemu-system-arm -M virt -cpu cortex-a7 \
      -nographic -kernel examples/qemu-arm-nuttx/cpp/action-server/build-zenoh/cpp_action_server
    ~/.nros/sdk/qemu/11.0.0-nros2/bin/qemu-system-arm -M virt -cpu cortex-a7 \
      -nographic -kernel examples/qemu-arm-nuttx/c/action-client/build-zenoh/c_action_client

    # the same two on the distro emulator: both immediate
    /usr/bin/qemu-system-arm -M virt -cpu cortex-a7 -nographic -kernel <same>

    # the discriminator
    arm-none-eabi-readelf -l <image> | grep '^  LOAD'

---

## FIXED 2026-09-04 — direction 1, in the NuttX fork's board linker script

The change is one commit on the `nuttx` fork's patch branch — `aea00d73` on
`nano-ros`, a fast-forward over the previously pinned `c3fa5dfb` (0 behind,
1 ahead) — pushed on the owner's instruction, with the superproject pin bumped
to it in the same PR. Reading the fork's branches needed `git ls-remote`: the
submodule is a single-branch clone (`+refs/heads/master:refs/remotes/origin/
master`), so `git branch -a` showed only `master` and the `nano-ros` branch
`.gitmodules` names looked as though it did not exist.

### The change

`boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld`:

```
-  .bss : {
+  .bss ALIGN(ADDR(.noinit) + SIZEOF(.noinit) + 1, 16) :
+       AT (LOADADDR(.noinit) + SIZEOF(.noinit)) {
```

Both halves are load-bearing:

* the **ALIGN** forces `.bss`'s VMA off the end of the preceding section. `ld`
  opens a new segment only when the next section's VMA is not contiguous, and
  the `+ 1` before rounding makes that unconditional — an already-aligned
  `.data` still moves. Cost: at most 15 bytes of RAM.
* the **AT** keeps `.bss`'s LMA on the ROM run. Without it `ld` assigns
  `LMA == VMA` in RAM, which splits the segment correctly and **breaks the
  image** (see below).

The result is byte-for-byte the layout the two naturally-split images already
had: three `PT_LOAD`s, `.bss` at `p_filesz == 0` with an LMA in flash.

### Measured

All twelve NuttX arm C/C++ images, on `11.0.0-nros2`:

| | before | after |
| --- | --- | --- |
| `PT_LOAD` count | 2 (ten images), 3 (two) | **3 (all twelve)** |
| time to first console byte | 19.5–20.1 s (ten), 0.05 s (two) | **0.05–0.10 s (all twelve)** |

`rtos_e2e`, the six NuttX C/C++ cells, `--retries 0`, same host, back to back:

| cell | before | after |
| --- | ---: | ---: |
| action C | 25.7 s | 6.0 s |
| action C++ | 45.7 s | 4.8 s |
| pubsub C | 81.4 s | 61.1 s |
| pubsub C++ | 79.6 s | 60.6 s |
| service C | 20.9 s | 0.7 s |
| service C++ | 41.3 s | 0.8 s |
| **wall clock** | **294.8 s** | **134.1 s** |

**6 of 6 passed before and after** — this is a latency change, not a behaviour
change. The pubsub cells stay near 61 s because that is their deliberate
60-sample window, which no longer has ~20 s of emulator stall in front of it.

### Two variants that were wrong, and are worth not re-trying

* **`. = . + 16;` between `.noinit` and `.bss` does nothing.** Both sections are
  placed with `> RAM`, and a region-assigned output section takes its address
  from the region's allocation pointer, so the bare dot assignment is ignored.
  Verified in the generated script — the text was present and `.bss` still
  started at `.data`'s end.
* **`AT > RAM`, and an explicit ALIGN with no AT, both split the segment and
  both break the image.** The image boots and prints its banner, then fails at
  session open with `zpico Session -> ConnectionFailed`. Confirmed as an A/B
  against a control built from the pristine script on the same tree, with a
  working router, control re-run afterwards to rule out drift. The shift size is
  not the variable: a 16-byte move fails exactly like a 0x250 one. The variable
  is the LMA — flash works, RAM does not.

  **Why an LMA in RAM breaks the network stack is NOT explained.** Nothing in
  the boot path should read `.bss`'s load address. Recorded rather than guessed
  at; anyone changing this section again should treat it as a live hazard.

### Found on the way: two linker scripts, two sources, one link

`nros-nuttx-ffi` preprocesses `$NUTTX_DIR/boards/arm/qemu/qemu-armv7a/scripts/
dramboot.ld` — the LIVE tree — while `nros-board-nuttx-qemu` preprocesses
`nros-nuttx-export-<arch>/scripts/dramboot.ld` — the SNAPSHOT. Both emit a
`dramboot.ld` into their own `OUT_DIR`; only the `nros-nuttx-ffi` one reaches
the link. Editing the snapshot alone therefore changes nothing while looking
like it should, which cost a build cycle here. phase-339's rule is that
consumers link the snapshot, so the `nros-nuttx-ffi` path is the odd one out.
Not fixed here — it is a separate change with its own blast radius.

### Still open in this issue

The mechanism section above is still INFERRED: `third-party/qemu/qemu` is not
initialised, so `hw/core/loader.c` was not read, and no stock upstream QEMU 11
was available to say whether this is upstream behaviour or something our patch
line introduced. The fix does not depend on that answer — it removes the
zero-fill rather than changing how it is performed — but the answer is still
worth having, because any other board whose `.data` is `AT > ROM` has the same
exposure.

Direction 3 (print which emulator resolved) is also not done.
