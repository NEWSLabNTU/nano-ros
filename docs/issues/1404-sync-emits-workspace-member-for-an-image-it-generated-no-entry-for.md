---
id: 1404
title: "`nros sync` lists `src/esp32_entry` as a workspace member of
  `examples/workspaces/rust` while generating no entry for that image, so every
  cargo command in the workspace fails at manifest parse — including
  `just format`"
status: open
type: bug
area: cli, examples, build
severity: high
related: [0463, 0474, 1114, phase-445, RFC-0098]
---

## Symptom

After `nros sync` in `examples/workspaces/rust`, any cargo command run from any
package in that workspace fails before doing anything:

```
error: failed to load manifest for workspace member
  `…/examples/workspaces/rust/src/esp32_entry`
referenced by workspace at `…/examples/workspaces/rust/Cargo.toml`
Caused by: failed to read `…/src/esp32_entry/Cargo.toml`
Caused by: No such file or directory (os error 2)
```

`just format` dies on it (`cargo fmt` runs `cargo metadata` first), which makes
it a blocker for the whole fixture path: `just format` must run BEFORE
`build-test-fixtures` or every native fixture re-stales.

## Cause, measured

The generated root `Cargo.toml` lists every other image's entry under the
coordinate directory sync actually wrote:

```toml
members = [
    "build/freertos-zenoh/freertos_entry",
    "build/nuttx-zenoh/nuttx_entry",
    "build/posix-zenoh/native_entry",
    "build/threadx-linux-zenoh/threadx_entry",
    …
    "src/esp32_entry",          # <- the odd one out
]
```

`ls examples/workspaces/rust/build/` shows `freertos-zenoh  nuttx-zenoh
posix-cyclonedds  posix-xrce  posix-zenoh  threadx-linux-zenoh` — **no esp32
coordinate**. The resolved model directory
(`build/nros/models/demo_bringup/`) contains nine model files and the string
`esp32` appears in none of them.

So sync generated no entry for `[image.esp32]` and then emitted a member for it
anyway, at the **pre-phase-445 `src/` path** rather than the `build/<coord>/`
one every other image uses. `system.toml`'s own comment beside that image says
what the current shape is supposed to be:

> the entry is generated now (`build/<coord>/esp32_entry/`), so the image states it.

Not a toolchain gap: `riscv32imc-unknown-none-elf` is installed, and the board
descriptor `packages/boards/nros-board-esp32-qemu/nros-board.toml` claims the
name `esp32-c3-baremetal` that the image asks for.

The two Zephyr entries in the same file show the shape a non-cargo-built entry
is supposed to take — `exclude`, with a reason:

```toml
# Built by their own framework (west / idf), never by cargo.
exclude = [ "src/zephyr_entry", "src/zephyr_entry_robot1" ]
```

Whichever is right for esp32 — a generated `build/<coord>/esp32_entry`, or an
`exclude` row — the current output is internally inconsistent: it names a member
that the same sync run did not create.

## Residue found alongside, and it is NOT the cause

`examples/workspaces/rust/src/esp32_entry/` exists on disk, untracked,
containing exactly one file:

```
src/esp32_entry/.cargo/config.toml
  include = [ "../../../../../../nros-patch.toml", "nros-board.toml" ]
```

`nros-board.toml` is phase-445 W6's deleted board projection (RFC-0098 D1), so
this directory is pre-RFC-0098 leftover. **Measured: removing it and re-running
`nros sync` reproduces the same members list**, so sync is not reacting to the
directory's presence — the member is emitted unconditionally. The directory is
still residue worth deleting, but it is a separate cleanup.

## Repro

```sh
cd examples/workspaces/rust && nros sync
cd src/action_client_pkg && cargo metadata --no-deps --format-version 1   # fails
grep -n esp32 ../../Cargo.toml                                           # src/esp32_entry
ls ../../build/                                                          # no esp32 coordinate
```

## Impact

Every cargo command in `examples/workspaces/rust` — `fmt`, `metadata`,
`build`, `check` — fails at manifest parse for every package in it. `just
format` fails, and because formatting must precede a fixture build, tier 1 is
unreachable without a bypass.

## Acceptance

* `nros sync` never emits a `members` entry for a path it did not write. If an
  image yields no entry, it produces no member, or an `exclude` row with the
  same "built by its own framework" reason the Zephyr entries carry.
* A gate, or a sync self-check, that refuses to write a `Cargo.toml` naming a
  member with no manifest — the failure is mechanical and cheap to detect at the
  point of writing, where the message can name the image.
* The stale `examples/workspaces/rust/src/esp32_entry/` directory is removed.
