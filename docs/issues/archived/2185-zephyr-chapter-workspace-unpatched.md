---
id: 2185
title: "The Zephyr chapter's native_sim road builds in a west workspace nano-ros never patched"
status: resolved
type: bug
severity: medium
area: [zephyr, docs, installer]
related: [2177, 1662, 1796, 0605, phase-517, phase-524]
found: 2026-10-31
---

**Home:** [phase-524](../../roadmap/phase-524-emulator-twins.md)
— W4. The Zephyr chapters this phase writes take the same west-workspace road, so the chapter road gets the patch set (or a refusal) there. The generic `zephyr` descriptor's `[board.provisioning]` belongs to W3.

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

## Re-measured on main (2026-11-07, origin/main 0ede1760c2)

The chapter had moved (`book/src/guide/platforms/zephyr.md`, probe 34) and was
unchanged in substance. Measured in a workspace made from that block, in
`tmp/w2185/zws` (`west init -l manifest`, `west update --narrow -o=--depth=1`):

- `modules/lang/rust/CMakeLists.txt` had no `# nano-ros: board facts` line.
- The chapter's Rust talker configured, and its cargo command carried no
  `NROS_BOARD`, `NROS_BOARD_TOML`, `NROS_SESSION_PROPERTIES_RS`, derived
  `NROS_MAX_*` / `NROS_EXECUTOR_*` knob or `CC_<triple>`. After the patch set,
  the same command carried all of them.
- `nros setup board zephyr --zephyr-workspace <ws>`, which
  `zephyr-west-app.md` probe 37 (track `zephyr-west-app-rust`) already ran,
  refused: "`zephyr` has no `zephyr_line` in its `[board.provisioning]`
  block". That track could not get past that step.
- `scripts/zephyr/patches/4.4.sh` did not run `cargo-features-patch.sh`
  (issue 2203 noted it), so a 4.4 workspace provisioned through
  `nros setup board` lacked the carrier too.
- `examples/templates/zephyr-byo/README.md` applied three NSOS patches by hand
  and none of the Rust ones.

## Resolution

Both fix directions, on every road that makes its own west workspace.

- **The generic `zephyr` board is provisionable.** Its descriptor states
  `[board.provisioning] zephyr_line = "3.7"`, with no `rust_targets`: the
  board needs no Rust. `nros setup board` adds the triple a Rust image
  compiles for from `[board.zephyr] rust_target`, resolved for the host
  (`{host_arch}-unknown-none`), when the workspace holds zephyr-lang-rust. If
  the host has no rustup it prints a note instead of failing, because a C
  workspace needs no target.
- **Configure refuses an unpatched workspace.** The refusal lives in the
  Zephyr module, not in `nros build`, so it covers `nros build`, a plain
  `west build` and the fixture lanes alike.
  `nros_check_zephyr_rust_cargo_patch()` (`cmake/NanoRosBoardFacts.cmake`,
  called from `zephyr/CMakeLists.txt`) reads `CMakeLists.txt` in the module
  directory Zephyr itself resolved (`ZEPHYR_LANG_RUST_MODULE_DIR`). For a
  `CONFIG_RUST` image without the patch's marker it stops the configure with
  `nros setup board <BOARD> --zephyr-workspace <ws>`. A C image makes no
  claim.
- **Every patch set carries the carrier.** `patches/4.4.sh` now runs
  `cargo-features-patch.sh`, whose self-test already covers the 4.4 layout.
  `tests/zephyr-patch-set-workspace-tests.sh` case 5 asserts it for every
  `patches/<line>.sh`. Mutation (the line removed from 4.4.sh): the test fails.
- **The chapters run it.** `zephyr.md` has a new probe 36 after the west
  workspace (variants `native-sim`, `native-sim-rust`, `zephyr-twin`), plus
  the step in its Build section. `zephyr-west-app.md` has a new probe 36 for
  all four variants; the line moved there from the Rust-only probe 37. The
  BYO template README calls `nros setup board` in place of its hand-applied
  patches. `nucleo-f767zi.md` already ran it (probe 36).

The probe now asserts a Rust-lane fact. On the `native-sim-rust` track,
`nros build` can no longer configure an image whose cargo command lacks the
carrier, so a green build means the carrier was there.

Sweep (every chapter, template and script that makes its own west workspace):

```
git grep -n 'west init' -- '*.md' '*.sh' '*.yml' | grep -v archived
```

## Verified

- `tmp/w2185/zws`, the chapter's workspace (unpatched):
  - The Rust talker's `nros build` stops at configure with the new
    refusal, naming
    `nros setup board native_sim/native/64 --zephyr-workspace <ws>`.
  - After `nros setup board zephyr --zephyr-workspace .`, which applied
    the 3.7 set and added `x86_64-unknown-none`, the configure passes and
    cargo receives the board facts listed above.
- That build then stops at `nros::main!: pkg `rs_node_bringup` not found in
  workspace ... Known pkgs: []`. The board registry's `awaits` for the
  `native-sim-rust` track already records that error, and it is independent
  of this fix: an unpatched build with the refusal disabled hits the same
  error. So the full Rust road was not run to a boot here.
- Unit tests: `setup::board_rust_targets_tests` and
  `board_projection_round_trip`.

