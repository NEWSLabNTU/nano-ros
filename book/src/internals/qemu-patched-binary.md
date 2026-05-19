# Patched `qemu-system-arm` Binary

nano-ros ships a project-local QEMU build that the test harness uses
in preference to whatever `qemu-system-arm` is on `$PATH`. This
chapter explains why it exists, how it gets built, how a test picks
it up, and how to add a new patch.

## Why

Two production-blocking issues motivated the patched build:

1. **LAN9118 RX FIFO drain bug** (mainline QEMU through at least
   11.0). Under sustained burst RX, the LAN9118 emulator's
   `lan9118_can_receive()` can stop returning true even after the
   guest drains the FIFO, so frames silently disappear.
   bisected this and ships the fix as
   `third-party/qemu/patches/0001-hw-net-lan9118-add-can_receive-flush-on-FIFO-drain.patch`.
   Bare-metal MPS2-AN385 RTPS / Zenoh tests on the system QEMU
   sporadically fail without it.

2. **`-netdev dgram,local.type=unix,…` requires QEMU 7.2+**. Ubuntu
   jammy ships QEMU 6.2, and the dgram-tunnel pattern is how
   NuttX / ThreadX DDS multi-instance tests cross-deliver frames
   between two QEMU processes after retired the
   broken `-netdev socket,mcast=` cross-process path. With a too-old
   system QEMU, those tests fall back to `[SKIPPED]`.

Both problems disappear once tests use the patched build at
`build/qemu/bin/qemu-system-arm`.

## Layout

```
third-party/qemu/qemu/              # submodule, pinned to stable-11.0
third-party/qemu/patches/           # patch series, applied on top
  0001-hw-net-lan9118-…patch
build/qemu/bin/qemu-system-arm      # final installed binary
build/qemu/share/qemu/…             # firmware, etc.
```

The submodule URL and pin live in `.gitmodules`:

```
[submodule "third-party/qemu/qemu"]
    path = third-party/qemu/qemu
    url = https://gitlab.com/qemu-project/qemu.git
    branch = stable-11.0
```

`stable-11.0` is QEMU 11.0.x — already past the 7.2 cutoff for
`-netdev dgram unix` and recent enough that the patch series is
small.

## Build

`just qemu setup-qemu` (pulled in by `just setup`) does the full
build:

```bash
just qemu setup-qemu
```

The recipe:

1. Inits the submodule shallowly if it is not already present.
2. Short-circuits when `build/qemu/bin/qemu-system-arm` is newer
   than every file under `third-party/qemu/patches/` (touch a patch
   to force a rebuild).
3. Resets the submodule to its pinned tip, applies every `.patch`
   under `third-party/qemu/patches/` in alphabetical order via
   `git apply`.
4. Configures with `--target-list=arm-softmmu` (no other arches, no
   docs, no tools) and `--enable-slirp`.
5. `make -j$(nproc) && make install` into `build/qemu/`.

End-to-end cost is roughly ten minutes the first time and ~150 MB
of disk. Subsequent runs are no-ops.

`just qemu doctor` reports the build status and clearly distinguishes
the patched build from the system fallback.

## How tests pick it up

The single resolver lives in
`packages/testing/nros-tests/src/qemu.rs`:

```rust
pub fn qemu_system_arm_path() -> std::ffi::OsString { … }
pub fn qemu_system_arm_cmd()  -> std::process::Command { … }
```

Selection order:

1. `QEMU_SYSTEM_ARM` env var — developer override / CI pin.
2. Project-local `<workspace>/build/qemu/bin/qemu-system-arm`
   when it exists (auto-detected via `CARGO_MANIFEST_DIR` walk
   to the workspace `Cargo.toml`).
3. System `qemu-system-arm` on `$PATH` — kept as fallback so a
   minimal install still produces a clean `[SKIPPED]` rather than
   an exec error.

Every `Command::new("qemu-system-arm")` in the test crates goes
through this helper. New tests must use it.

`just/qemu-baremetal.just`, `just/nuttx.just` and `just/freertos.just`
do the same in shell, gating their `qemu-system-arm` invocations
through:

```just
QEMU_BIN := absolute_path("build/qemu") / "bin/qemu-system-arm"

# inside a recipe:
{{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt …
```

## Smoke test

`packages/testing/nros-tests/tests/qemu_patched_binary.rs` asserts:

- `qemu_system_arm_path()` resolves to either `QEMU_SYSTEM_ARM` or
  `<workspace>/build/qemu/bin/qemu-system-arm`.
- The patched binary reports version ≥ 7.2.
- `-netdev help` advertises `dgram` (the multi-instance backend).

Tests use `nros_tests::skip!` when the patched build is absent, per
the project convention from `CLAUDE.md`'s "Tests must fail on
unmet preconditions" rule — a fresh clone without
`just qemu setup-qemu` surfaces a clear `[SKIPPED]` with the
suggested remedy instead of silently passing.

## Adding a new patch

1. Land the upstream fix or write a downstream-only patch against
   the pinned submodule tip.
2. Save it as a numbered file under `third-party/qemu/patches/`
   (e.g. `0002-hw-net-…patch`). Keep one logical change per patch.
3. Bump any inline comment that names specific patches.
4. Touch the patch file (or just commit it) — `just qemu setup-qemu`
   detects the patch is newer than the installed binary and
   rebuilds.
5. Bump the relevant CI cache key (see) so other
   machines also rebuild.

## Submodule pin bump

When upstream rolls a new stable branch and an existing patch
either lands upstream or no longer applies cleanly:

1. Edit `.gitmodules` `branch = stable-…` to the new branch name.
2. `cd third-party/qemu/qemu && git fetch && git checkout
   origin/stable-…`
3. Re-run `just qemu setup-qemu`. If a patch fails to apply,
   either drop it (landed upstream) or rebase it onto the new tip.
4. Commit the submodule SHA bump together with any patch-series
   updates.

## Scope

Only `qemu-system-arm` is unified. `qemu-system-riscv32` is
Espressif's fork (different upstream, different patches);
`qemu-system-riscv64` and `qemu-system-aarch64` ship no patches
today. Other arches stay on the system binary until they
accumulate their own patches; the helper pattern is easy to copy
when that happens.
