---
id: 415
title: "`nros::main!`'s framework table is deploy-keyed, so an out-of-tree RTIC/Embassy board emits OwnedSpin"
status: resolved
type: bug
area: codegen
related: [phase-337, phase-346, rfc-0064, issue-0248]
---

> **RESOLVED 2026-08-10 by phase-346 W1.** The mapping moved to
> `nros_orchestration_ir` (beside `board_path_for`, the table the macro and the
> CLI emitter already shared), and an out-of-tree board reaches it through its
> OWN manifest: the Entry package's build script calls
> `nros_build::emit_board_framework()`, which resolves the board dep, reads
> `[package.metadata.nros.board] framework`, and emits
> `cargo::rustc-env=NROS_BOARD_FRAMEWORK` plus `rerun-if-changed` on what it read.
>
> **Not at macro-expansion time, which is where this issue proposed it.** A
> spike (recorded in the phase doc) measured that proc-macro env and file reads
> are invisible to cargo's fingerprint, and that a cargo-config value is not even
> visible when cargo runs from a workspace root — so the obvious route serves a
> stale or empty answer, which IS this defect wearing a different hat. A build
> script can declare the edges, so the resolution lives there.
>
> **An unknown framework is now an error** naming the accepted set, never a
> fall-through to `OwnedSpin`. That fall-through was the whole issue.
>
> The proof that the seam is reachable is not an assertion: `Framework::Embassy`
> carried `#[expect(dead_code)]` reading "the expect fires the day it becomes
> constructible", and building this change made that expectation UNFULFILLED —
> the compiler reporting Embassy is selectable again.
>
> Residual, tracked in phase-346 rather than here: `nros sync` does not yet
> GENERATE that build script, so an out-of-tree integrator adds three lines
> themselves; and no out-of-tree Embassy image was linked (this repo carries no
> embassy dependency set for Cortex-M), so the evidence is resolution tests plus
> the retired expectation.

## Symptom

An out-of-tree board crate that declares

```toml
[package.metadata.nros.board]
framework = "embassy"     # or "rtic"
```

is understood by `nros ws check` (which reads that key —
`nros-cli-core/src/cmd/check_workspace.rs::framework_for_board_crate`) but NOT by
the `nros::main!` proc-macro, which picks the emit shape from a hardcoded
**deploy-string** table:

```rust
// packages/core/nros-macros/src/main_macro.rs
fn framework_for(deploy: &str) -> Framework {
    match deploy {
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => Framework::Rtic,
        "zephyr" => Framework::Zephyr,
        "esp32-qemu" | "qemu-esp32-baremetal" => Framework::Esp32,
        _ => Framework::OwnedSpin,
    }
}
```

An unknown deploy key falls through to `OwnedSpin`, so the board gets a plain
`fn main()` instead of `#[embassy_executor::main]` / `#[rtic::app]`. The failure
is not a diagnostic — it is a **silently wrong entry shape**, which on a
bare-metal Cortex-M target surfaces as an image that links and then does nothing
the framework was supposed to do.

## Why it surfaced now

Before phase-337 W7.a, every framework had at least one in-tree deploy key, so
the table was always reachable and the gap was invisible. W7.a deleted the three
STM32F4 board crates; `embassy-stm32f4` was the **only** key that selected
`Framework::Embassy`. The framework itself is deliberately kept — `EmbassyBoardEntry`
(`nros-platform/src/board/embassy_entry.rs`) and the macro's Embassy emit branch
are the seam RFC-0064 says an integrator consumes — but nothing in-tree reaches
it any more, so the emit branch is now only exercised by the macro's own parser
tests.

`Framework::Rtic` is in the same shape and merely happens to still have an
in-tree key (`rtic-mps2-an385`).

## Fix shape

Make the macro read the same SSoT `nros ws check` reads, rather than adding a
second spelling of the mapping (CLAUDE.md's recurring-defect rule):

1. resolve the board crate from `board_path_for(deploy)` **or** the entry pkg's
   board dependency, then
2. read `[package.metadata.nros.board] framework` off that crate's manifest at
   macro-expansion time, and
3. keep the deploy table only as the in-tree fast path, with the metadata answer
   winning when both exist.

Step 2 needs a build-graph fs round-trip at expansion time — the same round-trip
`rtic_board_spec_for`'s `dispatchers` const is deferred on, and the reason this
was not done inside W7.a.

Until then an out-of-tree framework board must pass `board = ...` and accept
`OwnedSpin`, or vendor a deploy key.

## Related

- issue 0248 — the Embassy board crate this table's last Embassy key pointed at
  (deleted by phase-337 W7.a; the issue's underlying "Deferred image signals into
  a channel nothing drains" defect is retired with it).
- RFC-0064 — boards arrive through integration shells, which is exactly the path
  this gap blocks.
