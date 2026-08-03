# Phase 337 — the supported board matrix, one board at a time

**Implements:** [RFC-0064](../design/0064-board-support-organization.md) revision 3
("The supported matrix", "Three layers, not two", "Target state").
**Absorbs:** [phase-322](phase-322-board-crate-consolidation.md) W1.a / W1.d / W1.e /
W1.g / W1.h (the board-crate merges; 322 stays the record of the *measurements*).
**Closes:** issue #405 (in W3).
**Touches:** RFC-0049 (the config ladder — W1.a extends it to the build block),
RFC-0051 / phase-329 (the cell tables every wave edits), RFC-0012 (BSP integration).

**Status.** DRAFT — not started.

---

## Why this phase is shaped like this

The target is RFC-0064's: **27 board crates → 16, 344 fixture rows → 336,
202 cells → 208** (Runtime 174 → 177). That is a lot of surface, and the tempting
shape is one big migration commit. Do not.

**Each wave is ONE board family plus its fixtures plus its tests, and lands on its
own.** Two reasons, both practical:

1. **Blast radius.** A board wave that breaks something breaks one platform's
   lane, which one `just <platform> build-fixtures` + that platform's cells will
   show. A combined migration fails as "the sweep is red" with N candidate causes,
   which is how phase-330's validation burned 26 rounds.
2. **Parallel agents.** Waves touch disjoint board crates, so two agents can hold
   two waves at once. What they *do* share is a handful of registry files (below) —
   line-level conflicts, resolvable on rebase, the same as `docs/issues/README.md`.

**The user reserves the fixture/test set.** W2 and W7 encode matrix *decisions*
(add a witness, drop boards). W3–W6, W8 are consolidations that hold regardless of
how the matrix decision lands. If the set is still under review, run W1 and W3–W6
first; they are value-positive either way.

## The wave contract

Every board wave does these six things and is not done until all six hold:

| | Step | Evidence it landed |
|---|---|---|
| 1 | Crate work — merge / extract / thin, adopting `BaseConfig` (W1.b) | line count before→after, recorded in the wave |
| 2 | Registry row — `packages/boards/board-support.toml` | `check-board-tiers` green |
| 3 | Fixture rows — `examples/fixtures.toml` + the build recipe | `just <platform> build-fixtures` green |
| 4 | Cells — `packages/testing/nros-tests/src/matrix.rs` | `matrix_fixture_coverage` green **both directions** |
| 5 | Runtime proof — that platform's lane, on freshly rebuilt fixtures | named test list passes |
| 6 | Doc — the board's book page + RFC-0064's target table row ticked | — |

**Scope fence for every wave:** touch only your board's crates, your platform's
fixture rows, your platform's cells, and your one row in each shared registry. A
wave that edits another board's crate has escaped its blast radius — split it.

**Do not** rebuild the whole fixture set to verify a wave. Build your lane
(`just build-test-fixtures lane=<lane>` narrows both the module fan-out and the
manifest rows since #393), and remember any rebase re-stales every prebuilt
fixture (the mtime treadmill).

### Shared files every wave touches (expect rebase conflicts here)

| File | What a wave changes |
|---|---|
| `packages/boards/board-support.toml` | one row |
| `packages/testing/nros-tests/src/matrix.rs` | that platform's cells (+ `PlatformId` for W2/W8) |
| `examples/fixtures.toml` | that platform's rows |
| root `Cargo.toml` | workspace members, when a crate is added or removed |
| `packages/core/nros-orchestration-ir/src/lib.rs` | `board_path_for` board-key table |
| `nros-sdk-index.toml` | `[board.*]` alias, if the board has one |

Write full logs to files when running background sweeps — `| tail` hides the real
error.

---

## W1 — Prerequisites (ADDITIVE ONLY; no board changes shape)

The point of W1 is that the cross-cutting work becomes *additive*, so each board
adopts it inside its own wave instead of one flag-day commit.

- [ ] **W1.a — Finish the Cortex-M4F/M7 unblock.** RFC-0064 sequencing step 0
      fixed the config half (`config/freertos-lwip/nros-platform.toml` now lists
      `arch = ["cortex-m3", "cortex-m7"]`), but
      `packages/boards/nros-board-freertos/build.rs:273-287` still **hard-panics**
      for any `thumb*` target that is not `thumbv7m` unless `FREERTOS_CFLAGS` is
      set. Every industrial FreeRTOS board this phase aims at is Cortex-M4F/M7
      (S32K344 is M7). Derive the cflags from the `[arch.*]` profile the platform
      config already carries rather than panicking; keep an explicit
      `FREERTOS_CFLAGS` as the rung-1 override.
      *Verify:* a compile-only `thumbv7em-none-eabihf` check in the embedded lane.
      *Blast radius:* one `build.rs`. No board migrated. **Do this first** — it is
      small and it is what makes the whole matrix reachable by real users.
- [ ] **W1.b — Shared `BaseConfig` in `nros-board-common`, adopted by NOBODY yet.**
      phase-322 W1.g measured **12 hand-rolled `Config` structs**, ≥9 carrying an
      identical `{mac, ip, netmask, gateway, locator, domain_id}` core, with the
      `DeployOverlay`→`Config` merge written out at least four times. Add the
      shared type + overlay-merge **additively**; each board wave migrates its own
      `Config` onto it as wave step 1. Without this the merges re-fork — with it as
      a flag day, the blast radius is 12 crates at once.
- [ ] **W1.c — Tier registry row key → `(crate, matrix_platform)`.**
      `board-support.toml` keys tier by crate and its gate asserts every board
      directory appears exactly once. After W3 a single `nuttx-qemu` crate serves
      arm (tier 1) and riscv (tier 2); after W9 one `nros-board-zephyr` serves
      three tiers. Make `matrix_platform` a list (or key rows by the pair) and
      update `scripts/check-board-tiers.py` so its predicates run per *witness*,
      not per crate. **Blocks W3.**

## W2 — Zephyr QEMU Cortex-M: the new witness (purely additive)

Touches **no existing board**, so it collides with nothing and can run in parallel
with any other wave. It also delivers the phase's clearest user-visible win.

**Why:** all 28 Zephyr runtime configs are `native_sim/native/64`, and
`cmake/zephyr/native-sim-line-*.conf` sets `CONFIG_NET_SOCKETS_OFFLOAD=y` with
`CONFIG_ETH_NATIVE_TAP=n`. So every Zephyr test bypasses Zephyr's own IP stack,
on a 64-bit host. Zephyr's in-kernel net stack, a real driver, and 32-bit pointer
width have **never** run — and `nros-platform-zephyr/src/net_wait.c:53`'s
`#ifdef CONFIG_BOARD_NATIVE_SIM` else-branch has never executed in CI.

- [ ] **W2.a — Settle the board.** RFC-0064 `[OPEN]`: which QEMU-able Zephyr board
      carries a usable Ethernet driver — `mps2/an385` via `smsc911x`, or SLIP/TAP
      on `qemu_cortex_m3`. Not answerable from this tree; needs a Zephyr checkout.
      Prefer `mps2/an385` if it works: `nros-tests/src/qemu.rs:242` already boots
      `-machine mps2-an385 -nic user,model=lan9118` for two other families, so the
      runner is reuse, not new code.
- [ ] **W2.b — The conf bundle.** `boards/<board>.conf` (+ `.overlay` if needed)
      beside the existing `prj.conf`s — the same mechanism the 28 native_sim
      configs and the FVP board already use. **No new crate** (see W9).
- [ ] **W2.c — `PlatformId::ZephyrQemuCortexM`** — enum arm, `index()` band,
      `fixture_tokens()`. The injectivity gate re-proves port/domain
      collision-freedom automatically.
- [ ] **W2.d — Cells:** pubsub × {rust, c, cpp} × zenoh as `Runtime` (3);
      service and action as `BuildOnly` with the reason string until they run (6).
- [ ] **W2.e — Fixture coverage:** join the west-lane exemption in
      `tests/matrix_fixture_coverage.rs::every_runtime_cell_has_a_fixture_row`,
      naming the new board. **Adds zero `fixtures.toml` rows** — the west leaves
      lane (`scripts/build/zephyr-fixture-leaves.sh`) carries its own staleness
      signature.
- [ ] **W2.f — Runner + lane wiring**, modelled on the existing networked-QEMU
      helper.
      *Verify:* the three new Runtime cells deliver; native_sim stays green.

## W3 — NuttX: two crates → one (closes #405)

Both upper layers come from upstream NuttX — kernel *and* arch ports, with the
link list **discovered** by scanning `$NUTTX_DIR/staging` for `lib*.a` rather than
hardcoded — so the in-tree crates are pure board overlay. Their `build.rs` bodies
are literally the same four calls.

**Blocked by W1.c** (one crate, two tiers).

- [ ] **W3.a** Adopt `BaseConfig` (W1.b) in both crates first, so the merge is a
      move rather than a reconciliation.
- [ ] **W3.b** Move the **1059 byte-identical lines** into the existing
      `nros-board-nuttx` (both boards already depend on it):
      `c/nuttx_run_tiers.c` (587), `src/config.rs` (261), `src/node.rs` (133),
      `src/entry.rs` (43), `c/nuttx_builtins_stub.c` (35).
- [ ] **W3.c** Collapse to one board crate; the per-arch delta stays **data**:
      `nuttx-config/defconfig`, `<arch>-nuttx-toolchain.cmake`, the nine `NUTTX_*`
      values in `nros-board.toml [env]`, `nros-nuttx-ffi/.cargo/config.toml`
      (+ the riscv target JSON). Keep both ZST type names as aliases — phase-322
      verified the entire semantic difference is one `SLIRP_DEFAULT_IP`, which
      `DeployOverlay` already overrides.
- [ ] **W3.d** Registry rows per W1.c: one crate, two witnesses, two tiers.
- [ ] **W3.e** Fixtures: the 30 `nuttx` + 4 `nuttx-riscv` rows keep their
      coordinates; point the build recipes at the merged crate.
- [ ] **W3.f — Close issue #405 here.** `lane-coords` maps `nuttx-riscv,c,zenoh`
      to the `nuttx` module, but `just nuttx build-fixtures` builds only the arm
      side — the riscv workspaces are separate `full-matrix` recipes, so tier 2
      demands a fixture its own builder cannot produce. Fix it in this wave: the
      nuttx stage honours its coords and appends the riscv recipes (serial, same
      stage — they share one kernel tree). Add the coverage check issue-0196 asks
      for: what the gate demands must be producible by the recipes the lane runs.
      *Verify:* `just build-test-fixtures lane=tier2` then `just ci-matrix`, with
      no manual `build-riscv-c-workspaces` step.

## W4 — ThreadX: extract the arch port (do NOT merge the boards)

Layer 1 already shipped: `nros-board-threadx/src/entry.rs` (1120 lines) is generic
over the board — `run_entry::<MyBoard, Config, F, E>`, internals bounded
`B: BoardPrint + BoardExit` — and both boards already delegate to it.

The asymmetry is layer 2. `nros-board-threadx-qemu-riscv64` is 3598 lines of which
**~1250 are an arch port**: `tx_thread_{schedule,context_save,context_restore}.S`
(1002) + `config/tx_port.h` (252), a modified copy of
`third-party/threadx/kernel/ports/risc-v64/`. The fork is legitimate — upstream
types `ULONG` as 8 bytes, NetX Duo's packet code does `ULONG *` arithmetic
assuming 4-byte words, and retyping shifts every `TX_THREAD` offset.
`threadx-linux` has no `.S` because upstream's Linux port already uses 4-byte
`ULONG`.

**So merging the two boards is the wrong cut** — it would `cfg`-gate RISC-V
assembly into a crate that also serves Linux.

- [ ] **W4.a** New layer-2 unit (`nros-board-threadx-port-riscv64`, or a `port-*`
      feature of the family crate) holding the three `.S` files + `tx_port.h`,
      with the upstream provenance and the `ULONG` rationale carried over verbatim.
- [ ] **W4.b** Thin `threadx-qemu-riscv64` to an overlay: `Config` defaults on
      `BaseConfig`, the four trait impls, console/exit/panic, virtio-net bring-up.
- [ ] **W4.c** Thin `threadx-linux` likewise (`src/config.rs` is already 84 %
      identical to its sibling — 55 diff lines of 339).
- [ ] **W4.d** **Behaviour-neutral wave:** the 42 + 23 fixture rows and the 18 + 12
      cells do not change. If a cell moves, the wave did more than it should.
      *Verify:* both ThreadX lanes, on rebuilt fixtures.

## W5 — FreeRTOS: template the per-board files

The irreducible per-board delta measures at **~60–80 lines** (vector table 19,
memory map 3 numbers, CPU clock 1, cflags 1, netif registration 4, driver
reference). The crate is ~1600 lines plus a 727-line `startup.c`. The gap is all
mechanical.

- [ ] **W5.a** Hoist the config headers to shared defaults: `config/lwipopts.h`
      (133) and `config/arch/cc.h` (55) have **zero** board-specific content;
      `config/FreeRTOSConfig.h` (111) has two (`configCPU_CLOCK_HZ`,
      `configPRIO_BITS`). Board supplies the values, not the files.
- [ ] **W5.b** Retire the `startup.c` shadow copy. ~575 of its 727 lines are a live
      duplicate of `nros-board-freertos`'s `network_glue.c` + `freertos_hooks.c` +
      the board's own `board_mps2.c`; it survives only because the CMake lane
      compiles it while the cargo lane compiles `board_mps2.c`. One source, both
      lanes.
- [ ] **W5.c** Drop `nros_freertos_diag_network` (~180 lines of raw LAN9118 CSR
      pokes and hand-assembled ARP frames, duplicated into **both** C files and
      called on no working path), or move it behind an explicit debug feature.
- [ ] **W5.d** De-duplicate `build.rs`: `configure_arm_cm3` / `add_freertos_includes`
      / `add_lwip_includes` are byte-identical to the shared crate's, and
      `emit_nros_app_config` is 57 lines of hand-maintained C-string mirror of
      `nros_board_freertos::Config::default()` — fold it onto `BaseConfig`.
- [ ] **W5.e** Linker script → template + the three numbers (`FLASH` origin/length,
      `RAM`, `_estack`).
- [ ] **W5.f** **Prove the claim**: write the skeleton of a second FreeRTOS board
      and show it is ~80 lines. It does not have to ship — the number is the
      deliverable, and it is what tells an S32K344 user whether this path is real.
      *Verify:* the freertos lane; MPS2 artefacts byte-comparable where the change
      was meant to be a pure move.

## W6 — Bare-metal MPS2: fold RTIC in as a feature

- [ ] **W6.a** Fold `nros-board-rtic-mps2-an385` into `nros-board-mps2-an385`
      behind an `rtic` feature — it already depends on the base crate and calls its
      `init_hardware` / `exit_success` / `enable_wfi_idle`. Deletes ~120 duplicated
      lines including a second `mask_to_prefix` and the **divergent config defaults**
      (`10.0.2.10` vs `192.0.3.10`) that are invisible until runtime.
      *Verify:* the `qemu` lane's baremetal cells.

## W7 — Demotions (GATED on the matrix decision)

Do not start until the fixture/test set is confirmed. Both removals are free on
the test axis, which is what makes them safe once decided.

- [ ] **W7.a** `stm32f4` + `rtic-stm32f4` leave the matrix — **0 Runtime cells**
      today, so no runtime coverage is lost; 3 BuildOnly cells and 8 fixture rows
      go with them. Convert to the book's **worked customization example**: the
      same hardware, reached through RFC-0064's ladder instead of an in-tree crate
      nobody can boot in CI. The demotion is only honest if the tutorial lands with
      it.
- [ ] **W7.b** Delete the scaffolds — `embassy-stm32f4`, `esp32s3`,
      `s32z270dc2-r52` contribute **zero cells of any tier**, so removal is
      provably free here. **`orin-spe` first needs untangling**: it is load-bearing
      as a pseudo-platform in link-feature selection
      (`nros-zpico-build/src/runner.rs:225,419-420,528-529` +
      `config/orin-spe/nros-platform.toml`). Untangle, then delete — never blind.
- [ ] **W7.c** Delete `nros-board-bare-metal` (phase-322 W1.h): 161 lines of which
      135 are a doc comment describing a family driver no board opted into.

## W8 — linux: merge `native` + `posix`, retire `native`

Widest wave (187 fixture rows), mechanically safe, and self-gating. **Do it last**,
when every other wave has settled, so its rename does not churn under them.

**The naming decision** (RFC-0064): platform stays `posix` — a genuine portability
seam, and the platform layer names software-stack facts (RFC-0049's duty rule).
The board becomes `linux` — the board layer names what we actually claim, and a
tier-1 promise means "`just ci` exercises it", which only Linux does (all 19 CI
jobs are `ubuntu-*`; `nros-platform-posix/src/timer.c:72` calls `timer_create`
with no fallback and macOS has no POSIX timers).

- [ ] **W8.a** Merge `nros-board-native` into one `nros-board-linux`.
      `board_path_for` already maps **both** keys to the same ZST
      (`nros-orchestration-ir/src/lib.rs:78`), so `nros-board-posix` (549 lines) is
      named by no generated entry — this is ceremony removal, not a behaviour
      change.
- [ ] **W8.b** `PlatformId::Native` → `Linux`: enum, `index()` band,
      `fixture_tokens()` (`native` → `linux`). `fixture_token_mapping_round_trips`
      gates that the rename landed in both directions.
- [ ] **W8.c** The 187 `platform = "native"` fixture rows.
- [ ] **W8.d** Board-key tables + descriptors (`packages/boards/posix/`), the SDK
      index alias, book pages.
- [ ] **W8.e** *(optional, only if a hosted non-Linux board is ever wanted)* a
      `timer_create` fallback (dispatch source or `pthread_cond` timed wait), which
      is what would let `macos`/`freebsd` join later as tier-3 boards on the
      unchanged `posix` platform.
      *Verify:* full `just ci` — this wave touches the reference platform.

## W9 — Zephyr boards stop being crates

Depends on W2 (the conf-bundle mechanism) and W1.b (shared `Config`).

- [ ] **W9.a** Fold `nros-board-fvp-aemv8r-smp` (160 lines: `boards/*.conf` +
      `.overlay` + `prj.conf` + a `Config` + `board.cmake`) into
      `nros-board-zephyr` as a conf bundle. A per-Zephyr-board *crate* is exactly
      the duplication this phase exists to remove — Zephyr already owns boot, MMU,
      net stack and driver.
      *Verify:* the FVP build-only lane still builds (the model is license-gated,
      so it stays tier 3 / 0 Runtime cells).

---

## Dependency graph

```
W1.a ─ (independent, do first)
W1.b ─┬─────────────► adopted inside W3, W4, W5, W8
W1.c ─┴─► W3
W2   ─── (independent of every other wave) ─► W9
W3, W4, W5, W6  ─── mutually independent, one board family each
W7   ─── gated on the matrix decision
W8   ─── last (widest)
```

Two agents can hold two of {W2, W3, W4, W5, W6} simultaneously. They will conflict
only in the shared registry files listed above, at one row each.

## Acceptance

- [ ] Board crates 27 → 16, with each removal traceable to a wave.
- [ ] Fixture rows 344 → 336 (only `stm32f4`'s 8 leave; 187 `native` rows renamed).
- [ ] Cells 202 → 208, Runtime 174 → 177 — the only additions are W2's witness.
- [ ] No wave's commit touches a board crate outside its own family.
- [ ] `check-board-tiers`, `matrix_fixture_coverage` (both directions) and the
      allocator injectivity gate green after every wave, not only at the end.
- [ ] A second FreeRTOS board is demonstrably ~80 lines (W5.f).
- [ ] `just ci-matrix` green after each wave that has tier-2 cells.

## Risks

- **The matrix set is still under review.** W2 and W7 encode decisions; W3–W6 and
  W8 do not. Sequence accordingly rather than blocking the whole phase.
- **Crate merging buys no CI time.** Fixture rows are what cost wall clock and the
  merges remove none. `stm32f4`'s 8 rows are 2 % of the manifest against FreeRTOS
  ~1370 s and native ~1300 s per lane. The payoff here is maintenance surface —
  duplicated lines and silent drift, the class that rotted two forks unnoticed
  (`esp32s3` missing the #190 fix, `rtic-mps2-an385`'s divergent IP defaults).
  Do not sell this phase as a speedup.
- **Parallel sessions push to `main` constantly.** Every pull re-stales prebuilt
  fixtures; rebase once, rebuild the affected family, then test without pulling
  again.
- **`orin-spe` is not the scaffold its tier says.** It reaches link-feature
  selection as a pseudo-platform. W7.b unblinds this before deleting.
