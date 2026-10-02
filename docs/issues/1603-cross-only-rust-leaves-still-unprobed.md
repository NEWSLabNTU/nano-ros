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

## 2026-10-03 -- 14 of the 32 probe now, 6 already did, 12 need a different answer

Measured with every vendored source this needs provisioned in the worktree
(`nros setup --source freertos-kernel lwip threadx threadx-netxduo nuttx-libc
nuttx-kernel nuttx-apps`), one `nros sync` per leaf, metadata wiped first.

| family | leaves | on a provisioned host, unchanged | change | after |
| --- | --- | --- | --- | --- |
| `mps2-an385-freertos/rust/*` | 6 | **all 6 PROBE** (`metadata/<component>.json`) -- the row above was the unprovisioned worktree, not the leaves; issue 0288's host-probe gates on the board crate already work | none | probed |
| `mps2-an385-baremetal/rust/*-rtic*` | 8 | `.unprobeable` (`bkpt`) | issue 1265's target-scoping: board, `nros-rmw-zenoh`, `mps2-an385-pac`, `rtic`, `cortex-m`, `panic-semihosting` under `[target.'cfg(target_os = "none")'.dependencies]` | **all 8 probe** |
| `rv-virt-threadx/rust/*` | 6 | `.unprobeable` (`E0152 duplicate lang item panic_impl`, then `E0463 can't find crate nros_board_threadx_qemu_riscv64`) | the same target-scoping (board, `nros-platform`, both RMW backends), AND `#![cfg(target_os = "none")]` on `src/app_main.rs` -- the lib IS the staticlib image here, and its glue module anchors the board | **all 6 probe** |
| `qemu-armv7a-nuttx/rust/*` | 6 | `.unprobeable` (`nros setup --source nuttx-kernel`; provisioned: `E0152 duplicate lang item in crate core: sized`) | target-scoping (`cfg(target_os = "nuttx")`) does NOT help -- reverted | not probed |
| `zephyr/rust/*` | 6 | `.unprobeable` (`no matching package named zephyr-build`) | none tried | not probed |

The correction to the table above: **no RTIC leaf's `lib.rs` uses `rtic::`.**
The four that the survey said did (`talker-rtic`, `listener-rtic`,
`*-rtic-mixed`) name `rtic` only in `//!` doc comments; the generated
`#[rtic::app]` lives in the entry, so target-scoping was enough for all eight.

**Measured derivation, before -> after** (`build/<image>/nros-cargo.toml`, the
derived `[env]` rows): before, NO derived row -- every pool at the crate
default. After, e.g. `talker-rtic` `NROS_EXECUTOR_MAX_CBS=1`,
`ZPICO_MAX_PUBLISHERS=1`, `NROS_RMW_SUBSCRIBER_SLOTS=0`; `service-server-rtic`
`NROS_DECLARED_SERVICE_SERVERS=1`, `NROS_CYCLONEDDS_MAX_KINDS=4`;
`rv-virt-threadx/action-server` `NROS_DECLARED_SERVICE_SERVERS=4`,
`NROS_EXECUTOR_ACTION_CLIENTS=1`, `ZPICO_MAX_PUBLISHERS=2`,
`NROS_CYCLONEDDS_MAX_KINDS=11`. The cross images still build:
`fixtures-build.sh baremetal rust` (all 13 baremetal rows, the 8 RTIC among
them) and `just threadx_riscv64 build-fixture-extras` (the six leaves, zenoh
and cyclonedds) exit 0, and the ThreadX images still export `app_main`
(`nm`), i.e. the cfg'd glue is in the target build.

### What is left (why this stays open)

* **NuttX -- the leaf's cargo config poisons a HOST build, and the probe cannot
  subtract it.** The NuttX target is tier 3, so the board's `cargo_config`
  carries `[unstable] build-std = ["core", "alloc", "panic_abort"]`, and the
  leaf's (sync-written, gitignored) `.cargo/config.toml` `include`s it -- the
  probe harness, run inside the leaf for the `[patch.crates-io]` rows, inherits
  it. `metadata_build`'s `CARGO_UNSTABLE_BUILD_STD=""` override was meant to
  neutralise exactly this and does not: cargo MERGES config arrays, so neither
  the empty env value nor `--config 'unstable.build-std=[]'` removes the
  included list (both measured: `E0152 duplicate lang item in crate core`).
  Building `std` from source for the host instead (`=std,panic_abort`) fails
  differently: the leaf's `libc` patch is the NuttX fork, and host `std` built
  against it does not compile (`E0599 no ... default for timespec`). The
  answer is a probe that does not inherit the board's `cargo_config` at all --
  run the harness OUTSIDE the leaf with only the patch rows (`nros-patch.toml`
  + the leaf's generated rows) named by `--config` -- which is a
  `metadata_build` change, not an example change.
* **Zephyr -- `zephyr` / `zephyr-build` resolve only inside a west build**, and
  the lib IS the image (`crate-type = ["staticlib"]`, `zephyr_component_main!`
  in `src/app_main.rs`, a `build.rs` that calls `zephyr_build`). A
  build-dependency cannot be target-scoped, so the host probe fails at
  RESOLUTION. Answers: split the node into its own crate the image depends on,
  or read the entities from the west build's artifact (1265's original
  direction).
* **(Revised direction for both, 2026-10-03 — see the section at the end.)**
* **Outside this issue's 32, the same shape**:
  `packages/testing/nros-tests/bins/rtic-run-plan-e2e` is `.unprobeable` and
  cannot take the target-scoping alone -- its `lib.rs` calls
  `nros_board_mps2_an385::exit_success()`.

## Revised direction (2026-10-03, against the unified build path)

phase-470 did not touch either remaining family: `qemu-armv7a-nuttx/rust/*`
stays a class-3 leaf (cargo owns the link) and `zephyr/rust/*` a class-4 leaf
(cmake owns the link, `rust_cargo_application()`). Both still exist and both
still take the LEAF producer of RFC-0100 D4 (`write_for_leaf`), which needs the
probe. What the unified path changed is that each blocker now has an in-tree
precedent to converge on, rather than a new mechanism to invent:

* **NuttX — converge the probe on the BUILD road's config convention.**
  RFC-0098 / phase-445 already moved the image build off the leaf's own cargo
  config: `nros build` runs cargo from the directory ABOVE the leaf with
  `--config build/<image>/nros-cargo.toml`. The gitignored leaf-local
  `.cargo/config.toml` survives for exactly two readers — a bare `cargo` run
  inside the leaf, and this probe — and it is the file that `include`s the
  board's `build-std`. So the fix recorded above (run the harness outside the
  leaf, with only the patch rows named by `--config`) is not a probe-specific
  workaround; it is the probe adopting the road the build already takes, and it
  leaves that leaf-local file with one reader fewer.
* **Zephyr — the split already exists, as the workspace shape.** phase-470 W5
  generated the Zephyr workspace entries: the node is an ordinary package that
  host-builds (so the probe answers for it), and the west application is
  GENERATED around it (`builder::west_app`, `rust_cargo_application()`). A
  standalone Zephyr leaf whose node lives in its own crate and whose
  `CMakeLists.txt` + `app_main.rs` are the thin image half is the same split
  done by hand in a copy-out leaf (RFC-0026 keeps leaves self-contained, so the
  leaf does not become a workspace). Prefer this over reading the west
  artifact: it makes the leaf probe like every other Rust package, where the
  artifact route would be a fourth way of learning entities.

Neither change belongs to RFC-0100; both are probe/leaf work. Files: NuttX —
`orchestration::metadata_build` (the harness invocation) only; Zephyr — the six
`examples/zephyr/rust/*` leaves only. No overlap with issues 1608 / 1647.
