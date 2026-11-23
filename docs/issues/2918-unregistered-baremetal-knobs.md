---
id: 2918
title: "Two bare-metal build settings are read by code but are not knob-registry rows, so no naming rule covers them"
status: open
type: bug
area: [config, knobs, baremetal]
severity: low
found: 2026-11-24
---

## What happens

Two build-time variables reach bare-metal images without a row in the knob
registry:

- `NROS_HEAP_SIZE`: the static heap in bytes. `option_env!` reads it in
  `packages/platform/nros-platform-mps2-an385/src/memory.rs` and
  `packages/platform/nros-platform-stm32f4/src/memory.rs`. The heap refusal
  in `packages/platform/nros-baremetal-common/src/heap.rs` tells the user to
  raise it.
- `NROS_BAREMETAL_STACK_KB`: the mps2-an385 main stack in KiB. It is read in
  `packages/boards/nros-board-mps2-an385/build.rs` through
  `platform_config::knob_env`. The stack-guard message and the link-time
  refusal name it.

The book documents both (`reference/environment-variables.md`,
`guide/platforms/bare-metal.md`, `guide/build/configure-an-image.md`,
`reference/build-outputs.md`). The book is right about what the code reads.

Because neither is a registry row:

- `just check config-names` never sees them, so the naming rule does not apply.
  `NROS_BAREMETAL_STACK_KB` states a size in KiB, where the rule wants bytes
  in the name (`_bytes`), as `memory.heap_bytes` and the other renamed sizes
  are. `NROS_HEAP_SIZE` has no owner segment and ends in `_SIZE`.
- `nros knob show NROS_HEAP_SIZE` answers "no knob named `NROS_HEAP_SIZE`",
  and the same for the stack. `nros knob list` shows `memory.heap_bytes`
  and `memory.app_stack_bytes`, and nothing says whether either sizes these
  two images.
- They cannot be stated as `[image.<id>.knobs.*]` in `system.toml`, only as
  a build variable: the shell, or an image's `env` row.
- `check-book-retired-names` cannot catch a future rename of either, because
  it reads retired spellings from the registry.

Found during the book's retired-name sweep (#4726).

## What would close it

1. Add registry rows for both, owned by the bare-metal platform:
   - the heap: `memory.heap_bytes` if it can reach bare metal, else a
     bare-metal row whose key ends `_bytes`;
   - the stack: `memory.app_stack_bytes` ("per platform") if it can, else a
     bare-metal row in bytes.
2. Retire `NROS_HEAP_SIZE` and `NROS_BAREMETAL_STACK_KB` on those rows. The
   stack's retired entry carries `factor = 1024, from_unit = "KiB"`, as
   `NROS_FREERTOS_HEAP_KB` does.
3. Read the new names in the two `memory.rs` files and the mps2 `build.rs`,
   and update the refusal messages, `config/stack-floors.toml`'s method text
   and the book pages above.
4. Add a changelog `breaking` fragment naming both renames.

## Related

- `packages/tooling/nros-zephyr-build/knobs.toml` (`memory.heap_bytes`,
  whose retired entries show the KiB-to-bytes form)
- `scripts/check/check-config-names.py`, `scripts/check-book-retired-names.py`
