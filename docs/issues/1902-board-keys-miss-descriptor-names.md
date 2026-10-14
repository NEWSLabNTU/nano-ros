---
id: 1902
title: "Three board-descriptor names are not entry keys: `threadx-riscv64` is accepted by the catalog and refused by the entry codegen, so the every-board alias test is red"
status: open
type: bug
area: [tooling, build]
severity: low
found: 2026-10-15
related: [1285, 1109, 1519, 1525, phase-500]
---

## Summary

A board has one set of names, written in its descriptor
(`packages/boards/**/nros-board.toml`, `names = [...]`). The entry pipeline
keeps its own key table, `nros_entry_lower::BOARD_KEYS`
(`packages/cli/nros-entry-lower/src/lib.rs`). Issue 1285 made that table the
one key-to-family map, and phase-500 W1 checked that it agrees with the
descriptor for the host board only. Asked of every board, the two disagree:

- `threadx-riscv64` is a name of a tier-2 board, and `BOARD_KEYS` lacks it.
  The board catalog accepts it, and the typed C/C++ entry codegen refuses it.
- `mps2-an385-zephyr` and `qemu-cortex-a53` are names of two Zephyr
  descriptors, and `BOARD_KEYS` lacks both. Both boards are
  `tier = "scaffold"`.

No name is in `BOARD_KEYS` with the wrong family.

## Measured

### The host-only test, and why it is host-only

`packages/cli/nros-cli-core/tests/board_key_table.rs`,
`the_host_board_keys_are_one_set_everywhere`, compares six sets for the host
board. Its doc comment says why it stops there:

```rust
/// Scoped to the HOST board on purpose. Asked of every board, the same rule is
/// red today for reasons outside this issue: `threadx-riscv64` is a descriptor
/// name `BOARD_KEYS` lacks, `BOARD_PATHS` splits one descriptor by entry shape
/// (`rtic-mps2-an385` vs bare `mps2-an385`), and ESP32 is dormant (issue 1525).
#[test]
fn the_host_board_keys_are_one_set_everywhere() {
    ...
    let host: Vec<_> = catalog
        .descriptors()
        .iter()
        .filter(|d| d.platform.board_family() == Some(BoardFamily::Linux))
        .collect();
```

### Every descriptor name against `BOARD_KEYS`

The script below reads every tracked `nros-board.toml` (test fixtures
excluded), maps its `platform` to a family, and looks each name up in
`BOARD_KEYS` and `nros_orchestration_ir::BOARD_PATHS`. It also reads the
`tier` of each board crate from `packages/boards/board-support.toml`.

```
$ python3 tmp/pissues/board_names.py .     # (script not committed)
mps2-an385-zephyr        zephyr           tier=['scaffold'] path=- MISSING from BOARD_KEYS (want Zephyr)
qemu-cortex-a53          zephyr           tier=['scaffold'] path=- MISSING from BOARD_KEYS (want Zephyr)
threadx-riscv64          threadx-riscv64  tier=[2]          path=- MISSING from BOARD_KEYS (want Threadx)
esp32-qemu / esp32 / esp32c3 / esp32-c3 / esp32-c3-baremetal
                         esp32            tier=['scaffold'] no-RTOS platform: absent (correct)
baremetal / bare-metal / qemu-mps2-an385 / rtic-mps2-an385
                         bare-metal       tier=[2]          no-RTOS platform: absent (correct)
(the other 21 names: ok — present, same family)

NAME IN >1 DESCRIPTOR: threadx: [nros-board-threadx-linux, nros-board-threadx-qemu-riscv64]
BOARD_KEYS row with no descriptor name: armfvp (Zephyr)
BOARD_KEYS row with no descriptor name: freertos-qemu-mps2-an385 (Freertos)
BOARD_PATHS row with no descriptor name: freertos-qemu-mps2-an385 -> ::nros_board_mps2_an385_freertos::Mps2An385
BOARD_PATHS row with no descriptor name: qemu-rtic-mps2-an385 -> ::nros_board_mps2_an385::RticMps2An385
BOARD_PATHS row with no descriptor name: mps2-an385 -> ::nros_board_mps2_an385::Mps2An385
```

What each line means:

- **Forward direction (descriptor name, no key): 3 names.** Only
  `threadx-riscv64` belongs to a supported board (tier 2). The two Zephyr
  names belong to `tier = "scaffold"` boards.
- **No family mismatches.** Every name that is in `BOARD_KEYS` has the family
  its descriptor's `platform` implies.
- **`threadx` names two descriptors.** This is deliberate.
  `BoardCatalog::resolve_deploy` picks between them by target, and
  `board_descriptor.rs` tests that (`"threadx is claimed by two boards"`). Both
  are ThreadX, so the family is the same either way.
- **Reverse direction (key, no descriptor name).** `armfvp` and
  `freertos-qemu-mps2-an385` are in `BOARD_KEYS`. `freertos-qemu-mps2-an385`,
  `qemu-rtic-mps2-an385` and `mps2-an385` are in `BOARD_PATHS`. None of them
  is a name in any descriptor. A selector written with one of them is accepted
  by the entry tables, but no board catalog lookup finds it. That
  consequence is **unmeasured**.

### The mps2 descriptor that `BOARD_PATHS` splits on purpose

`packages/boards/nros-board-mps2-an385/nros-board.toml` is one descriptor,
`platform = "bare-metal"`, with
`names = ["baremetal", "bare-metal", "qemu-mps2-an385", "rtic-mps2-an385"]`.
`BOARD_PATHS` maps its keys to two different board types:

```
("rtic-mps2-an385",      "::nros_board_mps2_an385::RticMps2An385", false),
("qemu-rtic-mps2-an385", "::nros_board_mps2_an385::RticMps2An385", false),
("qemu-mps2-an385",      "::nros_board_mps2_an385::Mps2An385",     false),
("mps2-an385",           "::nros_board_mps2_an385::Mps2An385",     false),
```

The doc comment on `BOARD_PATHS` (`packages/core/nros-orchestration-ir/src/lib.rs`)
says why: the RTIC entry surface moved into this crate in phase-337 W6.a, and
the two keys stay separate "because the two entry shapes are distinct". So the
descriptor is one board, and the Rust pack has two entry shapes for it, picked
by the key. This is not a `BOARD_KEYS` problem: the platform has no RTOS, so
none of these names belongs in `BOARD_KEYS`, and none is there. A
"one descriptor, one board type" rule cannot hold for this board. An
every-board test has to allow one descriptor to map to several types, or
compare families only.

### ESP32

All five ESP32 names are absent from `BOARD_KEYS`, which is correct: esp32 has
no RTOS family. The board is `tier = "scaffold"` in `board-support.toml`
(line 203) and dormant (issue 1525). The same tier marks
`nros-board-mps3-an536-freertos`, `nros-board-qemu-cortex-a53` and
`nros-board-mps2-an385-zephyr`, so "skip `tier = "scaffold"`" would also skip
the two Zephyr names above. That is consistent with the tier's meaning. The
an536 names are in `BOARD_KEYS` anyway.

### What `board = "threadx-riscv64"` does today

I copied `examples/rv-virt-threadx/c/talker` to a temp leaf and changed only
`[image.rv-virt-threadx] board`.

The leaf reader accepts it and finds the right board:

```
$ nros ws leaf-system tmp/pissues/leaf-threadx-riscv64
NROS_LEAF_IMAGE=rv-virt-threadx
NROS_LEAF_BOARD=threadx-riscv64
NROS_LEAF_DEPLOY=threadx
NROS_LEAF_CMAKE_BOARD=rv-virt-threadx
rc=0
```

The typed entry codegen refuses it. It was run against the resolved
`examples/workspaces/c` model and probe metadata, with only `--board` changed:

```
$ nros codegen entry --lang c --typed --metadata <c probe>/nros-metadata.json \
      --workspace examples/workspaces/c \
      --model examples/workspaces/c/build/nros/models/demo_bringup/system_model.yaml \
      --board <B> --out tmp/pissues/entry-<B>.c
--board rv-virt-threadx   rc=0
--board threadx           rc=0
--board threadx-riscv64   rc=1
Error: unknown board key `threadx-riscv64` — the entry pipeline knows: linux, zephyr,
fvp-aemv8r-smp, armfvp, nuttx, qemu-armv7a-nuttx, rv-virt-nuttx, nuttx-riscv, freertos,
mps2-an385-freertos, freertos-qemu-mps2-an385, freertos-posix, s32z270-freertos, s32z270,
mps3-an536-freertos, an536, threadx, threadx-linux, threadx-qemu-riscv64, rv-virt-threadx
Location:
    nros-cli-core/src/cmd/codegen.rs:450:78
```

So a C/C++ image that selects this board by a name its own descriptor
publishes passes `nros ws leaf-system` and the board catalog. It then fails at
the typed entry, with an error that lists `rv-virt-threadx` and
`threadx-qemu-riscv64` but not the name the user wrote.

A plain C leaf with its own `main()` (like the talker) never calls
`codegen entry`, so it would not reach this. I did not build one to confirm.
A Rust leaf goes through `nros::main!`, which asks `BOARD_PATHS` first and
then the board crate's `NROS_BOARD_TYPE`
(`packages/core/nros-macros/src/main_macro.rs:626`). `threadx-riscv64` is not
in `BOARD_PATHS`. Whether the fallback rescues it depends on that crate's
metadata and build script. **Unmeasured**: a Rust riscv64 leaf was not built.

Side observation, also **unmeasured** past this output: with
`board = "threadx"`, `leaf-system` prints `NROS_LEAF_CMAKE_BOARD=threadx`, and
`cmake/board/` has no `nano-ros-board-threadx.cmake` (only `rv-virt-threadx`
and `threadx-linux`).

## Why nothing caught it

`board_key_table.rs` holds `BOARD_KEYS` against itself, against `BOARD_PATHS`
and against the descriptors' `entry_kind`. Its only name-set comparison against
the descriptors is host-only. `the_rust_pack_key_set_agrees_with_the_family_table`
starts from `BOARD_PATHS` and checks its keys against `BOARD_KEYS`, so a
descriptor name that is in neither table is never looked at. That is the 0196
shape: the rule is about every board, and the test reads one.

## Fix direction

1. Add `("threadx-riscv64", BoardFamily::Threadx)` to `BOARD_KEYS`, and decide
   whether `BOARD_PATHS` gets it too. If not, add it to
   `family_keys_without_a_rust_board_are_exactly_these`.
2. Widen the test. For every descriptor whose platform has a family and whose
   registry tier is not `scaffold`, every `names` entry is in `BOARD_KEYS` with
   that family. Platforms with no RTOS need no row. One descriptor may map to
   more than one `BOARD_PATHS` type (the mps2 case above), so compare families
   only, never board types.
3. Decide the reverse direction: either every `BOARD_KEYS` / `BOARD_PATHS` key
   is a descriptor name, or a key that is not one is listed with a reason.
   `armfvp`, `freertos-qemu-mps2-an385`, `qemu-rtic-mps2-an385` and
   `mps2-an385` would need one of those today.

Gate: the widened `board_key_table.rs` test (it runs in `test-unit`). It has to
read the descriptors through `BoardCatalog`, as the host test does, so a new
descriptor is covered with no edit to the test.
