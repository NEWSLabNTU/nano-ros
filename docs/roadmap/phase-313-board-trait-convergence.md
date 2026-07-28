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

## Fix method (explored 2026-07-28) — the blocker dissolves into verifiable increments

Investigation of the actual coupling turned the "unverifiable big-bang" fear into
a bounded, per-board-verifiable job. Key findings:

- **The LIVE framework boot path is ALREADY migrated + verified.** `nros::main!`
  emits `<Board as nros_platform::board::BoardEntry>::run(...)` / `run_tiers`
  (`nros-macros/src/lib.rs:35`, `main_macro.rs`) — the NEW family. Every existing
  QEMU/e2e fixture already exercises it. The migration does NOT move this path.
- **What retires is a PARALLEL legacy entry surface**, not shared boot infra: the
  boards' standalone `pub fn run(Config, closure)` (→ `nros_board_common::run<B>`)
  + the ~5 smoke/logging bins that call it
  (`packages/testing/nros-tests/bins/logging-smoke-*`,
  `nros-smoke/esp32s3-board-bringup`) + the `nros-board-cffi` C-export macro.
- **Config is BUILD-TIME**, not live runtime input: consumers pass
  `Config::default()` or `Config::from_toml(<const CONFIG>)`. So the `type Config`
  split is faithful — hardware defaults become board-crate `const`s, the few
  runtime knobs flow through `RuntimeCtx`; no behavior invented.
- **`threadx-linux` is HOST-runtime-verifiable** (runs as a Linux process; has a
  host smoke test), so its family-driver rework can be fully confirmed WITHOUT
  QEMU — the ideal first fully-verified increment.

**Method — parallel, incremental, verifiability-ordered, host-first:**

1. Keep legacy alive per-crate; migrate ONE board/family at a time.
2. **Order by how it is verified**, not by size:
   - **threadx-linux + the `nros_board_threadx` family driver** FIRST — the
     `run<B>` rework is host-runtime-confirmable (no QEMU). Proves the recipe
     end-to-end with full verification.
   - then each QEMU-lane board/family one at a time, gated on its own lane
     (`freertos`→mps2-an385-freertos, `nuttx`→qemu-arm/riscv, `threadx`→qemu-riscv64,
     esp32-qemu).
   - then direct-exec boards (stm32f4, esp32s3), then `cffi` (the one real API
     change — the C-export macro moves from config-carrying `run(cfg, closure)` to
     `run(setup)`), then W6 (delete `board_init` + lint gate).
3. Per board: delete `pub fn run(Config, closure)` + `impl
   nros_board_common::BoardInit { type Config; init_hardware(&cfg) }`; fold the
   init logic into the new `nros_platform::board::BoardInit::init_hardware()`
   reading the board's baked `Config` internally; migrate that board's smoke bin
   to `<Board>::run(|runtime| …)`.
4. Verification per step: per-manifest `cargo check` for the target + a
   runtime confirm on the board's own lane (host for threadx-linux; QEMU
   otherwise). The live `nros::main!` path never moves, so regression risk is
   contained to the parallel API being retired.

This supersedes the size-ordered W3–W6 sketch below; the verifiability-ordered
sequence above is the plan of record.

## W-threadx attempt (2026-07-28) — runtime verification found a REAL gap; reverted

Attempted the first increment host-verified. It EXPOSED a gap the compile-check
would have missed, exactly why the host-first ordering matters:

- Deleted threadx-linux's legacy `board_init` impls + free `run`, migrated
  `logging-smoke-threadx-linux` to `<ThreadxLinux as
  nros_platform::board::BoardEntry>::run(|_rt| …)`. It COMPILED + linked clean.
- The host smoke test then FAILED: `Executor::open failed:
  Transport(ConnectionFailed)`, and the closure's log output never appeared.

**Root cause — the legacy `run` and the new `BoardEntry::run` are NOT
equivalent.** The new family driver's `run_entry`
(`nros-board-threadx/src/entry.rs:453`) opens a full `Executor::open(&exec_cfg)`
session BEFORE calling `setup`, and aborts (prints "Executor::open failed") when
it can't connect. The legacy `nros_board_threadx::run<B>(config, closure)` the
logging smokes use does NOT open a session — it boots the kernel + runs the
closure. So the legacy `run(Config, closure)` served TWO roles: full-app boots
AND **lightweight, no-session logging/init-only fixtures**. `BoardEntry::run`
only covers the first.

Reverted (the tree is back to green). The finding: **retiring the legacy `run`
needs a lightweight, no-session new-family entry** for the logging/init-only
smoke bins (`logging-smoke-*`, `nros-smoke/*-board-bringup`), OR those smokes must
be migrated to a session-backed setup (with a router). This is a small design
addition, not a per-board config split — and it BLOCKS every family's migration
the same way (freertos/nuttx have the identical logging-smoke pattern).

## W0 — lightweight no-session entry — DONE (2026-07-28, host-verified)

Added `nros_board_threadx::run_bare<B, C, F, E>(config, setup)` +
`app_task_entry_bare` (`nros-board-threadx/src/entry.rs`): the same pre-kernel
boot as `run_entry` (banner, `init_hardware`, network config, `tx_kernel_enter`)
but the app-thread callback runs a NULLARY closure (`FnOnce() -> Result`) with
NO `Executor::open` — for logging/init-only fixtures that open no ROS session.
`ThreadxLinux::run_bare` wraps it (`Config::default()` + log-writer seed).
`logging-smoke-threadx-linux` migrated to it. **Verified:** the host smoke test
`logging_smoke_harness_captures_stderr` is GREEN (every severity + the
"Application completed successfully." banner), where `BoardEntry::run` failed
`Transport(ConnectionFailed)`.

## W-threadx (threadx-linux) — DONE (2026-07-28, host-verified)

With `run_bare` in place, deleted threadx-linux's legacy residual: the free
`node::run`, `impl nros_board_common::{BoardInit, BoardPrint, BoardExit}` (the
new `nros_platform::board` impls carry the bodies now), and the `pub use
node::run` / legacy imports. Kept `ThreadxConfig` (a config trait the new family
driver consumes, not `board_init`). Updated the `nros-board.toml` link-pin
comment to name the new entries. **threadx-linux now implements ONE board-trait
family.** Verified: threadx-linux builds, the smoke test stays green, the family
driver's legacy `run<B>` (still used by threadx-qemu-riscv64) is untouched.

## Kernel families — DONE (2026-07-28, per-lane verified)

The three kernel-spawn families are fully off legacy `board_init`, following the
W0 `run_bare` template. Each: add the family driver's no-session `run_bare` (or
delete dead legacy for the boards that never consumed it), migrate its
smoke/bringup bin, delete the board's legacy `board_init` impls + free `run`,
green its lane.

- **threadx** — threadx-linux (W0), threadx-qemu-riscv64 (`run_bare` reused the
  family driver; `logging_smoke_threadx_riscv64_emits_every_severity` PASS on the
  riscv64 QEMU lane). Family fully migrated.
- **freertos** — added `nros_board_freertos::run_bare` + `app_task_entry_bare`
  (shared boot bringup, nullary closure, no `Executor::open`) + `Mps2An385::run_bare`;
  deleted mps2-an385's legacy impls + free `run`/`init_hardware` + the dead
  `reference-mps2` re-export. `logging_smoke_freertos_mps2_emits_every_severity`
  PASS on the MPS2-AN385 QEMU lane.
- **nuttx** — deletion-only (NuttX is shell-dispatched POSIX; the `logging-smoke-*`
  fixtures run their plain `fn main` via `nsh_main`, never consumed the family
  `run`; `reference-qemu-arm` was never enabled). Dropped `run_generic<B>` + the
  `nros_board_common::BoardInit` re-export + the `reference-qemu-arm` re-export;
  deleted qemu-arm/qemu-riscv legacy impls + free `node::run`.
  `logging_smoke_nuttx_qemu_arm_emits_every_severity` PASS; qemu-riscv reaches the
  unchanged `QemuRvVirt::run_tiers` via the ws-realtime-rust lane.

## Direct-exec (esp-hal) — DONE for the no-session boards (2026-07-28)

The esp32 boards used `nros_board_common::run<B>` for INIT-ONLY fixtures (no
session), so a free `run_bare(config, setup)` — Config-driven `node::init_hardware`
+ log writer + nullary closure + no-exit spin, mirroring the kernel `run_bare` —
replaces the legacy path cleanly.

- **esp32-qemu** — added free `run_bare` (a free fn, NOT a method on the
  `rmw-zenoh`-gated `Esp32QemuEntry`, so the no-RMW logging smoke reaches it);
  deleted legacy `run` + `node::Esp32Qemu` ZST + its `board_init` impls; migrated
  `logging-smoke-esp32-qemu`. Also fixed a latent #292 riscv32imc bug
  (`entity_counter` → `portable_atomic::AtomicU32`; `core`'s has no `fetch_add`
  on no-CAS ISA). `logging_smoke_esp32_qemu_emits_every_severity` PASS.
- **esp32s3** — same shape; only consumer is the init-only bringup smoke.
  **UNVERIFIED on this host** (no Xtensa `esp` toolchain; bringup is a manual /
  physical-HW fixture in no CI lane). Mechanically identical to esp32-qemu.

## Remaining (the harder tail — SESSION path, not run_bare)
1. **mps2-an385 + stm32f4 direct-exec `run`** still exists, consumed by
   **session-opening** binaries: `nros-bench/large-msg-baremetal` (uses
   `nros_board_mps2_an385::run` then `Executor::open`) and
   `examples/stm32f4/rust/talker` (verify). These are NOT init-only, so they do
   not map to `run_bare` — migrate them to the boards' existing
   `<Board as nros_platform::BoardEntry>::run` (the RuntimeCtx session path;
   both boards already impl it, entry.rs), restructuring each closure from
   manual `Executor::open` to `register()`-against-`RuntimeCtx`. THEN delete the
   legacy `run` + `node::{Stm32F4,Mps2An385}` ZSTs + `board_init` impls.
   NB `logging-smoke-mps2-baremetal` needs NO migration — it boots via
   `cortex_m_rt::entry` + direct `nros_log`, never the board `run`.
2. Then `cffi` (the C-export macro's config-carrying `run(cfg, closure)` → the new
   form; 7 refs), then W6 (delete `nros-board-common::board_init` + the lint gate).

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
