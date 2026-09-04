---
id: 1064
title: "`set_ipv4` and `set_baudrate` have real bodies on seven boards and no caller anywhere — who writes a board's IP now the generator is gone?"
status: resolved
type: bug
area: build, platform
severity: medium
found: 2026-09-04
related: [1063, phase-206, phase-349, 0202]
---

# Five boards implement a setter nothing calls

## RESOLVED 2026-09-05 — measured, then deleted whole

The open question was whether `set_ipv4`'s five bodies and the deploy-overlay
path write the same fields. **They do not — the overlay is a strict superset**,
which settles it:

```rust
fn set_ipv4(&mut self, addr: [u8;4], prefix: u8) {   // freertos, the real one
    self.base.ip = addr;
    self.base.netmask = netmask_from_prefix(prefix);
}

pub struct DeployOverlay {
    ip, gateway, netmask, locator, domain_id, transport   // all Option<…>
}
```

It carries `gateway` as well — which is what the already-removed `set_gateway`
was for — and unlike the setters it is **read**: `nros-board-common/src/base_config.rs:102`
for the whole family, plus esp32-qemu, mps2-an385, its RTIC variant and
nuttx-qemu directly. Ten crates implement `run_with_deploy`.

So `BoardTransportConfig` was not a seam awaiting a caller. It was the **dead
twin of a live path**, and the live one is better. The trait, its two remaining
methods and all five board impls are removed.

`set_baudrate` was checked separately rather than assumed: both impls
(mps2-an385, esp32-qemu) are `#[cfg(feature = "serial")]` and only overwrite a
`Config` default of `115200`, which remains. Deleting the setter changes no
image. It does leave serial baud settable only by editing the board `Config` —
the deploy overlay has no baudrate field — but that gap existed already; the
setter merely implied an override that nothing invoked.

**On the breaking change:** the trait was public API, so an out-of-tree board
could implement it. Its impl does nothing today, so what breaks is code that
compiled and never ran — arguably the kinder failure.

Together with phase-206 W4 (issue 1067) this finishes the same finding from both
ends: the discoverable board contract and the executed one were different
things, and the executed one — `BoardEntry::run` plus the deploy overlay — is
now the only one.

## The measurement

phase-206 W5 swept all seven `BoardTransportConfig` setters
(`packages/platform/nros-platform/src/board/config.rs`) rather than only the one
it was sent for. Callers: **zero, for all seven.**

| method | impls | callers | W5 action |
| --- | ---: | ---: | --- |
| `set_interfaces` | 0 | 0 | deleted |
| `set_mac` | 0 | 0 | deleted |
| `set_gateway` | 0 | 0 | deleted |
| `set_ssid` | 0 | 0 | deleted |
| `set_password` | 0 | 0 | deleted |
| **`set_ipv4`** | **5** | **0** | **kept — this issue** |
| **`set_baudrate`** | **2** | **0** | **kept — this issue** |

`set_ipv4` has real bodies on `nros-board-threadx-linux`,
`nros-board-threadx-qemu-riscv64`, `nros-board-freertos`,
`nros-board-mps2-an385` and `nros-board-esp32-qemu`. The freertos one writes
into the bundled lwIP `base.ip` plus `netmask_from_prefix(prefix)` — it is not a
stub. `set_baudrate` has bodies on mps2-an385 and esp32-qemu.

## Why they were not deleted with the other five

Deleting a seam with **no impl and no caller** is a cleanup. Deleting one with
**five impls** is a decision, and a different one: those bodies encode how a
board takes an IP, and removing the trait method throws that away without
answering *who writes a board's IP now*.

The single writer of all seven was the orchestration generator, deleted with the
standalone-package pipeline in `11a00b0f8` (#202) along with
`orchestration/generate.rs`. So this is not "code that was never wired" — it is
**code whose caller was removed by an unrelated retirement**, the same shape
phase-206's own foundation suffered (three retirements, none aware of it).

## The question this issue owes an answer to

A `NanoRosOwned` board on an RTOS needs an IP, a netmask and a gateway from
somewhere. Today:

* the generator that used to call `set_ipv4` is gone;
* `nros::main!` reads a **deploy overlay** from the entry crate's `Cargo.toml`
  (`[package.metadata.nros.deploy.<board>]` → `DeployOverlayLit { locator, ip,
  gateway, netmask, … }`, `main_macro.rs:2884-2897`) and hands it to
  `BoardEntry::run_with_deploy` / `init_hardware_with_deploy`;
* so the deploy-overlay path may already be the live answer, and
  `BoardTransportConfig` the dead twin of it.

**If that is right, this is not "add a caller" — it is "delete seven impls and a
trait", and confirm every board's IP arrives through the overlay.** That is
worth measuring before either move. Nobody has checked whether the five
`set_ipv4` bodies and the overlay path agree on what they write.

## Why it matters beyond tidiness

Two boards implementing the same setter differently, with neither reachable, is
a trap for the next person adding a board: the trait is the discoverable thing,
so a new board implements it, and its IP silently never arrives. That is the
declared-but-unread class (phase-349; sibling issue 1063) with the arrow
reversed — an implementation nobody invokes rather than a declaration nobody
reads.

## Not covered

* Whether `set_ipv4`'s five bodies and `run_with_deploy`'s overlay write the
  same fields. Unmeasured.
* Whether any out-of-tree board implements `BoardTransportConfig`. The trait is
  public API, so removing it is a breaking change for a consumer we cannot see.
* `set_baudrate`'s two bodies — same shape, serial rather than IP, and probably
  the same answer.
