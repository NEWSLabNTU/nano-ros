---
id: 1601
title: "`mps2-an385-baremetal/rust/talker-xrce` registers no RMW backend — its `transport = \"serial\"` no longer matches the board's `Some(\"xrce\")` gate"
status: resolved
type: bug
area: [examples, boards]
severity: medium
found: 2026-10-01
resolved_in: 2026-10-02
related: [0189, 1265, phase-244, phase-445, rfc-0098]
---

## What

`test_qemu_xrce_pubsub_e2e` (`packages/testing/nros-tests/tests/emulator.rs`)
fails 3/3 on a freshly built fixture, and the image says why:

```
Executor::open failed: Transport(InvalidConfig)
[ERROR] nros: cannot select an RMW backend — no RMW backend is registered
Bare-metal XRCE QEMU: published=0
```

## Why

The XRCE backend on this board is registered by
`nros-board-mps2-an385`'s `BoardEntry::setup_transport` (`src/entry.rs`), and
only inside

```rust
#[cfg(feature = "xrce-transport")]
if deploy.transport == Some("xrce") { ... nros_rmw_xrce_cffi::register() ... }
```

(issue 0189: bare metal runs no `.init_array`, so this explicit call is the ONLY
registration). phase-445 W6 (commit "the app rung gets a spelling, so the leaf
`[env]` has somewhere to go", 2026-09-11) changed the leaf's `system.toml` from
`transport = "xrce"` to `transport = "serial"` and started VALIDATING the key
against the three link kinds, calling `"xrce"` "the RMW name in the slot that
names the LINK". The board's gate was not moved with it, so since then the
image enables `xrce-transport` and never reaches the call.

Both halves are on `origin/main` as written; nothing about the leaf's
`Cargo.toml` is involved (found while issue 1265 rebuilt every
`mps2-an385-baremetal` Rust row — the other two pub/sub e2e tests on the same
fixture build, `test_qemu_bsp_pubsub_e2e` and `test_qemu_serial_pubsub_e2e`,
pass).

## Fix direction

The board should key on what the image declares that is actually about the
backend — the image's RMW (`[system] rmw = "xrce"`) plus a serial link — not on
the link-kind string standing in for both. Acceptance: the e2e test above
passes, and a gate or test that boots the image fails if registration is
skipped again (today the only witness is a QEMU e2e no merge lane runs).

## Resolution

Fixed by the PR "fix(#1601): the mps2 board registers XRCE on `rmw`, and the
link slot is a typed `LinkKind`" (2026-10-02).

**The class, not the site.** The defect was a string-typed slot whose
vocabulary changed under one reader. `DeployOverlay.transport` was
`Option<&'static str>`, so comparing it with an RMW name compiled forever.
Sweep (`git grep -nE 'transport *== *Some|deploy\.transport|"xrce"' --
packages examples`): the mps2 board was the ONLY reader of the overlay's
`transport` in any board crate; the CLI-side readers
(`cmd/build.rs`, `cmd/leaf_settings.rs`) already compare against `"serial"`
and are validated by `leaf_system::TRANSPORT_KINDS`; cmake emits
`NROS_LEAF_TRANSPORT` and nothing reads it; Zephyr XRCE images select their
transport through Kconfig and never see a `DeployOverlay`. There is no other
mps2 or esp32 XRCE leaf (`git ls-files examples | grep -i xrce`: the one
bare-metal leaf, the native `bridge-xrce` workspace and the Zephyr
C/C++/Rust leaves).

What changed:

- `nros_platform::LinkKind { Serial, Tcp, Udp }` — `DeployOverlay.transport`
  is now `Option<LinkKind>`, so a board comparing the link slot with an RMW
  name is a type error. `nros::main!` maps `TRANSPORT_KINDS` onto it
  (`link_kind_variant`, total over the list — a word with no variant becomes
  a `compile_error!` in the image, never a silent `None`).
- `DeployOverlay.rmw` — the image's declared RMW (`LeafSystem::rmw`), baked by
  the macro. `DeployOverlay::selects(rmw, link)` is the one predicate.
- `nros-board-mps2-an385` registers XRCE on `selects("xrce", Serial)`, and an
  image declaring `rmw = "xrce"` over any other link now exits LOUDLY at boot
  naming the fix, instead of reaching `Executor::open` with no backend.

**Witness without QEMU:** `nros-macros`
`link_kind_tests::the_xrce_uart_leaf_bakes_the_pair_its_board_registers_on`
reads the real `talker-xrce/system.toml` through the macro's own
`overlay_from_leaf` and asserts the baked overlay carries `rmw = "xrce"` over
`LinkKind::Serial` — the pair the board keys on. It runs in `test-unit`.
Plus `nros-platform` `link_kind_tests` (round trip; an RMW name is not a
link; `selects` needs both halves).

**Measured:** after `just build qemu`, `test_qemu_xrce_pubsub_e2e` PASSES
(`published=1`), alongside `test_qemu_bsp_pubsub_e2e` and
`test_qemu_serial_pubsub_e2e` on the same build (3/3).
