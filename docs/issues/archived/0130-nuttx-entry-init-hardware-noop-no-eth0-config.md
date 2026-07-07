---
id: 130
title: "NuttX Entry path never configures eth0 — `nros_platform::BoardInit::init_hardware` no-op → guaranteed Transport(ConnectionFailed) for networked entry e2e"
status: resolved
type: tech-debt
area: boards
related: [issue-0127, phase-275, phase-280, rfc-0032]
---

## Resolution (phase-280, 2026-07-08)

Both NuttX Entry paths now push the guest IP into `eth0` before opening the
executor, from ONE shared helper —
`nros_board_nuttx_qemu_arm::configure_entry_eth0(ip, prefix, gateway)`
(`SIOCSIFADDR` + `/dev/urandom` reseed, delegating to the sole
`node::init_hardware` body, no second `SIOCSIFADDR` site):

- **Rust path** (`703e840dd`): `entry_net_init` → `configure_entry_eth0`, called
  from the `BoardEntry::run` / `run_with_deploy` wrappers.
- **C / C++ path** (`1f8b82d3b`): `nros-nuttx-ffi` `main` calls
  `configure_entry_eth0` (slirp defaults `10.0.2.30/24` via `10.0.2.2`, per-entry
  `option_env!("NROS_IP"/…)` overrides) BEFORE `app_main()`, covering both the C
  and C++ `nano_ros_entry LAUNCH` entries.

**Runtime proof (phase-280 W3/W4): BOTH e2e GREEN in nextest** —
`rust_nuttx_entry_delivers_cross_process` (PASS) and
`c_nuttx_workspace_entry_delivers_cross_process` (PASS). The prebuilt
`nuttx_rs_talker_entry` ELF, booted under `qemu-system-arm -M virt` + slirp with a
host `zenohd`, applies `eth0 = 10.0.2.30` (pcap: `ARP who-has 10.0.2.2 tell
10.0.2.30`, `SYN → 10.0.2.2:7452`, full zenoh session) and delivers cross-process
— the `Transport(ConnectionFailed)` symptom is gone. (Getting the Rust e2e green
required reverting a wrong grep prefix: the Rust talker + `build_native_listener`
are both `std_msgs/String` (`"I heard:"`), not Int32 — an earlier edit had
switched it to `INT32_LISTENER_LOG_PREFIX`; delivery worked all along, the test
just never matched. The C entry's `demo_bringup` talker really is Int32, so
`c_nuttx_entry_e2e` correctly keeps `"Received:"`.) See
`docs/roadmap/archived/phase-280-*` for the nextest
CI-lane caveat.
---

## Summary

Every NuttX Rust Entry image (`nros::main!` → `BoardEntry::run` on
`QemuArmVirt`) boots, reaches `Executor::open`, and fails with
`Transport(ConnectionFailed)` — it can never reach the zenoh router because
the guest's static IP is never pushed into `eth0`. Found during the #127
board-centric link spike (2026-07-03): the spike image AND the pre-existing
`workspace-rust-qemu-nuttx` control print the exact same failure line, so
this is an Entry-**runtime** limitation shared by every entry image,
orthogonal to the (now landed) link convention.

**Build-asserts are unaffected** — the six `*_entry` fixture rows + issue
#127's `tests/nuttx_entry_build.rs` only assert the link. This issue blocks
any *networked* nuttx-entry e2e (cross-process pub/sub through the slirp
router).

## Root cause

`<QemuArmVirt as nros_platform::BoardInit>::init_hardware()` is a documented
no-op (`entry_212n.rs`, Phase 212.N.3): the platform-level trait is
parameterless (config moved to `RuntimeCtx`), so the body can't run the
config-dependent steps the legacy role path performs in
`crate::node::init_hardware(&Config)`:

- push `Config.ip` into `eth0` via `SIOCSIFADDR` (+ netmask/router), and
- re-seed `/dev/urandom` from the IP (session-ID uniqueness).

Without the `SIOCSIFADDR` step the guest keeps the defconfig-baked address
and cannot reach slirp's `10.0.2.2`, so `Executor::open` on the baked
`NROS_LOCATOR` (`tcp/10.0.2.2:7452`) always fails.

**Calibration (spike evidence):** the known-good ROLE fixture
(`nuttx-rs-talker`, legacy `node::init_hardware(&Config)` path) boots in the
same QEMU harness, prints
`nros NuttX platform starting (IP: 10.0.2.30, zenoh: tcp/10.0.2.2:7452)` and
publishes — harness and link are sound; only the Entry runtime path skips
the eth0 config.

## 2026-07-04 update — fix landed, runtime verification gated

`entry_net_init()` (nros-board-nuttx-qemu-arm/src/entry_212n.rs, commit
703e840dd) now performs the urandom-reseed + SIOCSIFADDR push before
`run_entry`: slirp defaults 10.0.2.30/24 via 10.0.2.2 (the values the
known-good role fixtures push), overridable per entry via the
`[package.metadata.nros.deploy.nuttx]` `ip`/`netmask`/`gateway` keys
(direction 1 below, board-scoped). Compiles through the full ARM lane —
which required un-breaking three adjacent pre-existing lane defects first
(commit d395e3922: arch-blind provision gates, duplicate vectortab link,
pre-W6 entry dep style).

Runtime e2e verification still gated on test-infra holes:
- `c_nuttx_entry_e2e` progressed from instant fixture-skip to a real 60 s
  run that times out — needs its own follow-up (first time it has actually
  executed on this box).
- `rtos_e2e` nuttx-rust resolvers still point at the role `[[bin]]` retired
  in phase-212 (#132) — those combos cannot exercise entry images yet.

## Fix direction

Wire the IP/locator plumbing through the Entry path (the 212.N.4 direction
the no-op body already names): either

1. `RuntimeCtx` (or the board's `run_entry`) carries the baked network
   config (`NROS_IP` alongside the existing `NROS_LOCATOR`/`NROS_DOMAIN_ID`
   `option_env!` bakes) and `run_entry` performs the
   `SIOCSIFADDR` + urandom-reseed step before `Executor::open` — reusing
   `node::init_hardware`'s body; or
2. move the step into a NuttX-specific `BoardEntry::run` body extension.

Then add the deferred networked entry e2e (QEMU boots a `*_entry` image,
zenohd on the host slirp side, assert cross-process delivery).
