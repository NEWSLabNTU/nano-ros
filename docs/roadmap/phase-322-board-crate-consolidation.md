# Phase 322 — Board crate consolidation

**Status (2026-07-31): drafted, DEFERRED.** Deliberately sequenced after
[phase-320](archived/phase-320-board-support-tiers.md) (support tiers) and
[phase-321](archived/phase-321-package-org-cuts-and-reorg.md) (cuts + reorganization).

**Why deferred.** This is the largest and riskiest of the three: it rewrites
board crates that several platforms boot from, and every merge needs its
platform's runtime lane re-run to prove nothing regressed. The tier work and the
directory cuts deliver most of the value at a fraction of the risk, and neither
depends on this. Do those first.

**Why it is still worth doing.** The forks have already rotted, twice, in ways
nobody noticed — see the evidence below. Every month this waits, the duplicated
copies drift further apart, and the drift is only ever discovered at runtime.

**Prerequisite.** W1.g (a shared runtime `Config`) is the root cause. Without it
the merged crates will simply re-fork, so it is not optional cleanup — it is the
thing that makes the rest durable.

---

## W1 — Board crate merges

Several boards are near-literal forks. **The empirical case for merging is that
the forks have already rotted, twice, in ways nobody noticed:**

- `packages/platform/nros-platform-esp32s3/src/memory.rs` is **missing the #190
  `foreign_free_count()` fix** its C3 twin has (verified: 2 occurrences in
  `nros-platform-esp32-qemu/src/memory.rs`, 0 in the S3 copy).
- `nros-board-rtic-mps2-an385`'s `qemu_config()` silently diverges from
  `nros-board-mps2-an385`'s `base_config()` on default IP and locator
  (`10.0.2.10` vs `192.0.3.10`) — invisible until runtime.

**The objection that normally blocks these merges does not apply here.** No board
crate emits a reset vector: `#[entry]` is emitted by `nros::main!()` in the Entry
package and dispatched off a **deploy-key string match**
(`nros-macros/src/main_macro.rs:2888,2931`), not off crate identity. Board crates
only re-export the attribute. Verified: `rg '#\[entry\]' packages/boards/` → no
hits. And the board-key→crate mapping is already a table
(`nros-orchestration-ir/src/lib.rs:73`, its own doc calls it "the single source
of truth"), so a merge costs 1–3 string edits and breaks no Entry package.

| Cluster | Now | After | Verdict | Code lines removed |
| --- | --- | --- | --- | --- |
| NuttX qemu arm/riscv + façade | 3 | 1 | **MERGE** — highest confidence | ~1300 |
| ESP32 platform crates | 4 | 3 | **MERGE platforms only** | ~600 |
| STM32F4 plain/rtic/embassy | 4 | 2 | **MERGE** behind features | ~420 |
| MPS2-AN385 | 5 | 4 | **MERGE-PARTIALLY** (`rtic` feature) | ~150 |
| host native/posix | 2 | 1 | **MERGE** | ~114 |
| ThreadX | 3 | 3 | **KEEP SEPARATE** | ~50 |
| infra | 4 | 2–3 | delete `bare-metal` | ~230 |

~27 board crates → ~19, ~2900 code lines deleted.

- [ ] **W1.a** **NuttX — merge `nros-board-nuttx-qemu-{arm,riscv}` + absorb the
      façade.** Verified byte-identical between the two crates: `c/nuttx_run_tiers.c`
      (587 lines), `src/config.rs` (261), `src/entry.rs` (43),
      `c/nuttx_builtins_stub.c` (35) — **926 duplicated lines**. After filtering
      ZST renames and doc rewording, the *entire* semantic difference is one line:
      `SLIRP_DEFAULT_IP` `[10,0,2,30]` vs `[10,0,2,15]`, which the existing
      `DeployOverlay` machinery already overrides. Architecture is already
      externalised into `NUTTX_CROSS` / `NUTTX_PLATFORM_CFLAGS` env, so the crate
      fork buys nothing. Different target triples are not a blocker — one crate
      builds for many. Keep both ZST names as type aliases.
- [ ] **W1.b** **ESP32 — merge the two platform crates** into `nros-platform-esp32`.
      Per-file diff: `libc_stubs.rs` (283 lines), `clock.rs`, `random.rs`,
      `sleep.rs`, `timing.rs` differ by **zero** lines; the only semantic
      difference in 802 lines is `PlatformCriticalSection` (~25 lines, RISC-V
      `csrrci` vs Xtensa `rsil`) — textbook `#[cfg(target_arch)]`. **Keep the two
      BOARD crates separate**: different SoC, different transport (OpenETH+smoltcp
      vs serial-only), different target triples, and S3 has no `BoardEntry` at all.
      One wrinkle: `#![feature(asm_experimental_arch)]` must become
      `#![cfg_attr(target_arch = "xtensa", …)]`.
- [ ] **W1.c** **STM32F4 — one crate, features `rtic` / `embassy`** (mutually
      exclusive, `compile_error!` guard as at `nros-board-mps2-an385/src/node.rs:15`).
      `rtic-stm32f4`'s bringup is two lines delegating to the plain crate. Checked
      and cleared: same target triple, no `panic_handler` in any of the three, the
      `critical-section` feature is a no-op for stm32f4. **One real conflict**:
      `nros-board-embassy-stm32f4` enables `embassy-stm32`'s `memory-x` feature,
      which emits its own `memory.x` while `nros-board-stm32f4/build.rs` writes
      one too — merged, the linker picks by search order, i.e. a non-deterministic
      memory map. Drop `memory-x`; the board's `stm32f4.x` is richer anyway
      (defines `CCMRAM`, `_heap_start`/`_heap_end`). **Alternative honest
      verdict:** `embassy-stm32f4` is a self-declared skeleton that drops the
      peripheral handle (`let _p = embassy_stm32::init(…)`) and has no transport
      bringup — delete it and re-add as a feature when someone wires `embassy_net`.
- [ ] **W1.d** **MPS2-AN385 — fold `rtic-mps2-an385` into `mps2-an385`** behind an
      `rtic` feature; it already depends on the base crate and calls its
      `init_hardware` / `exit_success` / `enable_wfi_idle`. Deletes ~120 duplicated
      lines including a second `mask_to_prefix` and the divergent config defaults.
      **Keep `-freertos` separate** — different linker script, 727-line
      `startup.c`, its own `#[panic_handler]`, a 316-line build.rs compiling
      FreeRTOS+lwIP, and a different `nros_platform_*` symbol provider. Keep
      `mps2-an385-pac` (RTIC's `#[rtic::app(device = …)]` needs a nameable path).
- [ ] **W1.e** **host — fold `nros-board-native` into `nros-board-posix`.**
      `native`'s own doc says it delegates "one-for-one" to `PosixBoard` and that
      "there is nothing exotic about the 'native' target"; `board_path_for` already
      maps **both** keys to the same ZST, so `nros-board-posix` is never named by
      any generated entry. The only addition is a ~5-line `__FORCE_LINK_ZENOH`
      static → a feature. Pure ceremony: a crate existing to satisfy a naming spec.
- [ ] **W1.f** **ThreadX — KEEP SEPARATE, and copy its pattern.** Hard reasons:
      distinct `#[panic_handler]` ownership, distinct startup/trap/syscall C,
      hosted-vs-bare-metal link model, and different network drivers (AF_PACKET
      over veth vs NetX-Duo/virtio-net). This cluster is the model the others
      should follow: `nros-board-threadx` is a **real family driver** whose
      1120-line generic `entry.rs` both boards call into. Same for
      `nros-board-freertos`, where the MPS2 overlay is `pub use
      nros_board_freertos::Config;` and carries zero config code.
- [ ] **W1.g** **Root cause, worth more than any individual merge: there is no
      shared runtime `Config`.** Phase-313 deleted `nros-board-common`'s
      `board_init` module, leaving it a *build-helper* library (2180 of 2252 code
      lines behind `cfg(feature = "build-helpers")`). Result: **12 hand-rolled
      `Config` structs**, at least nine carrying the identical
      `{mac, ip, netmask, gateway, locator, domain_id}` core, and the
      `DeployOverlay`→`Config` merge written out at least four times. Add a shared
      `BaseConfig` + overlay-merge that boards extend. Without it the merges above
      will re-fork.
- [ ] **W1.h** `nros-board-bare-metal` — 161 lines of which **135 are doc comment**
      describing a `DirectExec` family driver **no board opted into**; `mps2-an385`,
      `stm32f4` and `esp32-qemu` each hand-roll `BoardEntry::run` instead. Either
      delete it (phase-321 W1.g) or — higher value — make those three implement `DirectExec`,
      which absorbs the W1.d duplication too.

---

## Acceptance

- Each merged cluster's platform lane is re-run and green **before** the merge
  commit lands — these boards boot, so a compile is not evidence.
- `board_path_for` still resolves every previously-valid deploy key; old ZST
  names survive as type aliases so no Entry package breaks.
- The `#190` `foreign_free_count` drift and the `rtic-mps2-an385` config-default
  divergence are both gone, with a test that would have caught either.
- A shared `BaseConfig` exists and the merged boards use it (W1.g), so the
  duplication cannot silently return.
