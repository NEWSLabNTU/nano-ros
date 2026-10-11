---
id: 1277
title: "`nros build` preflight demands a prebuilt `rust-std` for a board whose
  `cargo_config` builds `core` from source (`[unstable] build-std`)"
status: resolved
type: bug
area: cli, build
severity: low
related: [phase-445]
resolved_in: "1f3b0d4ad5"
---

## Resolved (2026-10-11)

Fixed by `1f3b0d4ad5` (2026-10-05) along the fix direction below. Preflight
treats a board whose `cargo_config` states a non-empty `[unstable] build-std`
as a build-std target and checks `rust-src` instead of the target's
`rust-std`. An unparseable config falls back to `config/rust-targets.txt`, as
before. The issue was never closed.

Re-measured on main 2026-10-11 (phase-445 re-measure):
`RUSTUP_TOOLCHAIN=nightly-2026-04-11 nros build demo_bringup:esp32 --workspace . --offline`
in `examples/workspaces/rust`, on a nightly with no `riscv32imc` target
installed. Preflight passed and the build went straight to compiling `core`
from `rust-src`. That is the refusal this issue quotes, gone.

Not measured, recorded as a hypothesis rather than a finding:
`rust_component_installed` asks `rustup component list` in the PREFLIGHT
process. If a build's toolchain ever came from somewhere that process cannot
see (a `rust-toolchain.toml` under the generated entry rather than the
caller's env), the check would read the wrong toolchain. The measured case
passes the toolchain through `RUSTUP_TOOLCHAIN`, which both share.


## What happens

`nros build demo_bringup:esp32` (examples/workspaces/rust, board `esp32-qemu`)
stops at stage 3 on a host whose row toolchain (`RUSTUP_TOOLCHAIN =
nightly-2026-04-11`, from the fixture row's `env`) has no prebuilt std for the
triple:

```text
Error: missing prerequisites for this build:
  - Rust target `riscv32imc-unknown-none-elf` (board `esp32-qemu`)
      run: rustup target add riscv32imc-unknown-none-elf
```

Measured 2026-09-10 in a fresh worktree, BEFORE any phase-445 change, while
taking the W4 baseline. `rustup target add --toolchain nightly-2026-04-11
riscv32imc-unknown-none-elf` cleared it.

## Why it is wrong

The board descriptor's `cargo_config` says `[unstable] build-std = ["core",
"alloc"]`: the image compiles `core` from `rust-src` and never links the
prebuilt `rust-std` the remedy installs. `builder::preflight::check` asks
`target_provisioning(nano_ros_root, target)`, which answers `Rustup` whenever a
prebuilt std EXISTS for the triple (riscv32imc is tier 2) — it does not look at
whether this board uses it. So the requirement it states is one the build does
not have, and the one it does have (`rust-src` on the ROW's toolchain) is not
checked.

It also reads `rustup target list --installed` for the toolchain the preflight
process runs under, not for the `RUSTUP_TOOLCHAIN` the build will use.

## Fix direction

When the descriptor's `cargo_config` declares `[unstable] build-std` (now
rendered into the image's `build/<coord>/<entry>/nros-cargo.toml`, phase-445
W4), preflight should require `rust-src` on the build's toolchain and stop
asking for the target's `rust-std`.

## Seen on CI: it stops tier 2's fixture build (2026-10-03)

The first sighting outside a dev host. Scheduled `run-matrix` run
**37103213105** (head `16e307742`), job **111146651304** `tier 2 (1-wise matrix)`,
first failing step `just build tier2`. The `esp32` family fails at the
workspace image, and the error is the one this issue quotes, verbatim:

```text
nros build demo_bringup:esp32 --workspace . --offline -- --profile nros-relwithdebinfo --target-dir target-fixtures/esp32
Error: missing prerequisites for this build:
  - Rust target `riscv32imc-unknown-none-elf` (board `esp32-qemu`)
      run: rustup target add riscv32imc-unknown-none-elf
nothing was built.
```

Earlier in the same `esp32.log`, two images for the same triple had already
built: `esp32-qemu-listener.bin`, and
`logging-smoke-esp32-qemu` (157,872 B, flash image written). Both use the
`cargo` road, which builds `core` from source. So the runner can build
`riscv32imc-unknown-none-elf`. Only the `nros build` preflight says it cannot,
which is this issue's "the requirement it states is one the build does not
have".

The other families in that run reported `OK`: native, freertos, nuttx, qemu,
threadx_linux, threadx_riscv64. Not the cause: `provision-zenohd`
exited 78 earlier in the same job. That is a reported lane skip (`lane
skipped: not root: ros-humble-rmw-zenoh-cpp not installed`), not this failure.

Severity: `low` may undersell it now. In this run it took the tier-2 lane's
whole fixture build red with one false refusal (one run observed so far).
