# Phase 170 — Bare-metal example collapse (per-board feature wrangling)

**Goal.** Collapse the per-RMW directory axis on the four
bare-metal-Rust platforms — `qemu-arm-baremetal`,
`qemu-esp32-baremetal`, `esp32`, `stm32f4` — to single
`<plat>/rust/<case>/` dirs with mutually exclusive
`rmw-{zenoh,dds}` Cargo features, mirroring the freertos /
threadx-rv64 / threadx-linux Rust collapses that landed in
Phase 118.B.4 / .B.6 / .B.7. Held over from Phase 118.B because
each bare-metal board crate exposes its own per-RMW feature
matrix (`dds-heap`, `ethernet`, board-specific `critical-section`
gates) that needs board-by-board feature-proxy work — not the
mechanical port the other RTOSes accepted.

**Status.** Not Started.

**Priority.** P2 — same class as Phase 167 (NuttX Rust collapse)
and Phase 168 (Zephyr collapse). Bare-metal cells today only
ship the canonical pubsub pair (talker + listener) — no
service/action variants — so the per-platform collapse is at
most 2 cases × 2 RMWs = 4 entries per platform, but the
per-board feature wrangling makes each entry non-mechanical.

**Depends on.** Phase 118 (collapse mechanism + smoke test
infrastructure).

---

## Cells in scope

```
examples/qemu-arm-baremetal/rust/<case>/      talker, listener  (zenoh + dds)
examples/qemu-esp32-baremetal/rust/<case>/    talker, listener  (zenoh + dds)
examples/esp32/rust/<case>/                   talker, listener  (zenoh only)
examples/stm32f4/rust/<case>/                 talker            (zenoh only;
                                              non-rtic variant)
```

RTIC variants (`talker-rtic`, `listener-rtic`,
`service-{server,client}-rtic`, `action-{server,client}-rtic`,
`*-rtic-mixed`, `talker-embassy`) stay as standalone cases
outside the matrix per the existing Phase 131 "variant naming
uses suffix form" rule.

Total in scope: 4 plat × ~1.5 RMWs × ~1.5 cases ≈ **9 cells**.

## Why bare-metal needs its own phase

The other RTOS collapses had two clean dependency axes:
**Cargo feature → RMW dep + nros feature**. Bare-metal layers a
third axis: **board feature**. Each `nros-board-<board>` crate
exposes `[features]` that the example must proxy to keep boot
state (heap size, ethernet init, critical-section
registration) consistent with the chosen RMW.

Concrete examples from the legacy siblings:

- **qemu-arm-baremetal/rust/dds/talker** uses
  `nros-board-mps2-an385 = { default-features = false, features
   = ["ethernet", "dds-heap"] }` — `dds-heap` flips the
  bare-metal heap from 64 KB to 2 MB to fit dust-dds's
  `DcpsDomainParticipant` builtin entities. `nros-platform =
  { features = ["platform-mps2-an385", "global-allocator",
   "critical-section"] }`. None of those features appear in
  the zenoh sibling.
- **qemu-arm-baremetal/rust/zenoh/talker** uses
  `nros-board-mps2-an385 = { path = ... }` (default features
  ON, no `dds-heap` / `ethernet` overrides — those defaults
  exist for the zenoh path) and skips the `nros-platform`
  dep entirely.

A clean collapse needs to proxy these through `rmw-*` features:

```toml
[features]
rmw-zenoh = ["dep:nros-rmw-zenoh", "nros-board-mps2-an385/zenoh"]
rmw-dds   = ["dep:nros-rmw-dds",
             "nros-board-mps2-an385/ethernet",
             "nros-board-mps2-an385/dds-heap",
             "nros-platform/critical-section",
             "nros/alloc",
             "dep:critical-section"]
```

— plus the matching `optional = true` on every dep that
isn't shared. The per-board feature lists differ across the
four platforms (different board crates, different feature
names), so the work is per-platform, not per-rtos-family.

## Work Items

- [ ] **170.1 — `qemu-arm-baremetal/rust/{talker,listener}`
       collapse.** Reuse the mps2-an385 board crate's existing
       `dds-heap` + `ethernet` features through the Cargo
       feature proxy shape above. Test build under
       `--features rmw-zenoh` + `--features rmw-dds`.
- [ ] **170.2 — `qemu-esp32-baremetal/rust/{talker,listener}`
       collapse.** ESP32-C3 board crate's per-RMW features
       (if any) — check `packages/boards/nros-board-esp32-qemu/`
       Cargo.toml feature list. May need a smaller heap-resize
       gate equivalent to mps2-an385's `dds-heap`.
- [ ] **170.3 — `esp32/rust/{talker,listener}` collapse.**
       Real-hardware ESP32 board. Zenoh-only today (dust-dds
       hasn't been brought up on the WROOM hardware; matrix
       lint records that gap). Restructure-only port.
- [ ] **170.4 — `stm32f4/rust/talker` collapse.** Single case,
       zenoh only. Restructure-only port (no DDS sibling
       exists for STM32F4).
- [ ] **170.5 — Test-harness `build_baremetal_rust_example_rmw`
       helper.** Mirror the `build_freertos_rust_example_rmw` /
       `build_threadx_*_rust_example_rmw` pattern. Each
       platform's binary lives at
       `target-<rmw>/<arch-triple>/release/<binary>` —
       `thumbv7m-none-eabi` (qemu-arm), `riscv32imc-unknown-none-elf`
       (esp32 / qemu-esp32), `thumbv7em-none-eabihf` (stm32f4).
- [ ] **170.6 — Justfile + phase_118_collapse smoke tests.**
       `just qemu-baremetal build-fixtures`, `just esp32
       build-fixtures`, `just stm32f4 build-fixtures` iterate
       collapsed cells × matching RMWs. Smoke tests assert
       binaries exist at the per-arch `target-<rmw>/<triple>/
        release/<binary>` paths.
- [ ] **170.7 — Drop legacy `<plat>/rust/{zenoh,dds}/<case>/`
       siblings.** Per Phase 118 Tier-5 cleanup pattern. RTIC
       variants stay (`talker-rtic`, etc.) — they're separate
       cases, not per-RMW siblings.

## Acceptance criteria

- [ ] All four bare-metal `<plat>/rust/<case>/` directories
       build via the canonical `cargo build --no-default-features
       --features rmw-<rmw>` pattern.
- [ ] `phase_118_collapse` smoke includes bare-metal cells.
- [ ] No regression on the legacy zenoh + dds siblings until
       170.7 lands.
- [ ] Phase 118 example-matrix coverage rolls bare-metal cells
       from legacy → collapsed.

## Notes

- **No service / action axis.** Bare-metal cells today only
  ship pubsub. Phase 118 catalog deliberately doesn't fill
  those cells until the bare-metal MCUs grow enough RAM for
  the action-server's goal-state pool — that's a different
  conversation. Phase 170 only collapses what already exists.
- **RTIC variants out of scope.** `talker-rtic` /
  `listener-rtic` / `service-*-rtic` / `action-*-rtic` are
  separate cases (per Phase 131's "variant naming uses suffix
  form" rule), not per-RMW siblings of `talker`. They don't
  get folded into the collapse — they each live at
  `<plat>/rust/talker-rtic/` etc. and keep their own
  Cargo.toml.
- **Bare-metal C / C++ holes.** Already documented as
  intentionally empty (Phase 118.E.1 / CLAUDE.md) — no C /
  C++ harness on `qemu-arm-baremetal`, `qemu-esp32-baremetal`,
  `esp32`, `stm32f4`. Phase 170 doesn't touch those cells.
