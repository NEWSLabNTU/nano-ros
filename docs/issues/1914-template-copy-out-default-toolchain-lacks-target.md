---
id: 1914
title: "`template-copy-out` builds a copied C++ template's message crates with the runner's default stable toolchain, which has no `thumbv7m-none-eabi`: E0463 can't find `core`"
status: open
type: bug
area: [ci, build, examples]
severity: medium
found: 2026-10-15
related: [1830, 1453, 1665, 1672]
---

## Measured

Run-matrix **38658400956** (workflow_dispatch, main `67e29e9fa`), job
**116173562884** `tier 2 (1-wise matrix)` on
`nano-ros-runner-newslab-118-server`, step `just ci matrix`, 00:20–00:57Z.
The tier stopped in its preflight; the companion job reports
`NO VERDICT: preflight failed before any cell completed`.

```
===== FAIL (template-copy-out, rc=1, 648201ms) =====
  Locking 5 packages to highest Rust 1.99.0 compatible versions
  Compiling byteorder v1.5.0
  error[E0463]: can't find crate for `core`
    = note: the `thumbv7m-none-eabi` target may not be installed
    = help: consider downloading the target with `rustup target add thumbv7m-none-eabi`
  FAILED: [code=101] nano_ros_cpp_ffi_builtin_interfaces/target/thumbv7m-none-eabi/nros-minsizerel/libnano_ros_cpp_ffi_builtin_interfaces.a …
```

12 `E0463`s across three copied templates: `cpp-port-minimal-publisher`,
`rclcpp-compat-smoke` and `topic-state-monitor-port`. Every one is a
`nano_ros_cpp_ffi_<msg>` crate built for `thumbv7m-none-eabi`.

## Reading (not yet confirmed by a rerun)

The copy is made in `/tmp/nros-template-copy-out.*`, outside the checkout, so
no `rust-toolchain.toml` governs it, and cargo resolves the runner's default
toolchain (`Rust 1.99.0`, the default `stable` that `nros setup` reports as
`already present (rustup, default stable-x86_64-unknown-linux-gnu)`).
`[host-env] ensuring rustup target: thumbv7m-none-eabi` earlier in the same
job adds the target to the repo's **pinned** toolchain, not to `stable`. So a
copied-out project, which is what a user gets, builds with a toolchain that
nothing provisioned for the board.

The failure becomes visible now because tier 2 is reaching its preflight
again. Earlier runs stopped sooner (issues 1901, 1908, 1899).

## What this is NOT

- **Not 1830.** That is a copied Rust package failing to resolve `nros`. Here
  resolution succeeds and compilation of `core` fails.
- **Not 1665.** That is Zephyr workspace Rust entries; these are FreeRTOS-board
  C++ templates on the cmake road.
- **Not a missing target on the runner overall.** The pinned toolchain has it.

## What would close this

Pick one, deliberately. The two sites are both defensible:

1. **The template pins its toolchain.** A copied-out project carries its own
   `rust-toolchain.toml`, or the cmake road passes `+<pinned>` to the cargo it
   runs, so a user's copy builds with the toolchain the board was provisioned
   for. This fixes the user's road, not just CI's.
2. **Provisioning covers the default toolchain.** `host-env` (or the runner
   image) adds every board target to the default toolchain too. This is
   cheaper, but it only works on a host that was provisioned this way.

Acceptance is a build: `just check template-copy-out` green on a runner where
`stable` has no `thumbv7m-none-eabi`.
