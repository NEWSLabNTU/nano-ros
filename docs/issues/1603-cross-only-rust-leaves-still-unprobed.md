---
id: 1603
title: "32 cross-only Rust example leaves still reach no metadata probe, and declare nothing — their pools stay at the crate defaults"
status: open
type: tech-debt
area: [tooling, build, examples]
severity: medium
found: 2026-10-01
related: [1265, 1556, 1061, 0827, 0288, rfc-0098]
---

## What

Issue 1265 made the node lib of a cross-only leaf build for the host by
TARGET-scoping what only the image needs (board crate, HAL, runtime, panic
handler) under `[target.'cfg(target_os = "none")'.dependencies]`, and applied it
to the 2 `esp32-c3-baremetal` leaves, the 5 non-RTIC `mps2-an385-baremetal`
leaves and the `nros new` baremetal/esp32 template. Every other cross-only
single-package Rust example still gets `<component>.json.unprobeable`, and none
of them DECLARES entities, so `nros sync` sizes their pools at the crate
defaults (issue 1061's fallback) — the direction that is safe for boot but
leaves no derivation and nothing that would notice a node outgrowing a default.

## Measured (2026-10-01, one `nros sync` per leaf in an agent worktree)

`tmp/`-scripted survey over every tracked `examples/*/rust/*/system.toml` beside
a `Cargo.toml`, excluding the host platforms (`native`, `threadx-linux`) and
`bridges`:

| leaves | first error the probe reports | what that says |
| --- | --- | --- |
| 8 `mps2-an385-baremetal/rust/*-rtic*` | `invalid instruction mnemonic 'bkpt'` | same shape 1265 fixed. 4 of the 8 (`talker-rtic`, `listener-rtic`, `*-rtic-mixed`) use `rtic::` in `lib.rs` itself, so target-scoping the deps is not enough there; the 4 service/action ones do not, and were not tried |
| 6 `zephyr/rust/*` | `no matching package named \`zephyr-build\`` | the Zephyr crates resolve only inside a west build; the probe's host cargo has no way to see them |
| 6 `mps2-an385-freertos/rust/*` | `a vendored source it resolves is not provisioned — run: nros setup --source freertos-kernel` | NOT a verdict: this worktree had no vendored sources. Whether the board crate's host-probe gates (issue 0288) let these probe on a provisioned host is unmeasured |
| 6 `qemu-armv7a-nuttx/rust/*` | same, `nuttx-libc` | same — unmeasured where provisioned |
| 6 `rv-virt-threadx/rust/*` | same, `threadx` | same — unmeasured where provisioned |

`stm32f4/rust/*` and `px4/rust/*` have no `system.toml` single-package shape and
were not in the survey.

## Fix direction

Per row, because the blockers differ:

* **RTIC** — split the RTIC app from the node (the node's `register` does not
  need `rtic::`), or accept these as a declared population with a cross-check.
* **Zephyr** — the probe needs a host stand-in for `zephyr`/`zephyr-build`, or
  the entities come from the west build's own artifact (1265's original
  "read from the cross-compiled artifact" direction).
* **FreeRTOS / NuttX / ThreadX** — first measure on a provisioned host; if they
  fail like mps2 did (target-only crates in `[dependencies]`), apply 1265's
  target-scoping.

Acceptance: each leaf either probes (a `metadata/<component>.json`, pools
derived) or is in a named, cross-checked declared population — none left at the
silent crate default.
