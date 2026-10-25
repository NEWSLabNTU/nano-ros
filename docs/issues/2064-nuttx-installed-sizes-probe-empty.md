---
id: 2064
title: "An installed `nros build` of a NuttX C image fails: the sizes probe returns 0"
status: open
type: bug
area: [nuttx, build, installed-sdk]
severity: high
found: 2026-10-26
related: [phase-517, RFC-0099, issue-0472, issue-0665]
---

## What

Replaying `integration-nuttx.md` on the installed track (`just probe nuttx-arm`,
`just probe nuttx-riscv` — phase-517 W10) gets through `nros setup`, the
scaffold (`nros new my_app --platform nuttx --lang c`), the configure and the
NuttX kernel build, then fails compiling `nros-c` inside the board FFI crate's
cargo build:

```text
warning: nros-c@0.6.0: nros-c: EXECUTOR_SIZE probe returned 0 — likely a
  `cargo check --no-default-features` run. The emitted `EXECUTOR_OPAQUE_U64S`
  will be 1; do not link the resulting rlib.
error[E0080]: evaluation panicked: SUBSCRIPTION_OPAQUE_U64S: the generated
  header states a SMALLER opaque size than the Rust type needs …
  --> …/share/nano-ros/packages/api/nros-c/src/opaque_sizes.rs:130:23
(and the same for SERVICE_SERVER, SERVICE_CLIENT, ACTION_SERVER, ACTION_CLIENT
 and EXECUTOR_OPAQUE_U64S)
error: could not compile `nros-c` (lib) due to 6 previous errors
```

`nros-cpp` reports the same zero (`CPP_EXECUTOR_OPAQUE_U64S`). Both NuttX
boards (`qemu-armv7a-nuttx`, `rv-virt-nuttx`) fail identically; every other QEMU
chapter (bare-metal, FreeRTOS, ThreadX Linux + RISC-V 64) builds and runs on the
same installed asset.

So on an installed SDK the sizes probe (`nros-sizes-build`, the nested cargo
build that reads the `__NROS_SIZE_*` symbols) yields no sizes for the
`armv7a-nuttx-eabihf` / `riscv32imac-unknown-nuttx-elf` build-std targets, and
the per-build header falls back to 1-word opaques that the compile-time guard
rightly refuses. The same images build from a checkout (the in-tree NuttX
fixture lanes), so the difference is something a checkout supplies and the
staged SDK root (`scripts/stage-sdk-root.sh`) does not — not yet located.

## Measured so far

Three earlier installed-track blockers on the same road were fixed in phase-517
W10 and are not this issue: `nros setup` now installs the pinned nightly +
`rust-src` the build-std boards need; the rv-virt provision script gets the
board's `NUTTX_CROSS`; and the FFI crate's cargo command carries the
`nros-minsizerel` profile DEFINITION (`nros profile env`), which only a
checkout's root `.cargo/config.toml` supplied before.

Leads, unverified: the probe build is reached through the FFI crate's own
workspace with `lto = "fat"` (bitcode-only rlibs have no symbol table to read),
or it resolves a path or patch only a checkout has. Reproduce with
`just probe nuttx-arm` (≈6 min with a prebuilt asset: `just probe
platform-asset`, then `PROBE_ASSET=<dir> PROBE_KEEP=1 just probe nuttx-arm` and
inspect the stopped container).

## Also seen on this road

`nros setup qemu-armv7a-nuttx` does not ask for the two host tools the NuttX
kernel build needs — Python's venv module (the build provisions `kconfiglib`
into a venv when `kconfig-frontends` is absent) and `unzip` (NuttX's own build
downloads and unzips `dtc` and `libmetal`), though the index declares
`[prereq.python3-venv]` and `[prereq.kconfig-frontends]`. The chapter now names
them as a step; the setup plan should ask for them as it does a tool's
`system` libraries.

## Acceptance

`just probe nuttx-arm` and `just probe nuttx-riscv` print the chapter's
documented `my_app: published 0 on /chatter` and pass.
