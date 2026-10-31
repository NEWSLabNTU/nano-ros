---
id: 2185
title: "The Zephyr chapter's native_sim road builds in a west workspace nano-ros never patched"
status: open
type: bug
severity: medium
area: [zephyr, docs, installer]
related: [2177, 1662, 1796, 0605, phase-517]
found: 2026-10-31
---

## What

`integration-zephyr.md` has the reader create their own west workspace (probe
34: a `west.yml`, `west init -l manifest`, `west update`) and then build in it. Nothing
on that road applies nano-ros's Zephyr patch set
(`scripts/zephyr/patches/3.7.sh`), which only `nros setup board <b>
--zephyr-workspace <ws>` and the in-tree `scripts/zephyr/setup.sh` run. Both
native_sim tracks (`native-sim`, `native-sim-rust`) pass on that unpatched
workspace, so nothing reports it.

What the reader loses is mostly invisible:

* **The Rust lane's whole carrier.** `${NROS_BOARD_FACTS_ENV}` reaches
  zephyr-lang-rust's cargo command only through a line
  `cargo-features-patch.sh` adds. That one variable carries the board facts
  (issue 0605), a leaf's sizing descriptor (1662), the session properties
  (2051), every derived knob (1796) and the C cross compiler (2177). Unpatched,
  each crate builds with its defaults. On native_sim the image still links and
  runs, which is why the track is green. Issue 2177 found it on mps2-an385,
  where the missing cross compiler is a hard failure.
* **The NSOS patches** (`nsos-*-patch.sh`, `native-sim-ipproto-ip-patch.sh`),
  which the native_sim socket paths were written against.

## Fix direction

* Have the chapter run `nros setup board zephyr --zephyr-workspace .` after
  `west update` (probe 34), the way issue 2177's mps2 block now runs it for
  `mps2-an385-zephyr`. The generic `zephyr` descriptor then needs a
  `[board.provisioning]` block (`zephyr_line = "3.7"` and, for the Rust road,
  the host triple, which is not a constant). That is the open question.
* Or make `nros build` refuse a Zephyr Rust image whose workspace lacks the
  cargo-features patch, naming `nros setup board`, so a silent default never
  ships. The patch marks its own lines (`# nano-ros:`), so the check is a read.
* Either way, the probe should assert a Rust-lane fact reached cargo, not only
  that the image booted.
