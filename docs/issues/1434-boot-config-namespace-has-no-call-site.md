---
id: 1434
title: "The baked namespace reaches no runner — `nros_boot_config_namespace()`
  has no call site and every board hardcodes `namespace: None`"
status: open
type: bug
area: boot, boards
related: [rfc-0045, 0794, 1410]
---

## Problem

The `.nros_boot_config` blob states a launch-declared namespace on BOTH
producers now — the C/C++ emitter since issue 0794, `nros::main!` since issue
1410 — and `BootConfig::from_baked` reads the bit correctly. What is missing is
a CONSUMER, on either road.

**C/C++.** The generated entry reads `nros_boot_config_node_name()` and nothing
else from the blob; its locator and domain reach the runner through the
`NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` compile definitions, and its
namespace reaches nothing at all:

```c
int main(int, char**) {
    return ::nros::board::LinuxBoard::run_components(
        nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup);
}
```

**Rust.** Every board that reads the blob drops the field on the floor —
measured, six sites, all spelled the same way:

```
packages/boards/nros-board-freertos/src/entry.rs:326:        namespace: None,
packages/boards/nros-board-freertos/src/entry.rs:824:        namespace: None,
packages/boards/nros-board-threadx/src/entry.rs:632:        namespace: None,
packages/boards/nros-board-threadx/src/entry.rs:1024:        namespace: None,
packages/boards/nros-board-mps2-an385/src/entry.rs:173:        namespace: None,
packages/boards/nros-board-esp32-qemu/src/board_entry.rs:191:        namespace: None,
```

Each takes `baked.node_name` (and, since issue 1050, `baked.rmw`) off the
resolved `BootConfig` and then writes `namespace: None` into the
`ExecutorConfig` it resolves. `nros-board-linux` does not read the blob at all
beyond the name — `lib.rs:313`, "Only `node_name` is mapped from the overlay;
locator/domain/namespace stay `None` so env keeps authority over them".

So the blob is TRUE — which is what RFC-0045's post-link patch tool and the
`nros_boot_config_*` accessors need — without being the delivery mechanism for
the PRIMARY SESSION's namespace on any platform.

## What is NOT broken, and why this is not urgent

A per-node registration already carries its own namespace: `nros::main!` emits
`runtime.node_identity = Some(("remap_talker", "/island"))` from the same FQN
split (issue 1172), and `NodeRecord` creates the node with it. Measured in the
expansion of `native_rust_remap_entry`. The gap is the SESSION rung — the
identity a board opens the executor with when nothing per-node names one, and
the field a post-link patcher would rewrite.

## Why it keeps getting carved out

Issue 0794 and issue 1410 both recorded this rather than closing it, because it
is a different SHAPE of change from either: it means giving the C-ABI board
runners (`run_components` / `run_tiers`, ten board crates) a namespace
parameter plus the parity ledger that pins that ABI, and deciding per board
whether the baked rung or the board `Config` wins. Neither the emitter fix nor
the macro fix touches a board.

## Acceptance

A C, a C++ and a Rust image whose launch declares a namespace open their
primary session under it — asserted on the ON-WIRE name, since the blob half is
what already works. The negative direction matters as much: an image whose blob
leaves `BOOT_SET_NAMESPACE` clear must keep whatever the next RFC-0045 rung
says, never `""` — and on the hosted board `$NROS_NODE_NAMESPACE` must still
outrank the bake (`namespace_resolves_over_all_three_rungs`).
