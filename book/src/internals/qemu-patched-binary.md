# Patched `qemu-system-arm` Binary

nano-ros ships a project-local QEMU build that the test harness uses
in preference to whatever `qemu-system-arm` is on `$PATH`. This
chapter explains why it exists, where it comes from, how a test picks
it up, and how to change it.

## Why

Two production-blocking issues motivated the patched build:

1. **LAN9118 RX FIFO drain bug** (mainline QEMU through at least
   11.0). Under sustained burst RX, the LAN9118 emulator's
   `lan9118_can_receive()` can stop returning true even after the
   guest drains the FIFO, so frames silently disappear. The fix is a
   commit on our fork branch (`hw/net/lan9118: flush queued packets when
   RX is enabled`).
   Bare-metal MPS2-AN385 RTPS / Zenoh tests on the system QEMU
   sporadically fail without it.

2. **`-netdev dgram,local.type=unix,…` requires QEMU 7.2+**. Ubuntu
   jammy ships QEMU 6.2, and the dgram-tunnel pattern is how
   NuttX / ThreadX DDS multi-instance tests cross-deliver frames
   between two QEMU processes after retired the
   broken `-netdev socket,mcast=` cross-process path. With a too-old
   system QEMU, those tests fall back to `[SKIPPED]`.

Both problems disappear once tests use the patched build.

## Where it comes from

The patches live on a branch of our QEMU fork,
`NEWSLabNTU/qemu` `nano-ros-v11.0.0-patches`, and
[nano-ros-sdk](https://github.com/NEWSLabNTU/nano-ros-sdk) publishes it as a
prebuilt dist. `nros-sdk-index.toml` pins it as one row, `[tool.qemu]`
(`version = "11.0.0-nros6"` at the time of writing — read the index, not this
line), with a `[tool.qemu.source]` recipe for a host that has no prebuilt.
There is no QEMU submodule in this tree any more.

Two places can hold it:

```
~/.nros/sdk/qemu/<version>/bin/qemu-system-arm   # the store: `nros setup --tool qemu`
build/qemu/bin/qemu-system-arm                   # project-local: `just qemu setup-qemu`
```

The store path is keyed by version, so it cannot drift from the pin. The
project prefix is ONE directory across pin bumps, which is why it carries the
`.nros-provenance` marker `nros setup` writes, naming the version it holds
(issue 0930). `just qemu setup-qemu` runs
`nros setup --tool qemu --prefix build/qemu`, which is a no-op when that marker
names the pinned version and replaces the prefix when it does not.

## How tests pick it up

ONE ladder (issue 1851), in `packages/testing/nros-tests/src/qemu.rs`
(`qemu_system_path`) and its shell twin `scripts/lib/qemu-bin.sh`
(`nros_qemu_bin`):

1. `QEMU_SYSTEM_<ARCH>` (`QEMU_SYSTEM_ARM`) — an explicit override.
2. The project prefix `build/qemu/bin/` — **only while its
   `.nros-provenance` names the version the index pins**. A prefix holding
   another version, or none (a hand build), is REFUSED with the remedy
   (`just qemu setup-qemu`, or remove `build/qemu`), never skipped: a test
   must not run on an emulator nobody chose. `just check tier-preconditions`
   reports the same thing before a run.
3. The store copy at the pinned version.
4. `$PATH`.

Every QEMU launch in the test crates, `just/` and `scripts/` goes through
it (`check-qemu-one-ladder`).

## Smoke test

`packages/testing/nros-tests/tests/qemu_patched_binary.rs` asserts:

- `qemu_system_arm_path()` resolves to either `QEMU_SYSTEM_ARM` or
  `<workspace>/build/qemu/bin/qemu-system-arm`.
- The patched binary reports version ≥ 7.2.
- `-netdev help` advertises `dgram` (the multi-instance backend).

Tests use `nros_tests::unmet!` when the patched build is absent —
a fresh clone without `just qemu setup-qemu` FAILS with a clear
`[UNMET PRECONDITION]` naming the remedy instead of silently passing.

## Adding a patch, or moving the pin

1. Land the change on the fork branch (`nano-ros-v11.0.0-patches`), one
   logical change per commit.
2. Cut a new nano-ros-sdk QEMU release from it.
3. Bump `[tool.qemu] version` and the dist rows in `nros-sdk-index.toml`.

Every checkout then sees its `build/qemu` as stale until it re-runs
`just qemu setup-qemu`; the store copy is fetched at the new version on the
next `nros setup --tool qemu`.

## Scope

Only `qemu-system-arm` is unified. `qemu-system-riscv32` is
Espressif's fork (different upstream, different patches);
`qemu-system-riscv64` and `qemu-system-aarch64` ship no patches
today. Other arches stay on the system binary until they
accumulate their own patches; the helper pattern is easy to copy
when that happens.
