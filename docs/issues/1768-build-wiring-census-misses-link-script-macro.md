---
id: 1768
title: "The build-script census reports the 8 converged linker-script shims as
  UNCLASSIFIED"
status: open
type: bug
area: build
severity: low
found: 2026-10-09
related: [phase-471, RFC-0101]
---

## What this is

`python3 scripts/nros-build-wiring.py` lists 8 build scripts as
`UNCLASSIFIED — rule these, do not add an 'other' bucket`:

    packages/boards/mps2-an385-pac/build.rs
    packages/boards/nros-board-mps2-an385/build.rs
    packages/testing/nros-bench/wake-latency-cortex-m3/build.rs
    packages/testing/nros-smoke/stm32f4-smoltcp-echo/build.rs
    packages/testing/nros-tests/bins/cdr-roundtrip-qemu/build.rs
    packages/testing/nros-tests/bins/heap-free-poc-mps2/build.rs
    packages/testing/nros-tests/bins/lan9118-qemu/build.rs
    packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal/build.rs

Each is one line — `nros_build_paths::link_script!("memory.x")` (one maps
`"mps2-an385.x" => "memory.x"`) — i.e. the shape phase-471 W4 converged them
ONTO. The `linker-script` role's predicate matches only the hand-written shape
it replaced (`rustc-link-search` plus a `.x` literal), so the migration moved
them out of the role it was meant to put them in. The two scripts the role
still matches are the deliberately standalone
`packages/reference/stm32f4-porting/{polling,rtic}` templates.

No gate reads the UNCLASSIFIED count, so the regression was silent.

## Fix

Teach the `linker-script` predicate the macro, and make a non-empty
UNCLASSIFIED bucket fail a check rather than print.
