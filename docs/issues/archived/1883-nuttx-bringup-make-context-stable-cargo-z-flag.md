---
id: 1883
title: "The NuttX per-bringup apps template could not build an image: stable cargo with `-Z`, a manifest no bringup has, and a symbol nothing emits"
status: resolved
type: bug
area: [nuttx, build]
severity: medium
found: 2026-10-13
resolved: 2026-10-14
related: [1396, 1850, 1880, phase-212, phase-258, RFC-0003, RFC-0065]
---

## Measured

The fixture `packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx`
was staged with `scripts/nuttx/stage-external-apps.sh <apps> --bringup
<fixture>/src/demo_bringup`, the documented flow. Then `make context` ran in
the configured NuttX tree. `nros codegen-system` succeeded, and then the
cargo step failed:

```
error: the `-Z` flag is only accepted on the nightly channel of Cargo
```

## Cause

The road had three separate breaks. The first one hid the other two.

1. **Toolchain.** The per-bringup `Makefile`
   (`integrations/nuttx/apps-external-template/`) includes only `Make.defs`.
   The nightly pin (`RUSTUP_TOOLCHAIN`), the armv7a triple fallback and
   `NROS_CARGO_BUILD` are all defined in the integration shell's `Makefile`,
   which this app never reads. So `NROS_TARGET_TRIPLE` was empty, the rule
   took upstream `RUST_CARGO_BUILD` (`-Zbuild-std`), and cargo ran on the
   repo's pinned stable toolchain.
2. **Manifest.** The cargo call used `--manifest-path
   $(NROS_BRINGUP_WORKSPACE)/demo_bringup/Cargo.toml`. A bringup has no
   `Cargo.toml` by design, and it lives at `<ws>/src/<bringup>` anyway. The
   bake's own `nros-system/Cargo.toml` stub lists members by bare package name
   relative to the bake directory, so it does not resolve either.
3. **Link.** The fixture's `nuttx_app_glue/main.c` forwards to
   `nros_system_main()`. `nros codegen-system` stopped emitting that in Phase
   258 (`codegen_system.rs`: "Adapters route through the typed `_install`
   carrier + `nros::main!`").

No lane built this road. `cli_bringup_nuttx` checked file shapes and the
staged pinning only, and `docs/reference/canonical-build-path.md` names no
NuttX make road. So all three breaks stayed invisible.

## Resolution — retired

A NuttX workspace image already has a road that builds and runs: the cargo
road. You declare `[image.<id>] board = "qemu-armv7a-nuttx"` in the bringup's
`system.toml` and run `nros build`. See `examples/workspaces/rust`
`[image.nuttx]` and the fixture row `workspace-rust-qemu-nuttx`. Reviving the
apps template would add a fourth road with its own knob carrier, which is the
case RFC-0065 D3 rules against ("passive accommodation stays, active routing
goes"). So it was retired, the same way the `idf.py` driver was:

- Deleted `integrations/nuttx/apps-external-template/`, the fixture
  `multi_pkg_workspace_nuttx`, and `tests/cli_bringup_nuttx.rs` (plus its
  three rows in `.config/lane-admission/gate.txt`).
- `scripts/nuttx/stage-external-apps.sh` stages the integration shell only.
  `--bringup`, `--image` and `--copy` now refuse with exit 2, naming this
  issue and the cargo road, instead of silently staging nothing.
- `check-nuttx-integration-makefile` and `check-shim-codegen-target` no longer
  name the template. The latter keeps its pre- and post-1396 NuttX text as
  self-test input.
- Book `integration-nuttx.md`, RFC-0003 §4, `just/nuttx.just` and the
  integration `Makefile` comments now point at the cargo road.

The integration shell (`integrations/nuttx/Makefile`, which builds
`nros-c`/`nros-cpp` for C/C++ NuttX apps) is untouched. It sets its own
nightly pin and triple, and `just nuttx build-integration-app` builds it.

## Verified

- `stage-external-apps.sh <apps> --bringup x` prints the refusal and exits 2.
  Run without flags, it stages `external/{Make.defs,Kconfig,nano-ros}` as
  before.
- `check-nuttx-integration-makefile` and `check-shim-codegen-target` pass.
