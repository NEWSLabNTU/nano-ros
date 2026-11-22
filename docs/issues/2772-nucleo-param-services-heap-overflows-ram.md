---
id: 2772
title: "The stock node-interfaces default answers for every image alike: the Nucleo-F767ZI Rust action server no longer links, and an RTIC image is refused"
status: open
type: bug
severity: high
area: [orchestration, zephyr, sizing]
related: [phase-556, issue-2647, issue-2559, issue-1766]
found: 2026-11-21
---

**Home: [phase-556](../roadmap/phase-556-ros2-compat-audit.md) W13 fallout.**
Both symptoms trace to 6229085857 ("every image serves the stock node
interfaces by default"). Neither 57c19cd7df nor 67d36cb764 moved a Rust
Nucleo image: Rust with `param_services` had a 192 KiB heap before and after
57c19cd7df, which only shrank the undeclared store from 32 slots to 8, and
67d36cb764 reads only the two ThreadX boards' rungs.

## What happens

1. **Tier 2 stops in BUILD.** `baremetal_levels_rtic_entry`
   (`examples/workspaces/rust/src/levels_bringup`, `framework = "rtic"`) is
   refused by `nros::main!`: "this system declares `param_services` but the
   RTIC framework entry does not register the runtime's parameter or lifecycle
   services". The refusal is right. The default put the stock set on an image
   whose entry cannot serve it. Every other RTIC image had opted out by hand;
   this one, added later, had not.
2. **A Nucleo-F767ZI Rust image no longer links.** The stack-floor drift row
   `action-server/rust/zenoh@nucleo-f767zi-zephyr` (a fresh `nros new` +
   `nros build`, the user's road) fails with
   `region 'RAM' overflowed by 108992 bytes` against the board's 384 KiB
   SRAM. The image now gets `param_services`, so the Kconfig heap default goes
   from 128 KiB (`default 131072 if RUST`) to 192 KiB
   (`default 196608 if RUST && NROS_CAPABILITY_PARAM_SERVICES`), and the
   executor backing grows by the 8-slot store (about 8.9 KiB a slot,
   issue 2647). Measured by the reporter: heap 196,608 B, `EXECUTOR_BACKING`
   110,224 B, main stack 77,728 B. Lowering the heap to the C images'
   112 KiB alone frees 81,920 B, short of the overflow by about 27 KiB, so no
   single default fixes it.

## Fix

1. (RTIC) The default is derived from the entry's capability:
   `nros_orchestration_ir::FRAMEWORKS_WITHOUT_NODE_SERVICES` names the
   frameworks whose entry registers no runtime node services, and
   `node_interfaces::entry_default` gives such an image `none` when no table
   states a switch and it declares no stock axis itself. A stated switch or a
   declared axis still reaches the macro's refusal.
2. (Nucleo) The Rust branch of the heap default is gone. A Rust image with
   param_services and no lifecycle gets the 112 KiB C and C++ already had:
   the branch's 114,144 B came from an image declaring both axes, and split,
   features' zephyr_rust_params on native_sim peaks at 45,840 B. The
   action-server row links with about 4.6 KiB of RAM spare, and in its Renode
   twin the heap peaks at 47,880 of 115,200 B. Still open: that row's stack
   walk rose to 109,184 B (a type-bound indirect edge, not a deeper call) now
   that the image links; the row and the drift check's handling of it are
   issue 2783's, and peak-sized stacks give the template back about 44 KB.

## Acceptance

- `baremetal_levels_rtic_entry` builds; tier 2 reaches its cells.
- The three Nucleo stack-floor drift rows build and link.
- Every in-tree small-SRAM image is checked for the same overflow.
