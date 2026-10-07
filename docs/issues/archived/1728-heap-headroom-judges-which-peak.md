---
id: 1728
title: "Three heap figures disagreed for the safety island; the boot record did
  not say which peak it judged, its knob advice ignored the 512 B slab, and a
  stated derivable knob below the derivation was silent"
status: resolved
type: limitation
area: [zephyr, memory, sizing]
severity: medium
found: 2026-10-06
related: [phase-474, phase-478, issue-1424, issue-1490]
resolved_in: "phase-474 I5 (feat/474-rest)"
---

## The three figures, read

| figure | what it is |
| --- | --- |
| "set `CONFIG_NROS_ZEPHYR_HEAP_SIZE` >= 133952" | `read-boot-report.py`'s `peak + floor` on the phase8-W1 QEMU image (4 nodes, parameter store, peak 109,376). A measurement of a different image, not a derivation: nano-ros has no heap derivation (issue 1424 chose "checked, not derived"); the Kconfig default reads the image's own declarations (`NROS_CAPABILITY_PARAM_SERVICES`, `NROS_PARAM_STORE`, `RUST`), which for the board image (C++, no parameter services) is 65,536. |
| board W31: peak 77,160 of 102,912, ok | the running peak of the 3-node board image as read after the acts. |
| board W4: "HEAP HEADROOM REFUSED" peak 79,712 of 102,912 | the same image plus the `/diagnostics` publisher (issue 1635); 23,200 spare is below the fixed 24,576 floor (`HEAP_HEADROOM_FLOOR`, the one constant the cmake arena check also uses), so it is refused. |

`heap_peak_bytes` is a high-water mark the Zephyr heap updates on every
allocation (`nros_boot_report_note_heap` from `nros_platform_alloc` /
`_realloc`), so a dump always judges the RUNNING peak as of the dump. What
was missing: the record could not say how much of it came after the first
spin, the advice `peak + floor` named a knob 512 B too high (capacity is the
knob plus zpico-alloc's 8 x 64 B slab, hence 102,912 for 102,400), and
nothing compared a stated derivable knob with its derivation (phase-478 D2).

## Resolution

- The boot record (layout v11) appends `heap_peak_at_first_spin`: the peak
  frozen when the stage reaches FirstSpin. `read-boot-report.py` prints it
  beside the running peak and the headroom verdict says which it judged
  ("the running peak at the dump; at FirstSpin it was X, so Y came after").
- The verdict's knob is `peak + floor - 512`, and the capacity line says
  "(NROS_ZEPHYR_HEAP_SIZE + 512 B slab)".
- `_nros_resolve_derivable_knob` compares a stated value (Kconfig or
  environment) with the image's derivation: below is a configure WARNING
  naming the knob, both numbers and the rung; above or equal is a STATUS line.
  The stated value still wins. The heap is not derivable, so it is not on
  this path; its check stays the measured headroom.

## Not done

- A QEMU heap read after the host graph joins: the record lives in guest RAM
  and the Zephyr QEMU lanes run through `west build -t run` with no monitor
  socket to read it from (`ZephyrProcess::heap_headroom` refuses emulators
  and says so). The board and native_sim dumps already judge the running
  peak.
- Reproducing phase8-W17's QEMU `HEAP EXHAUSTED` in `zpico_read` with graph
  discovery off (phase-474 I5 (a)) and bounding the read path's allocation
  (I5 (b)): needs the island's QEMU image and a host Autoware; not run here.

## What the island sets

From the W4 dump (peak 79,712): `CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 103776`;
104,448 (102 KiB) keeps the floor with 672 B to spare. Re-read the dump after
an act on the new pin and take `peak + floor - 512` from the line it prints.
