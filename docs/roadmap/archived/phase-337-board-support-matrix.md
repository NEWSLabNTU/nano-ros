# Phase 337 — the supported board matrix, one board at a time

**Implements:** [RFC-0064](../design/0064-board-support-organization.md) revision 3
("The supported matrix", "Three layers, not two", "Target state").
**Absorbs:** [phase-322](phase-322-board-crate-consolidation.md) W1.a / W1.d / W1.e /
W1.g / W1.h (the board-crate merges; 322 stays the record of the *measurements*).
**Closes:** issue #405 (in W3).
**Touches:** RFC-0049 (the config ladder — W1.a extends it to the build block),
RFC-0051 / phase-329 (the cell tables every wave edits), RFC-0012 (BSP integration).

**Status.** All waves landed (2026-08-04/05): W1, W2 (a–f), W3, W4, W5, W6,
W7 (a/b/c), W8 (a/b/c/d) and W9.a. W8.e stays deliberately undone — it is marked
optional and only wanted if a hosted non-Linux board is ever added.

**One acceptance criterion is still open**, and the phase was briefly mismarked
COMPLETE while it was: `just ci-matrix`. It became live when W2 landed a tier-2
board, and attempting it uncovered two fixture-BUILD defects that made tier 2
unrunnable — issue 0439 (fixed here) and issue 0433 (upstream). See Acceptance.

The two headline results: Zephyr stopped meaning one 64-bit host config (the
`mps2_an385` witness boots, takes an IP from a real ethernet driver, and
publishes — after five defects that only a non-native_sim Zephyr could surface),
and Zephyr boards stopped being crates (`fvp-aemv8r-smp` is now a conf bundle;
18 board directories → 17). See "What is left" at the bottom for the wave-by-wave
state and Acceptance for the measured numbers.

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

### Interface with [phase-329](phase-329-test-taxonomy-completion.md) (running CONCURRENTLY)

329 is refactoring the *consumer* side of the same tables this phase writes rows
into. Neither phase blocks the other, but three specific places will collide and
one ordering is genuinely worth respecting.

**Where they touch.**

| Surface | 337 does | 329 does |
|---|---|---|
| `matrix.rs` cell rows | adds/moves/removes rows per wave (contract step 4) | leaves rows alone; changes who READS them (W1) |
| `matrix.rs` `PlatformId` | **adds** one (W2.c), **renames** `Native`→`Linux` (W8.b) | wants the per-platform edit surface shrunk (inventory: "4 places per new platform") |
| the 5 sched-dim test files | owns their board families (W3/W4/W5) | folds all 10 into ONE rstest (W2) |
| `fixtures.toml` rows | −8 (stm32f4, W7.a), 187 renamed (W8.c) | concluded rows are NOT reducible (W8 verdict) |
| `matrix_fixture_coverage` | must be green both directions per wave | ADDS G5 (files derive case sets from CELLS, W1) |

**The ordering that matters: 337 W2 and W8 want 329 W1/W4 first.**

W2.c adds a `PlatformId` and W8.b renames one. Today that means editing four
matches in `matrix.rs` (`enum` :27, `index` :60, `fixture_tokens` :92, :125) and
dodging `zephyr.rs`'s `unreachable!` arms (:129, :130, :544, :662), which panic
on an unrecognised coordinate rather than failing to compile. 329's inventory
names this exact landmine and W4 removes it. Doing W2/W8 first is not blocked —
it is just paying a cost 329 is in the middle of deleting.

The other direction is a real constraint: **once 329 W1 lands its G5 gate, a
337 wave that adds a Runtime cell must add the consumer case in the SAME wave**,
because a cell with no derived case fails G5. That tightens wave contract step 4
and is the single change most likely to surprise a wave landing mid-flight.

**The 5 sched-dim files are jointly owned.** `nuttx_{sporadic_budget,tier_priority}
_applied.rs`, `threadx_{preempt_threshold,time_slice}_applied.rs` and
`orchestration_tiers_freertos.rs` belong to W3/W4/W5 board families here and to
329 W2's 10→1 fold there. Whoever lands second rebases onto the other's shape —
in particular, if 329 W2 has landed, edit the `sched_dims` table, NOT the
per-file copies that no longer exist.

**Safe to run fully in parallel:** W3, W4, W5, W6 (board-crate work, own fixture
rows, own cell rows) and W1.b/W1.c. Those touch board crates and one row per
shared registry — the line-level conflicts the wave contract already expects.

**Two unrelated things both called W8.** This phase's W8 is the
`native`→`linux` rename; 329's W8 is the fixture-BUILD cut (whose row-dedup half
is RETRACTED, leaving only W8.d). Say "337 W8" / "329 W8.d" in commits and
issues.

**Row math is compatible.** 329 verified every fixture row is load-bearing and
that the manifest cannot be dedup'd; this phase's 344 → 336 is not dedup — it is
`stm32f4` leaving the supported matrix (W7.a), a decision, plus renames that
change no count. The two conclusions do not conflict.

**329 W8.d helps this phase's verification.** It narrows the tier-2 RUN to lane
coordinates so the BUILD can narrow too — which is exactly what "build your lane,
do not rebuild the fixture set" (above) asks a wave to do by hand today.

---

## W1 — Prerequisites (ADDITIVE ONLY; no board changes shape)

The point of W1 is that the cross-cutting work becomes *additive*, so each board
adopts it inside its own wave instead of one flag-day commit.

- [ ] **W1.a — MOVED to [phase-338](phase-338-source-portability.md) W4.a.** It is
      a source-portability defect (one source failing to reach an arch), so it
      lives with the rest of them and has one owner. Still the thing that gates
      this phase's *claim* about industrial FreeRTOS boards — track it, do not
      duplicate it. Original text kept below for context.

      ~~**Finish the Cortex-M4F/M7 unblock.**~~ RFC-0064 sequencing step 0
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
- [x] **W1.b — Shared `BaseConfig` in `nros-board-common`, adopted by NOBODY yet.**
      **LANDED 2026-08-04** (`24647274b`); adopted since by W3.a, W4.b/c, W5.d and
      W6.a.
      phase-322 W1.g measured **12 hand-rolled `Config` structs**, ≥9 carrying an
      identical `{mac, ip, netmask, gateway, locator, domain_id}` core, with the
      `DeployOverlay`→`Config` merge written out at least four times. Add the
      shared type + overlay-merge **additively**; each board wave migrates its own
      `Config` onto it as wave step 1. Without this the merges re-fork — with it as
      a flag day, the blast radius is 12 crates at once.
- [x] **W1.c — Tier registry row key → `(crate, matrix_platform)`.**
      **LANDED 2026-08-04** (`24647274b`); W3.d is its first two-witness row.
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

- [x] **W2.a — SETTLED 2026-08-04: `mps2/an385` via `smsc911x`.** Measured
      against the real Zephyr 3.7 checkout `just zephyr setup` provisions at
      `zephyr-workspace/zephyr`, which is what RFC-0064 said this question needed
      and could not have from the nano-ros tree alone:

      | Evidence | Where |
      |---|---|
      | `eth0: eth@40200000 { compatible = "smsc,lan9220"; interrupts = <13 3>; }` — present and enabled, no overlay needed | `boards/arm/mps2/mps2_an385.dts:211-218` |
      | `ETH_SMSC911X` is `default y` on `DT_HAS_SMSC_LAN9220_ENABLED` | `drivers/ethernet/Kconfig.smsc911x` |
      | `ETH_NIC_MODEL = "lan9118"` — the driver NAMES the QEMU model it expects | same file |
      | Zephyr's own runner is `qemu-system-arm -cpu cortex-m3 -machine mps2-an385` | `boards/arm/mps2/board.cmake` |
      | the harness already launches exactly that, `-nic user,model=lan9118` | `nros-tests/src/qemu.rs:242` |

      So the two candidate answers are not equal: `mps2/an385` needs **no**
      overlay, **no** SLIP, and **no** new runner — Zephyr's driver and our
      existing QEMU invocation already name the same NIC model. SLIP on
      `qemu_cortex_m3` would have added a host-side pty and a second runner
      shape for strictly less realism.

      This is also the exact hole the witness exists to close: an385 is
      **32-bit ARMv7-M** running Zephyr's **in-kernel** IP stack against a
      **real driver**, where all 28 existing Zephyr configs are
      `native_sim/native/64` with `CONFIG_NET_SOCKETS_OFFLOAD=y` (host
      sockets, 64-bit). `nros-platform-zephyr/src/net_wait.c:53`'s
      non-`CONFIG_BOARD_NATIVE_SIM` branch gets its first CI execution here.
- [x] **W2.b — LANDED 2026-08-05: the conf bundle, and five defects under it.**
      `cmake/zephyr/mps2-an385.conf`; **no new crate**, no overlay (W2.a's
      finding that the board's own DTS already enables `eth0`/`smsc,lan9220`
      held). **Verified end to end**: boots on `qemu-system-arm -machine
      mps2-an385`, `net_config: IPv4 address: 10.0.2.15` comes from the real
      `eth_smsc911x` driver over Zephyr's in-kernel stack, a zenoh session
      reaches a host router through SLIRP, and the talker publishes. 310 KB
      flash / 653 KB RAM of 4 MB each — not a tight board.

      The bring-up's actual content was five things that only a non-native_sim
      Zephyr can surface, none of them in the conf file:

      1. **`zpico.h` vs `zpico.c` disagreed on `size_t`.** cbindgen renders Rust
         `usize` as `uintptr_t`; the hand-written C uses `size_t`. The same type
         on x86-64, two types of one width on 32-bit ARM. Fixed at the generator
         (`usize_is_size_t`). Not an ABI change.
      2. **`portable-atomic`'s `unsafe-assume-single-core` was gated on an ARCH
         LIST**, which says yes to thumbv7m — a target with native LDREX/STREX
         that never needed the polyfill, and where the feature hard-conflicts
         with the `critical-section` upstream's `zephyr` crate always enables.
         `not(target_has_atomic = "ptr")` asks the real question. Three sites
         had spelled one intent three ways (arch list / target triple /
         unconditional); they are now one spelling.
      3. **`nros-rmw-zenoh-staticlib` had no allocator and no panic handler for
         bare-metal Zephyr** — every other no_std platform declares them;
         `platform-zephyr` did not, because cmake appends `,std` on native_sim
         and the host's std supplied both. New `platform-zephyr-baremetal`, plus
         entries in the `extern crate ... as _` lists: declaring a dep is not
         enough, an unreferenced one is never linked and its lang items never
         arrive.
      4. **Two cmake sites computed that feature string independently**; fixing
         one left the other on the old answer. Now one macro.
      5. **No entropy device on an385**, so `sys_rand_get()` linked against
         nothing and every RNG arm but `TEST_RANDOM_GENERATOR` is gated on
         `ENTROPY_HAS_DRIVER`. The conf records that this lands on a
         CONSTANT-seeded timer PRNG — a future PAIR off this bundle must vary
         the seed or repeat archived issue 0157's identical-GUID false negative.

      **The cells build the C entry, not the Rust one.** The pinned
      `zephyr-lang-rust` cannot compile the `zephyr` crate for ANY board whose
      devicetree has gpio nodes (**issue 0432** — a 5-arg `GpioPin::new` against
      a 6-arg signature; `CONFIG_GPIO=n` makes it 14 errors instead of 4). That
      is upstream and orthogonal to what this witness is for: 32-bit pointers,
      the in-kernel IP stack and a real ethernet driver are exercised identically
      by the C entry. W2.d's cell list is adjusted accordingly.
- [x] **W2.c — LANDED 2026-08-05: `PlatformId::ZephyrQemuCortexM`.** Enum arm,
      `index()` band, `fixture_tokens()` (`zephyr-cortex-m`, declared but spelled
      by no `fixtures.toml` row), `just_module()` (`zephyr` — one module, three
      boards, which is the RFC-0064 shape working), `ALL`.

      **The index was the one non-mechanical part.** `alloc::domain_of` gives
      each platform a 21-wide window out of 232 DDS domains, so it fits exactly
      eleven — the twelfth platform at index 11 computes domain 233 and
      `domains_valid` rejects it. The scarce resource is a LOW index and it
      belongs to platforms that BAKE, so `Fvp` and `Px4` (tier 3 / CarveOut, zero
      Runtime cells, windows that are unreachable arithmetic wherever they sit)
      move to the tail and the witness takes 9. If either ever gains a Runtime
      cell the gate fires — correct, because at that point the scheme is full and
      wants narrowing, not another renumber.
- [x] **W2.d — LANDED 2026-08-05. Cells:** pubsub × {c, cpp} × zenoh as
      `Runtime` (2 — **not 3**; the rust arm is blocked on issue 0432, and a cell
      that cannot build is not a `BuildOnly` cell, it is a lie); service and
      action as `BuildOnly` with the reason string until they run (4). Both
      Runtime cells pass in 3.3 s each on fixtures built through the west leaves
      lane. Zenoh only: cyclone and xrce on this board are untried, and the
      honest record of "nobody has attempted it" is no row, not a guess.
- [x] **W2.e — LANDED 2026-08-05.** Joined the west-lane exemption in BOTH
      directions of `matrix_fixture_coverage`, with no kind qualifier: this board
      has only Example cells and every one is a west build, so native_sim's
      examples-plus-non-rust-workspaces split has nothing to distinguish here.
      **Zero `fixtures.toml` rows added.** The leaves themselves are a new block
      in `zephyr-fixture-leaves.sh` rather than a widening of its board axis —
      that loop is native_sim × every lang × every rmw × every role, and this
      board is {c, cpp} × zenoh × talker.
- [x] **W2.f — LANDED 2026-08-05.** No new runner was needed:
      `QemuProcess::start_mps2_an385_networked` already drives this exact machine
      and NIC for the FreeRTOS lane, and `ZenohRouter::start_slirp` already
      existed for guests that reach the host at 10.0.2.2. `tests/
      zephyr_cortex_m_qemu.rs` is the consumer; ports come from `alloc::port_of`
      on BOTH sides, since the image bakes `CONFIG_NROS_ZENOH_LOCATOR` (a
      Cortex-M image has no env to read one from).

      **One red worth recording.** The test waits on the DRIVER's line
      (`IPv4 address: 10.0.2.15`), not the talker's. The console is muxed and
      Zephyr's logging flushes on its own schedule, so the first `Publishing:`
      lands in the stream BEFORE the boot banner — waiting on the talker pattern
      returns in ~0.1 s with `net_config` unflushed, then fails the driver
      assertion on a perfectly healthy run.
      *Verified:* both Runtime cells pass (3.3 s each). native_sim untouched.

## W3 — NuttX: two crates → one (closes #405) — **LANDED 2026-08-04**

Both upper layers come from upstream NuttX — kernel *and* arch ports, with the
link list **discovered** by scanning `$NUTTX_DIR/staging` for `lib*.a` rather than
hardcoded — so the in-tree crates are pure board overlay. Their `build.rs` bodies
are literally the same four calls.

**Blocked by W1.c** (one crate, two tiers).

**Measured:** 3350 → 2054 lines across the board crates (−1296, −39 %), counting
every tracked file except `Cargo.lock`, the two NuttX defconfigs and the upstream
patches (vendor data, and the two defconfigs are exactly what the merge keeps two
of). 27 board crates → 26.

- [x] **W3.a** Adopt `BaseConfig` (W1.b) in both crates first, so the merge is a
      move rather than a reconciliation. — `Config` now composes
      `BaseConfig` (`pub base`), with the prefix↔netmask conversion taken from
      the shared type instead of a per-board field; the fields became accessors,
      which is why `src/config.rs` grew 261 → 321 (it also gained four unit tests
      it never had).
- [x] **W3.b** Move the **1059 byte-identical lines** into the existing
      `nros-board-nuttx` (both boards already depend on it):
      `c/nuttx_run_tiers.c` (587), `src/config.rs` (261), `src/node.rs` (133),
      `src/entry.rs` (43), `c/nuttx_builtins_stub.c` (35). — done by W3.c's
      collapse rather than by hoisting into the family crate: with ONE board crate
      those 1059 lines already exist once, and moving them up a layer would have
      put board-overlay code (the `SIOCSIFADDR` push, the `nsh_main` override) in
      the family driver, which is the wrong owner.
- [x] **W3.c** Collapse to one board crate; the per-arch delta stays **data**:
      `nuttx-config/<arch>/defconfig`, `<arch>-nuttx-toolchain.cmake`, the nine
      `NUTTX_*` values in each `[[board]]`'s `[env]`, the two FFI subcrates'
      `.cargo/config.toml` (+ the riscv target JSON). Both ZST names kept as
      `pub type` aliases of `NuttxQemu`. The one semantic delta phase-322 named,
      `SLIRP_DEFAULT_IP`, is kept per-arch behind `cfg(target_arch)` — NOT
      unified: each value mirrors its own board's defconfig `NETINIT` address, so
      it is an arch fact like the defconfig itself, and unifying it would have
      changed the rv-virt guest's address in a wave that is meant to move code.
- [x] **W3.d** Registry rows per W1.c: one crate, two witnesses, two tiers.
- [x] **W3.e** Fixtures: the 30 `nuttx` + 4 `nuttx-riscv` rows keep their
      coordinates; the build recipes point at the merged crate. Cells unchanged
      in both directions.
- [x] **W3.f — Closed issue #405 here.** `lane-coords` maps `nuttx-riscv,c,zenoh`
      to the `nuttx` module, but `just nuttx build-fixtures` built only the arm
      side — the riscv workspaces were separate `full-matrix` recipes, so tier 2
      demanded a fixture its own builder could not produce. `just nuttx
      build-fixtures` is now `build-fixtures-arm` + `build-fixtures-riscv` (one
      stage, serial — they share one kernel tree), with the riscv half gated on
      the run's own coords by the new shared `nros_lane_wants_platform` helper in
      `scripts/build/fixture-lane.sh`. The issue-0196 coverage check is
      `every_fixture_token_is_producible_by_the_module_that_owns_it`, which walks
      each module's recipe graph from `build-fixtures` and fails when a fixture
      token the module OWNS is produced by no recipe on that path; removing the
      new dependency edge makes it fail with #405's exact symptom.

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

- [x] **W4.a** New layer-2 unit (`nros-board-threadx-port-riscv64`, or a `port-*`
      feature of the family crate) holding the three `.S` files + `tx_port.h`,
      with the upstream provenance and the `ULONG` rationale carried over verbatim.
      **DONE 2026-08-04** — `packages/boards/nros-board-threadx-port-riscv64/`,
      1690 lines: `port/inc/tx_port.h` (252) + **five** `.S` (1438), not three —
      `tx_thread_stack_build.S` (249) and `tx_thread_system_return.S` (187) carry
      the same `ULONG` retype and the "~1250" estimate had missed them.
      `tx_initialize_low_level.S` stays on the BOARD: it is the qemu_virt BSP's
      low-level init (it `#include`s the board's `csr.h`), patched for the Phase
      120.3 SP alignment, not arch-port code.
- [x] **W4.b** Thin `threadx-qemu-riscv64` to an overlay: `Config` defaults on
      `BaseConfig`, the four trait impls, console/exit/panic, virtio-net bring-up.
      **DONE** — `src/config.rs` 330 → 173, `src/lib.rs` 447 → 438, crate total
      4023 → 2185 lines (tracked files, `Cargo.lock` excluded).
- [x] **W4.c** Thin `threadx-linux` likewise (`src/config.rs` is already 84 %
      identical to its sibling — 55 diff lines of 339). **DONE** —
      `src/config.rs` 339 → 178, `src/lib.rs` 284 → 275, crate total 1265 → 1097
      (tracked files, `Cargo.lock` excluded).
- [x] **W4.d** **Behaviour-neutral wave:** the 42 + 23 fixture rows and the 18 + 12
      cells do not change. If a cell moves, the wave did more than it should.
      *Verify:* both ThreadX lanes, on rebuilt fixtures. **HELD** — `git diff` on
      `examples/fixtures.toml` and `matrix.rs` is empty; the counts re-measured
      after the wave are still 42 + 23 and 18 + 12.

**The precedence hazard this wave created, and how it is contained.** Moving
`tx_port.h` out of the board's `config/` moves it out of an include dir that was
already first. Every ThreadX riscv64 compile must now search the arch port's
`inc/` BEFORE `<THREADX_DIR>/ports/risc-v64/gnu/inc`, and losing that is a CLEAN
COMPILE against upstream's 8-byte `ULONG` — corrupted packets at runtime, no
diagnostic. Five consumers were updated together (the class, not the site):
`nros-board-common/src/threadx_qemu_riscv64_build.rs`,
`nros-board-threadx/build.rs`, `nros-c/cmake/nros-threadx.cmake`
(`PORT_OVERRIDE_DIR`), `cmake/platform/nano-ros-threadx.cmake` (the Cyclone
ddsrt cross block) and `config/threadx/nros-platform.toml` (the zpico shim).
The backstop is a `_Static_assert(sizeof(ULONG) == 4, …)` in
`nros-board-common/c/threadx_hooks.c` — compiled on both the cargo and the CMake
path, for both boards — so a sixth consumer added later fails the build with a
message naming this wave. The CMake helper also DERIVES its exclusion list from
the override dir's file names instead of taking a hand-written
`BOARD_OVERRIDES`, so "excluded from upstream" and "compiled from the fork"
cannot drift apart.

## W5 — FreeRTOS: template the per-board files — **LANDED 2026-08-04**

The irreducible per-board delta was estimated at **~60–80 lines** (vector table 19,
memory map 3 numbers, CPU clock 1, cflags 1, netif registration 4, driver
reference). The crate is ~1600 lines plus a 727-line `startup.c`. The gap is all
mechanical.

**Result.** Board overlay **2497 → 1058** lines (−58 %); family crate 2715 → 3311
(it absorbed the shared headers, the shared linker script and the C-lane entry);
FreeRTOS family total **5212 → 4369** (−843). The W5.f measurement is below and
it **partly disproves the estimate** — see W5.f.

- [x] **W5.a** Hoist the config headers to shared defaults: `config/lwipopts.h`
      (133) and `config/arch/cc.h` (55) have **zero** board-specific content;
      `config/FreeRTOSConfig.h` (111) has two (`configCPU_CLOCK_HZ`,
      `configPRIO_BITS`). Board supplies the values, not the files.
      **Done:** all three now live in `nros-board-freertos/config/`; the board's
      `config/` is 12 lines (two `#define`s + three relative `#include`s).
      Relative includes rather than a second include dir, because
      `FREERTOS_CONFIG_DIR` is a single directory read by six build scripts plus
      CMake and turning it into a search path is a cross-cutting change.
      *Proof of pure move:* 19 objects (FreeRTOS kernel ×8, lwIP ×9,
      `freertos_hooks.o`, `network_glue.o`) compiled against the pre- and
      post-move headers are **byte-identical**.
- [x] **W5.b** Retire the `startup.c` shadow copy. ~575 of its 727 lines are a live
      duplicate of `nros-board-freertos`'s `network_glue.c` + `freertos_hooks.c` +
      the board's own `board_mps2.c`; it survives only because the CMake lane
      compiles it while the cargo lane compiles `board_mps2.c`. One source, both
      lanes.
      **Done:** `startup.c` deleted. `FREERTOS_STARTUP_SOURCE` now names the same
      files the cargo lane compiles, plus one new C-lane-only TU,
      `nros-board-freertos/c/freertos_c_entry.c` (the C equivalent of the Rust
      lane's `run_entry`: semihosting stdio, log writer, task creation, `main`).
      `Reset_Handler` calls `main` on both lanes. The drift the split was hiding
      was real: the shadow copy seeded the **platform** PRNG while the shared
      glue seeds `srand()` — different generators, and zenoh-pico's session ID
      reads the platform one. `freertos_c_entry.c` carries that seeding.
- [x] **W5.c** Drop `nros_freertos_diag_network` (~180 lines of raw LAN9118 CSR
      pokes and hand-assembled ARP frames, duplicated into **both** C files and
      called on no working path), or move it behind an explicit debug feature.
      **Done:** deleted from both. The technique it demonstrated is written up in
      `docs/guides/freertos-lan9118-debugging.md`, which is where a debugging aid
      belongs.
- [x] **W5.d** De-duplicate `build.rs`: `configure_arm_cm3` / `add_freertos_includes`
      / `add_lwip_includes` are byte-identical to the shared crate's, and
      `emit_nros_app_config` is 57 lines of hand-maintained C-string mirror of
      `nros_board_freertos::Config::default()` — fold it onto `BaseConfig`.
      **Done:** all four now live in `nros_board_common::freertos_build`, called
      by both build scripts. The copies had ALREADY diverged — the family crate
      resolved cflags from the `[arch.*]` profiles (phase-338 W4) while the
      overlay still hardcoded Cortex-M3, so a Cortex-M7 overlay would have
      compiled its board glue with M3 flags next to an M7 kernel. `Config` now
      composes `BaseConfig` and reads its scheduling defaults from
      `nros_board_common::FreertosScheduling`, shared with the emitter; that
      closed a live 128 KiB `app_stack_bytes` drift between the Rust default
      (393216) and the emitted C mirror (262144).
- [x] **W5.e** Linker script → template + the three numbers (`FLASH` origin/length,
      `RAM`, `_estack`).
      **Done:** `nros-board-freertos/config/nros-freertos-cortex-m.ld` holds the
      section layout; `mps2_an385.ld` is 7 lines and `INCLUDE`s it. `INCLUDE`
      resolves against the linker `-L` path — `OUT_DIR` on the cargo lane, an
      explicit `-L` in the board's CMake overlay. *Proof of pure move:* the same
      object linked old-vs-new gives an identical section layout, symbol table
      and loadable image, under **both** GNU ld and rust-lld.
- [x] **W5.f** **Prove the claim**: write the skeleton of a second FreeRTOS board
      and show it is ~80 lines. It does not have to ship — the number is the
      deliverable, and it is what tells an S32K344 user whether this path is real.
      *Verify:* the freertos lane; MPS2 artefacts byte-comparable where the change
      was meant to be a pure move.
      **Measured: 205 lines, not 80.** The complete file set for a second board
      (NXP S32K344, Cortex-M7) is in `book/src/porting/freertos-board.md`:

      | | lines |
      |---|---:|
      | `config/*` (4 files) | 12 |
      | `c/board_<name>.c` — vector table, reset, netif registration | 64 |
      | **per-board delta — what the estimate counted** | **76** |
      | `src/lib.rs` — board ZST + four trait impls | 57 |
      | `build.rs` | 45 |
      | `Cargo.toml` | 27 |
      | **total a user actually writes** | **205** |

      The estimate was **right about the layer it counted** (76 against "60–80")
      and **silent about the rest**. The 129 remaining lines are not board
      facts: every Cortex-M FreeRTOS board writes the same semihosting
      `BoardPrint`/`BoardExit` and the same two-line `BoardEntry` delegations
      with a different type name. That is the next template — a declarative
      macro in `nros-board-freertos`, deliberately NOT done here because it
      needs `cortex-m-semihosting` + `panic-semihosting` on the family crate
      (a dependency-edge change, i.e. a lockfile change) and W5 was scoped to be
      artefact-neutral. Until it lands, quote **205**, not 80.

      Also not removable by any template: the MAC driver. `lan9118_lwip.c` is
      ~507 lines, and a board whose vendor SDK ships no lwIP netif pays it.

## W6 — Bare-metal MPS2: fold RTIC in as a feature

- [x] **W6.a — LANDED 2026-08-04.** Fold `nros-board-rtic-mps2-an385` into
      `nros-board-mps2-an385` behind an `rtic` feature — it already depends on the
      base crate and calls its `init_hardware` / `exit_success` / `enable_wfi_idle`.
      Deletes ~120 duplicated lines including a second `mask_to_prefix` and the
      **divergent config defaults** (`10.0.2.10` vs `192.0.3.10`) that are invisible
      until runtime.
      *Verify:* the `qemu` lane's baremetal cells.

      **How it landed.** Board crates **27 → 26**. The old crate was 463 lines
      (416 `src/lib.rs` + 47 `Cargo.toml`); it lands as 393 lines of
      `src/rtic.rs` + 24 lines of manifest (deps + the two features) + 51 lines
      of `Config::qemu_slirp` (20 code, 31 doc). So **~46 lines of duplicated
      CODE are gone** (`qemu_config` 14, `qemu_config_with_overlay` 21,
      `parse_decimal_u32` 14, `mask_to_prefix` 5, minus the preset that replaces
      them) against a near-flat total line count — the phase's "~120 lines"
      estimate counted the manifest and the doc comments the fold *adds*. The win
      is one crate fewer and one `Default`-shaped function fewer, not LOC.

      * **Both `mask_to_prefix` copies are gone.** The shared spelling is
        `nros_board_common::prefix_from_netmask`, the sibling of the existing
        `netmask_from_prefix` and the body `BaseConfig::prefix()` already had
        (leading-run, not popcount — a discontiguous mask now reports `/8`, not
        `/16`). No in-tree deploy block sets a discontiguous netmask, so this is a
        strictly-better spelling, not a behaviour change.
      * **The divergent defaults were NOT silently merged.** `Config::default()`
        keeps the bridge plan (`192.0.3.10/24`, `tcp/192.0.3.1:7447`) — which is
        also exactly `BaseConfig::default()`, so the board stays aligned with W1.b's
        shared default. The slirp plan the RTIC path needs became a NAMED preset,
        `Config::qemu_slirp()` (`10.0.2.10/24`, gw `10.0.2.2`,
        `tcp/10.0.2.2:7450`, `NROS_LOCATOR`/`NROS_DOMAIN_ID` build-env overrides),
        documenting which `QemuProcess::start_*` runner each plan matches. Rationale:
        the two values are not drift — they are two QEMU launch modes the firmware
        cannot observe — so the defect was that one of them lived in a sibling crate
        as a second `Default`-shaped function. Naming it removes the invisibility
        while keeping the wave behaviour-neutral. Actively *moving* the default is
        also not free: `baremetal_run_plan_runtime` asserts an `Executor::open`
        FAILURE banner from a fixture with no deploy overlay, so pointing the
        unpinned default at a reachable slirp gateway could make that test hang.
      * **The crate is gone but the deploy key is not.** `rtic-mps2-an385` /
        `qemu-rtic-mps2-an385` still name the RTIC entry shape; `board_path_for` and
        the proc-macro's `take_dispatch_consumer` now resolve into
        `::nros_board_mps2_an385::`, which re-exports the folded surface at its root.
      * **Known consequence:** the merged crate deliberately carries no
        `[package.metadata.nros.board] framework = "rtic"` — framework is now a
        feature, not a crate fact, and declaring it unconditionally would make
        `nros ws check` reject `dispatch = "inline"` on every direct-exec Entry pkg.
        Cost: the RTIC-requires-Deferred lint no longer fires for a path-dep'd RTIC
        workspace. No in-tree workspace path-deps this board.
      * **Runtime proof** (`just qemu build-fixtures`, then
        `binary(emulator) or binary(baremetal_run_plan_runtime)`, 17 tests):
        `baremetal_board_run_executes_run_plan`, `test_qemu_bsp_pubsub_e2e`,
        `test_qemu_rtic_pubsub_e2e` (the platform's only `Runtime` cell),
        `test_qemu_rtic_service_e2e`, `test_qemu_rtic_action_e2e`,
        `test_qemu_serial_pubsub_e2e`, `test_qemu_xrce_pubsub_e2e` all pass.
        `test_qemu_rtic_mixed_priority_pubsub_e2e` is the documented
        LAN9118/slirp RX-stall flake: measured 3/5 solo on the folded build vs
        **2/5 on the pre-fold HEAD binaries through the same harness**, so the fold
        did not move it.

## W7 — Demotions (GATED on the matrix decision)

Do not start until the fixture/test set is confirmed. Both removals are free on
the test axis, which is what makes them safe once decided.

- [x] **W7.a — LANDED 2026-08-04** (`1b8c4e089`; tutorial first, `5ccd26adc`).
      `stm32f4` + `rtic-stm32f4` leave the matrix — **0 Runtime cells** today, so
      no runtime coverage is lost; 3 BuildOnly cells and 8 fixture rows went with
      them. `book/src/porting/stm32f4-out-of-tree.md` is the worked customization
      example, and it landed FIRST so the documented path never disappeared.

      **`embassy-stm32f4` rode in this wave, not W7.b.** It is a member of the
      same board FAMILY — same chip, same `examples/stm32f4/` tree, same
      `PlatformId`. Splitting it across two waves would have meant a wave that
      half-deletes a directory, which is worse than the wave-fence it would have
      honoured. Board crates 26 → 23; the ten `examples/stm32f4/rust/*` packages,
      the three `templates/cargo-*-stm32f4.toml`, `just/stm32f4.just` and
      `cmake/board/…-stm32f4-nucleo.cmake` went with them.

      **Kept on purpose:** `nros-platform-stm32f4`, `stm32f4-usart`,
      `packages/reference/stm32f4-porting/` and `nros-smoke/stm32f4-smoltcp-echo`.
      RFC-0064's "Deleted (12)" list is BOARDS; a platform crate is the chip
      material an out-of-tree board consumes, and the porting references are the
      BSP-developer templates the book page sends people to. None of the four
      depended on a deleted crate. Also kept: `Framework::Embassy` and its
      `nros::main!()` emit branch — the framework SEAM (see issue **0415**, which
      this wave filed: the macro's framework table is deploy-keyed, so nothing
      out-of-tree can reach that branch until it reads the board crate's
      `framework` metadata the way `nros ws check` already does).

      Issue **0248** (the Embassy board's scaffold defect) is RESOLVED by the
      deletion — which is what its own analysis pointed at: *"finishing as
      stm32f4 can never earn a CI runtime lane."*

      *Verify:* `just check fast` (`check-board-tiers` green), `just check build`,
      `matrix_fixture_coverage` both directions, `example_shape`,
      `example_portability`.
- [x] **W7.b — LANDED 2026-08-04.** Delete the scaffolds — `embassy-stm32f4`
      (taken by W7.a with its family, above), `esp32s3`,
      `s32z270dc2-r52` contribute **zero cells of any tier**, so removal is
      provably free here. Board crates 23 → 20; the scaffold tier is now EMPTY
      (the state stays declared in `board-support.toml` — it is what stops the
      next unfinished board from reading as support).

      **Beyond the three crates, the dead chains each left behind:**
      `nros-platform-esp32s3` (802 lines, no consumer left, and no Xtensa
      toolchain in the SDK index to build it — RFC-0064's ESP32 story is the
      ESP-IDF integration shell, which supports every part with zero files here),
      `nros-smoke/esp32s3-board-bringup` (its only dep was the deleted board),
      and the `board_import_s32z` fixture plus its `build-s32z-board-import`
      recipe, which had NO caller and was never registered in `WEST_FIXTURES`.

      **`orin-spe`'s chain, deleted in the same change** (the phase's ordering):
      `config/orin-spe/`, the `CARGO_FEATURE_ORIN_SPE` branches in
      `nros-zpico-build/src/runner.rs`, the `orin_spe` parameter of
      `config_header` (its two knobs — `Z_FEATURE_ENCODING_VALUES` and
      `Z_FEATURE_AUTO_RECONNECT` — become unconditional; a future
      space-constrained platform turns them off through the RFC-0049 knob ladder,
      not a board-named bool), the `zpico-sys` `orin-spe` feature, the
      `platform-orin-spe` features on `nros-platform` / `nros-rmw-zenoh` /
      `nros-rmw-zenoh-staticlib`, `nros-sdk-index.toml`'s `[board.orin-spe]` +
      `[gated.nv-spe-fsp]`, `just/orin-spe.just` and the `zpico_backend` lint
      value in the root `Cargo.toml`.

      `LinkPolicy::orin_spe()` was **renamed, not deleted**: the policy (no TCP /
      UDP / serial / raweth / TLS, IVC + custom only) is a capability shape, not a
      board fact, so it is `LinkPolicy::ivc_only()` with an `#[allow(dead_code)]`
      naming the wave. `packages/drivers/ipc/nvidia-ivc` stays — IVC is a LINK
      capability (`LinkFeatures::ivc`), and keying it to one vendor board is what
      made a scaffold load-bearing in the first place. Its test moved with the
      same reasoning: `orin_spe_mock_ivc` → `nvidia_ivc_mock_wire_format`, since
      RFC-0064 R3 had already measured that it "proves the IVC wire format on
      POSIX, NOT the board".

      The wave's pre-condition held: phase-338 W5.b had already measured that
      `orin-spe`'s pseudo-platform status was **not** a blocker — the only crate
      enabling the feature was the board itself, and no example, fixture or test
      touched it. So this was an ORDERING problem, not an untangling one, and the
      ordering is what the change above follows.
- [x] **W7.c — LANDED 2026-08-04.** Delete `nros-board-bare-metal` (phase-322
      W1.h): 161 lines of which 135 are a doc comment describing a family driver
      no board opted into. Board crates 20 → 19.

      Direct-exec now has NO family crate, and that is the honest state: the
      three boards of that shape (`mps2-an385`, `esp32-qemu`, and the departed
      `stm32f4`) each hand-rolled `BoardEntry::run`, so the driver documented a
      convergence that never happened. `nros-board-mps2-an385` is the worked
      reference the book points at instead
      (`book/src/porting/board-trait.md`, `concepts/board-integration.md` — the
      latter had been naming `nros-board-baremetal-cortex-m`, a crate that never
      existed under that name).

## W8 — linux: merge `native` + `posix`, retire `native`

Widest wave (188 fixture rows), mechanically safe, and self-gating. **Do it last**,
when every other wave has settled, so its rename does not churn under them.

**Status 2026-08-04: W8.a and W8.d landed; W8.b landed for the enum only; W8.c
open.** The split is deliberate and the reason is under W8.b — the fixture token
shares its spelling with the lane, the `just` module and the example directory,
three of which deliberately keep the name.

**The naming decision** (RFC-0064): platform stays `posix` — a genuine portability
seam, and the platform layer names software-stack facts (RFC-0049's duty rule).
The board becomes `linux` — the board layer names what we actually claim, and a
tier-1 promise means "`just ci` exercises it", which only Linux does (all 19 CI
jobs are `ubuntu-*`; `nros-platform-posix/src/timer.c:72` calls `timer_create`
with no fallback and macOS has no POSIX timers).

- [x] **W8.a** Merge `nros-board-native` into one `nros-board-linux`.
      `board_path_for` already maps **both** keys to the same ZST
      (`nros-orchestration-ir/src/lib.rs:78`), so the shim was named by no
      generated entry — this is ceremony removal, not a behaviour change.

      **LANDED 2026-08-04.** `nros-board-posix` (592 lines, the family driver)
      became `nros-board-linux`; `nros-board-native` (226 lines, a shim that
      delegated every trait method one-for-one) is deleted, with its two real
      contributions folded in: the `__FORCE_LINK_ZENOH` static and
      `register_linked_rmw()`, plus the RMW dep/feature table that makes the
      BOARD the RMW selection point. `register_linked_rmw()` now sits on the two
      boot FUNNELS (`boot_hosted`, `run_tiers`) instead of on four forwarding
      methods, so a path added later cannot forget it. `PosixBoard`/`NativeBoard`
      → `LinuxBoard` across 203 files. Board crates 19 → 18.
- [x] **W8.b — PARTIALLY LANDED 2026-08-04: the enum, NOT the token.**
      `PlatformId::Native` → `Linux` across the enum, `index()`, `just_module()`,
      `ALL`, every `cell(...)` / `sched(...)` / `c(...)` row and every consumer
      (`alloc`, `ci_lane`, `interop`, 13 test files). `fixture_tokens()` still
      returns `["native"]`, and `fixture_token_mapping_round_trips` passes
      because the map stays bijective either way.

      **Why the token did not move with it — measured, not deferred out of
      caution.** `"native"` is not one vocabulary, it is four that share a
      spelling, and only ONE of them is the fixture token:

      | Use | Example | Moves to `linux`? |
      |---|---|---|
      | fixture token (`platform =`) | `examples/fixtures.toml` ×188 | yes — W8.c |
      | shell ARGUMENT built from it | `fixtures-build.sh native rust`, `workspace-fixtures-build.sh native`, `fixture-make-driver.sh native-cmake-rmw`, `phase226-cxx-eff.sh --platform` | yes, same piece |
      | lane-coord prefix derived from it | `fixture-lane.sh`'s `grep -v '^native,'`, fed by `lane-coords` → `fixture_tokens()` | yes, same piece |
      | the LANE name | `just build-test-fixtures lane=native`, `_NROS_LANES` | **no** |
      | the `just` MODULE | `just native build-fixtures`, `just_module()` | **no** (~100 CI refs) |
      | the example DIRECTORY | `examples/native/rust/talker` | **no** (it is a `dir =` value, not the token) |

      Renaming the token alone leaves rows saying `linux` and the builder that
      produces them still called with `native` — two spellings of one fact, the
      exact defect class this repo keeps re-fixing. Doing all three token-derived
      rows together is correct but must be proven by a full `just ci`, because
      this is the reference platform and a half-resolved fixture reads as a
      missing fixture, not as an error (the 0350 class).
- [x] **W8.c — LANDED 2026-08-05.** All 188 `platform = "native"` fixture rows
      (the RFC's 187 was one low) → `linux`, together with the two other
      token-derived vocabularies, in ONE commit. The lane name, the `just` module
      and `examples/native/` keep theirs, as the table above says.

      **The rename was safer than its size suggests**, because issue 0406's
      `nros_fixture_require_known_platform` validates the builder's platform
      argument against the manifest's own platform list. A missed caller exits 2
      with "unknown platform" instead of sweeping zero rows successfully — which
      is exactly what happened to `check-fixture-id-guard.sh`'s manifest sampler
      and to `check-fixtures-stale.sh`'s scope map, both caught by `check-fast`
      on the first run rather than by a fixture that silently stopped existing.

      Two files now carry BOTH spellings on purpose, and each says so at the
      line: `fixture-lane.sh` compares the LANE (`native`) and greps the
      COORDINATE prefix (`^linux,`), and `check-fixtures-stale.sh` maps
      `SCOPE=native` to `--platform linux`. That is the seam between two
      vocabularies, not a leftover.
- [x] **W8.d — LANDED 2026-08-04 (descriptor + registry).**
      `packages/boards/posix/` → `packages/boards/linux/`, and its `[[board]]
      names` becomes `["linux", "native", "posix"]` — `linux` canonical, the other
      two kept as accepted INPUT spellings because ~200 example and fixture
      manifests carry `deploy = "native"`. `board-support.toml` collapses its TWO
      tier-1 host rows (one per merged crate) into one `matrix_platform = "Linux"`
      row, and `check-board-tiers.py`'s tier-1 nightly-lane exemption keys off
      `Linux` instead of `Native` — the host is exempt because `just ci` runs it
      directly, which is stronger than the nightly build the token stands in for.
      The SDK index needed no change: it has no `[board.native]` alias.
- [ ] **W8.e** *(optional, only if a hosted non-Linux board is ever wanted)* a
      `timer_create` fallback (dispatch source or `pthread_cond` timed wait), which
      is what would let `macos`/`freebsd` join later as tier-3 boards on the
      unchanged `posix` platform.
      *Verify:* full `just ci` — this wave touches the reference platform.

## W9 — Zephyr boards stop being crates

Depends on W2 (the conf-bundle mechanism) and W1.b (shared `Config`).

- [x] **W9.a — LANDED 2026-08-05.** `nros-board-fvp-aemv8r-smp` →
      `nros-board-zephyr/boards/fvp-aemv8r-smp/`; 18 board directories → 17.

      The crate's Rust half (`Config`, `init_hardware`, `run` — 160 lines) had
      **zero consumers**: `nros_board_fvp_aemv8r_smp` appeared in exactly one file
      in the tree, its own `Cargo.toml`. What it actually shipped was a
      `prj.conf`, a DTS overlay, a Kconfig fragment and a `board.cmake` — a config
      bundle wearing a Cargo.toml, which is the whole argument. Zephyr already
      owns boot, MMU, net stack and drivers.

      **The board KEY does not change.** `nano_ros_use_board(fvp-aemv8r-smp)`
      works at every call site because the LOOKUP widened rather than the callers
      moving: a board crate (`nros-board-<name>/board.cmake`) first, then a conf
      bundle (`nros-board-<family>/boards/<name>/board.cmake`). Shape 1 stays
      supported — out-of-tree boards that carry real Rust (a `BoardEntry`, a
      driver) are still crates, which RFC-0064 says explicitly. Two matches is an
      error, not a search-order tiebreak: board keys are global. The CLI's
      `locate_board_crate` implements the same two steps for `nros board info` /
      `nros setup board`.

      Third registry row for `nros-board-zephyr` — native_sim (tier 1), the
      Cortex-M witness (tier 2), the FVP (tier 3). Exactly what W1.c's
      (crate, matrix_platform) keying exists for.

      *Verified:* `board_import_fvp_builds_via_nano_ros_use_board` PASSES on a
      freshly configured west fixture, and its `CMakeCache.txt` resolves the
      overlay to the NEW bundle path — not a stale one. `nros board info
      fvp-aemv8r-smp` resolves to the bundle dir. `check-board-tiers` green at 17
      directories. The FVP model itself stays license-gated, so `fvp_smoke` still
      skips — unchanged by this, and the reason the board is tier 3.

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

Measured 2026-08-05, after every wave.

- [~] **Board crates 27 → 16.** At **17 directories** (15 crates + the two
      descriptor-only dirs `linux/` and `zephyr/`, which the registry counts but
      which are not crates). Every removal traces to a wave: W3 −1, W6 −1,
      W7.a −3, W7.b −3, W7.c −1, W8.a −1, W9.a −1; W4.a +1 (the arch port).
      **One BELOW the target on crates, one above on directories** — the target
      counted the two descriptor dirs as crates, which they are not. Nothing
      remains to remove: the last per-board Zephyr crate went in W9.a, and what
      is left is either a real crate with real code or a descriptor dir that
      phase-321 W2 moves out of `packages/boards/` entirely.
- [x] **Fixture rows 344 → 336.** At **337** `platform =` rows: `stm32f4`'s 8
      left with W7.a (344 → 336 was the prediction; the pre-wave count was 345,
      not 344, so the post-wave figure is 337). W8.c renamed 188 of them
      `native` → `linux` and changed no count, as predicted.
- [~] **Cells 202 → 208, Runtime 174 → 177.** At **205** cells (176 Runtime, 18
      BuildOnly, 11 CarveOut): −3 from `Stm32F4`'s BuildOnly cells (W7.a), +6
      from W2.d's witness. Three short of the 208 target and one short on
      Runtime, both for the same reason: the witness landed **2 Runtime + 4
      BuildOnly instead of 3 + 6**, because issue 0432 blocks the Rust arm
      entirely. A cell that cannot build is not a `BuildOnly` cell, so those rows
      are ABSENT with the reason recorded rather than present and false. The
      target's arithmetic assumed a Rust row that upstream will not compile.
- [x] **No wave's commit touches a board crate outside its own family.** W7.a is
      the one that looks like an exception and is not: `embassy-stm32f4` is a
      member of the STM32F4 family, and the wave says so.
- [x] `check-board-tiers`, `matrix_fixture_coverage` (both directions) and the
      allocator injectivity gate green after every wave, not only at the end —
      `just check fast` + `just check build` were run and green per wave, and
      the four pre-existing reds they surfaced were fixed first, each in its own
      commit.
- [x] A second FreeRTOS board is demonstrably ~80 lines (W5.f). **Measured 2026-08-04:
      76 lines of board delta, 205 lines total — the estimate counted only the
      C/config layer. See W5.f.**
- [~] `just ci-matrix` green after each wave that has tier-2 cells. **The earlier
      "NOT RUN — no landed wave adds or moves a tier-2 cell" is EXPIRED**: W2
      landed `ZephyrQemuCortexM` as a tier-2 board, and the tier-2 1-wise cover
      picks it up automatically (`zephyr-cortex-m,c,zenoh` is in
      `lane-coords tier2`). So this criterion became live the moment W2.d landed,
      and the phase was briefly marked COMPLETE while it was outstanding.

      **Attempting it found that tier 2 could not run AT ALL** — for reasons that
      predate this phase and have nothing to do with its cells:

      * **issue 0439** (found and FIXED here) — a lane-narrowed build killed any
        recipe naming a fixture by `--id`. Three of eight tier-2 modules died, so
        no stamp was written and `_lane-gate` refused before running anything.
        Two guards, each right alone: 0393's `--coords-from` removes rows for
        lane reasons; 0406 treats an `--id` matching zero rows as a wrong
        invocation. Together, the lane's own narrowing got blamed on the caller.
      * **issue 0433** (upstream, another agent) — NuttX arches share one
        `$NUTTX_DIR/staging`, so building both clobbers the link list. Visible
        here as `nuttx` failing under `lane=all` but passing under `lane=tier2`,
        which builds one arch.

      Both are fixture-BUILD defects, not matrix or board defects, which is why
      no earlier wave saw them: `lane=all`, `lane=native` and `lane=tier1` do not
      combine flag-narrowed recipes with lane coordinates.

## What is left, and what it needs

| Wave | State | What it needs |
|---|---|---|
| W2.a | **done** | — |
| W2.b | **done** | the bring-up itself is finished and proven: the image boots, takes an IP from the real driver, and publishes over a zenoh session. Five defects fixed under it; one (0432) filed rather than fixed. |
| W2.c–f | **done** | `PlatformId::ZephyrQemuCortexM` at index 9, 2 Runtime + 4 BuildOnly cells, the west-lane exemption, the `build-cortex-m-*` fixture leaves and a runner. Both Runtime cells pass in 3.3 s each on fixtures built through `zephyr-fixture-make-driver.sh`. The registry row came with a widening of `check-board-tiers`, which could not see a board that owns no directory. |
| W8.c | **done**, verification carried | all three token-derived vocabularies moved together; the lane name, the `just` module and `examples/native/` deliberately kept theirs. The full `just ci` its acceptance asks for has NOT come back clean — not for any token reason (no run produced a platform or coordinate error) but because issues 0435, 0433 and 0439 each broke the fixture build in turn. Failures fell 126 → 43 → 33 → 15 purely by rebuilding. |
| W9.a | **done** | folded into `nros-board-zephyr/boards/fvp-aemv8r-smp/`; 18 board directories → 17. `nano_ros_use_board(<name>)` unchanged at every call site — the lookup widened instead. |

Every wave of this phase has landed. What remains is the `just ci-matrix`
acceptance criterion (see Acceptance) — blocked twice over by fixture-BUILD
defects, one of which (0439) this phase found and fixed.

## Close-out (2026-08-06) — COMPLETE

Every wave landed (W1–W8). The two remaining unchecked boxes are not outstanding
work in this phase:

- **W1.a** is explicitly MOVED to phase-338 W4.a (and phase-338 is itself
  complete). It is tracked there, with one owner, which is what the box says.
- **W8.e** is marked *optional, only if a hosted non-Linux board is ever wanted*
  — a `timer_create` fallback that would let macos/freebsd join later as tier-3
  boards. A conditional future, not a debt.

Leaving both boxes unchecked on purpose: ticking them would claim work that was
either done elsewhere or never done at all.

Downstream note: this phase's board reshaping is what phase-339 then had to make
arch-safe, and phase-339's close-out records the one place it bit (issue 0456 —
two of three riscv recipes never said which arch they were, so the arm defaults
applied).

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
