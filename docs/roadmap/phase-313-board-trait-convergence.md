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

### W2 — template: native/posix — ALREADY DONE (verified 2026-07-28)
`nros-board-native` / `nros-board-posix` already implement ONLY
`nros-platform::board` (`impl BoardInit { init_hardware() }` + `impl BoardEntry`
with `RuntimeCtx`); zero real `nros_board_common` refs, no dep. They needed no
hardware `Config` (hosted), so they were the trivial case — the recipe, but not
the hard part. (The earlier "22 crates on legacy" figure was a comment-match
false positive; see the precise audit below.)

### Precise audit (2026-07-28) — real (non-comment) legacy usage

| Bucket | Crates |
| --- | --- |
| **Off legacy** (0 real refs, no/dead dep) | native, posix, bare-metal, fvp-aemv8r-smp, mps2-an385-freertos, rtic-mps2-an385, s32z270dc2-r52, zephyr, **embassy-stm32f4** + **rtic-stm32f4** (dead dep dropped W3, below) |
| **Real legacy users** (config-flow coupled) | cffi(7), esp32-qemu(4), esp32s3(4), freertos(2), mps2-an385(4), nuttx(1), nuttx-qemu-arm(3), nuttx-qemu-riscv(3), orin-spe(1), stm32f4(5), threadx(3), threadx-linux(1), threadx-qemu-riscv64(1) — **~13**, plus `nros-board-common` (the home to delete) |

### W3 — dead-dep drops — PARTIAL DONE (2026-07-28)
`embassy-stm32f4` + `rtic-stm32f4` carried a `nros-board-common` dep with only
COMMENT references ("for symmetry"). Dropped both; each compile-checks clean for
`thumbv7em-none-eabihf`. They are now fully off legacy. The remaining direct-exec
boards (esp32*, mps2-an385, stm32f4) have REAL usage — see the blocker.

## BLOCKER (2026-07-28) — the core migration is a verification-gated boot-path + config-flow refactor

The ~13 real legacy users are NOT a mechanical trait-rename. They are coupled
through the **shared `nros_board_common::run<B: BoardInit>(config, f)` boot
path** and the **config-carrying `BoardInit { type Config; init_hardware(&cfg) }`
model**:

- `stm32f4`/`esp32`/`mps2` etc. call `nros_board_common::run::<Self>(config, f)`
  and `impl BoardInit for Self { type Config = …; init_hardware(&cfg) }`.
- `nros-board-cffi`'s C-export macro hard-depends on the legacy shapes:
  `<$ty as BoardInit>::Config`, `init_hardware(cfg)`, and the config-CARRYING
  `BoardEntry::run(cfg, closure)` — the new `run(setup)` drops the `cfg` arg.
- the kernel families (`freertos`/`threadx`/`nuttx`) own generic `run<B>` boot
  paths that thread `config` explicitly.

Migrating them means (a) rewriting the SHARED `run<B>` boot path to the
`RuntimeCtx` model — all its callers move at once, and (b) SPLITTING each board's
`type Config` into build-time consts (clock tree / MMIO base → board-crate
`const`) vs runtime knobs (→ `RuntimeCtx`), which is **runtime-behavior-affecting**
and must be validated per board on its QEMU/hardware boot. Board crates are
outside the root workspace (per-manifest `cargo check` works, but runtime
correctness of the config split does NOT show up at compile time).

Landing the shared-boot-path rework + 13 config-flow refactors WITHOUT per-board
QEMU runtime verification risks silent boot/config breakage on targets that can't
be re-tested in a single session. **This is a multi-session, verification-gated
effort — not completable in one turn.** Resuming needs: the per-kernel QEMU
lanes (`just {freertos,threadx,nuttx,esp32} …`) run per migrated board, and a
decision per board on the Config field split.

## Next steps when resumed
1. Rework the shared `nros_board_common::run<B>` boot path → the `RuntimeCtx`
   model behind a NEW-family `run(setup)` in the family-driver crates, keeping
   the legacy `run` alive until every caller moves (parallel, not big-bang).
2. Migrate one kernel family end-to-end WITH its QEMU lane green, as the real
   template (freertos → mps2-an385-freertos is the smallest).
3. Then the direct-exec boards, then cffi (the C-export macro API change), then
   the delete + lint gate (W6).

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
