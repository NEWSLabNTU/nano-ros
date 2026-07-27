# Phase 313 — Board-trait convergence: retire the legacy board_init family

Implements **[issue #0243](../issues/0243-platform-board-trait-duplication-transition.md)**
(RFC-0034). Finishes the phase-212.N.1 migration that landed the new
`nros-platform::board` trait set but almost none of the board migration, and
retires the legacy `nros-board-common::board_init` family so there is ONE board
contract, not two name-colliding ones.

## Problem

Two parallel board-lifecycle trait families are live at once:

| | Legacy (phase-152.4.B) | New (phase-212.N.1) |
| --- | --- | --- |
| Home | `nros-board-common::board_init` | `nros-platform::board` |
| `BoardInit` | `{ type Config; init_hardware(&Config) }` | `{ init_hardware() }` |
| Config flow | carried on the trait, threaded through `run<B>(cfg, f)` | pulled from a typed `RuntimeCtx` inside `BoardEntry::run`'s `setup` |
| Also: | `Board`, `BoardPrint`, `BoardExit`, `BoardEntry`, `DirectExec` | `Board`, `BoardPrint`, `BoardExit`, `BoardEntry`, `TransportBringup`, `NetworkWait`, … |

**Audit (2026-07-28, all 24 board crates):** ~22 implement the LEGACY family;
only 4 (`freertos`, `posix`, `threadx`, `zephyr`) also touch `nros-platform::board`
— and dually, as a thin `BoardEntry` wrapper delegating to the legacy
`run<B: BoardInit>(cfg, f)`. So the new family is aspirational; legacy is the
de-facto contract. A new board author must guess which `BoardInit` to implement,
and the `Board`/`BoardInit`/`BoardEntry` name collision is a standing footgun.

Decision (issue #0243, 2026-07-28): **finish the migration + retire legacy.**

## Approach

Move every board + the generic-kernel driver crates onto `nros-platform::board`
(config → `RuntimeCtx`), then delete `nros-board-common::board_init` + the
`NodeRuntime → NodeDispatchRuntime` deprecated alias, and add a lint that keeps
the legacy path from reappearing. Each wave is independently landable and leaves
the tree green (`just ci`).

The generic-kernel `run<B, F, E>(config, f)` boot paths
(`nros-board-{freertos,threadx,nuttx}/src/node.rs`, + the bare-metal/native
direct-exec paths) are the load-bearing change: they currently take `Config` by
value and call `B::init_hardware(&cfg)`; the new model constructs a `RuntimeCtx`
and calls the config-free `BoardInit::init_hardware()` + `BoardEntry::run`'s
`setup(&mut RuntimeCtx)`.

## Work items

### W1 — drop the elapsed `NodeRuntime` deprecated alias — DONE (2026-07-28)
Removed the `#[deprecated] pub use runtime::NodeDispatchRuntime as NodeRuntime`
(`nros-platform/src/board/mod.rs`) + the crate-root re-export
(`nros-platform/src/lib.rs`). The only consumers were those two internal
re-exports (no external impl used it), so it was a clean delete; impls use
`NodeDispatchRuntime` directly. The user-facing `nros::NodeRuntime` metadata type
is a different symbol, unaffected.
- **Verified:** `nros-platform` builds + lib tests 8/8, clippy clean;
  `nros-board-native` builds; no board-trait `NodeRuntime` reference remains.

### W2 — template: migrate the simplest kernel family fully (native/posix)
Take `nros-board-native` / `nros-board-posix` fully onto `nros-platform::board`
(config via `RuntimeCtx`), deleting their legacy `board_init` usage. Establishes
the migration recipe every later wave follows.
- **Acceptance:** native/posix implement ONLY `nros-platform::board`; native
  example + host lanes green; no `nros-board-common::board_init` reference in
  either crate.

### W3 — hosted / direct-exec boards
Migrate `esp32-qemu`, `esp32s3`, `rtic-*`, `embassy-stm32f4`, `stm32f4`,
`mps2-an385`, `bare-metal` to the new family (they take the direct-exec /
framework paths, not the generic-kernel `run<B>`).
- **Acceptance:** each builds its fixture on the new family; legacy refs gone.

### W4 — generic-kernel driver crates (the boot-path rework)
Rework `nros-board-{freertos,threadx,nuttx}`'s `run<B, F, E>(config, f)` to the
`RuntimeCtx` model + migrate their per-board overlays
(`mps2-an385-freertos`, `nuttx-qemu-{arm,riscv}`, `threadx-{linux,qemu-riscv64}`,
`orin-spe`). The largest wave; each kernel lands independently.
- **Acceptance:** each kernel family's QEMU/e2e fixture green on the new family;
  legacy `run<B>` deleted per crate as it migrates.

### W5 — remaining + `cffi`
`zephyr` (owns `main`, `NetworkWait`-only), `nros-board-cffi`, and any straggler;
migrate the `nros-tests` bins/tests that import the legacy family.
- **Acceptance:** no crate outside `nros-board-common` references
  `board_init`; `just ci` green.

### W6 — delete legacy + gate
Delete `nros-board-common::board_init` (+ re-exports). Add a lint / grep gate
(the `check-no-*` family precedent) that fails on a new
`nros-board-common::board_init` or board-trait `NodeRuntime` reference, so the
boundary can't re-rot.
- **Acceptance:** the module is gone; the gate is wired into `just ci`; a
  reintroduction fails the check.

## Done when

Every board crate implements exactly one board-trait family
(`nros-platform::board`); `nros-board-common::board_init` + the `NodeRuntime`
alias are deleted; a lint prevents their return; `just ci` green throughout. A
new board author has one `BoardInit` to implement, config flows through
`RuntimeCtx`, and there is no name collision.

## References
- Issue: #0243 (the audit + the finish-vs-retire decision).
- Design: RFC-0034 (platform layer split — the boundary this converges on).
- Prior: phase-212.N.1 (landed the `nros-platform::board` traits + config→RuntimeCtx).
- Related: phase-230 (allocator/scalar-ABI funnel — a DIFFERENT RFC-0034 work stream).
