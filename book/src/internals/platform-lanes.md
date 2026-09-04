# Per-Platform Contributor Lanes

These are the in-tree `just` recipes contributors use to build fixture
sets, boot QEMU lanes, and run per-platform test sweeps from a nano-ros
checkout. Users never need them — the user path (setup, build, run with
the vendor's own tools) is on each platform's own page, which links back
here per platform. Every lane assumes `source ./activate.sh` (or
`direnv allow`) in the checkout first.

## FreeRTOS

Lanes for [FreeRTOS (QEMU MPS2-AN385)](../getting-started/freertos.md).

The prebuilt *test fixtures* bake per-language allocator ports instead
of 7447 (the copy-out examples' port) so suites run in parallel;
`just freertos talker` boots the fixture, so pair it with
`just freertos zenohd` (listens on 7800, the Rust pub/sub fixture port).

`just freertos build-fixtures` builds every in-tree zenoh + DDS example
across Rust / C / C++ in one pass.

`just freertos zenohd &` starts the router on the fixture port (7800)
and `just freertos talker` boots the prebuilt talker *fixture* in QEMU
instead of the copy-out example. For batch testing without manual QEMU
launches, `just freertos test` runs every E2E (pub/sub, service,
action) against a temporary in-test zenohd.

`just freertos build` / `just freertos test` build and exercise the
in-tree fixtures.

## ESP32

Lanes for [ESP32 (esp-hal, bare-metal Rust)](../getting-started/esp32.md).

Build:

```bash
# QEMU ESP32 (qemu-system-riscv32). `just esp32 build-qemu` (which
# `just esp32 talker` depends on) builds the QEMU-board variant; the
# example's build.rs invokes `nros generate-rust` automatically, so
# the `generated/` dir populates on first build (gitignored).
just esp32 build-qemu
```

Run (the router must already be listening on the port the example
dials — see the platform page):

```bash
# Boot the talker binary in qemu-system-riscv32 (esp32c3):
just esp32 talker
# Expected serial output (per src/lib.rs):
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   ...
```

The `just esp32 talker` recipe re-runs `build-qemu` every invocation,
so a first / cold run adds ~25 s of build time on top of the ~15 s
readiness signal.

The shorter deployment spelling:

```bash
just esp32 build
just esp32 talker
```

## NuttX

Lanes for [NuttX (QEMU)](../getting-started/nuttx.md) and
[Integration: NuttX external app](../getting-started/integration-nuttx.md).

Build (arm QEMU):

```bash
just nuttx build
```

This cross-compiles all NuttX examples for `armv7a-nuttx-eabi` using
`cargo +nightly build --release`.

Build (RISC-V `rv-virt`):

```bash
just nuttx build-riscv-c        # C example fixtures
just nuttx build-riscv-rust     # Rust example fixtures
```

Test:

```bash
just nuttx test        # arm QEMU integration tests
just nuttx test-all    # including the networked E2E lanes
```

Run: for nano-ros's own in-tree QEMU examples, `just nuttx zenohd &`
starts the router on the fixture port (8200) and `just nuttx talker`
wraps `qemu-system-arm` with the right wiring. `talker` there is the
Rust variant; the C / C++ variants boot through the `make`-driven path
described under "Auto-configure glue" in
[Integration: NuttX external app](../getting-started/integration-nuttx.md).

## Zephyr

Lanes for [Zephyr (native_sim)](../getting-started/zephyr.md) and
[Zephyr Integration (west module)](../getting-started/integration-zephyr.md).

E2E testing:

```bash
# Zenoh examples
just zephyr build           # Build Rust zenoh examples
just zephyr build-c         # Build C zenoh examples
just zephyr test            # Run zenoh E2E tests

# XRCE examples
just zephyr build-xrce      # Build all XRCE examples (Rust + C)
just zephyr test-xrce       # Run XRCE E2E tests

# All examples
just zephyr build-all       # Build everything
just zephyr ci              # Doctor + test (CI shortcut)
```

nano-ros's own zephyr talker has a matching recipe for the canonical
`native_sim` build path: `just zephyr talker`, paired with
`just zephyr zenohd &` (listens on the fixture port 7400).

To completely recreate the in-tree Zephyr workspace:

```bash
just setup zephyr --force
```

## Arm FVP

Lanes for [ARM FVP (`FVP_BaseR_AEMv8R`)](../getting-started/arm-fvp.md).

Doctor: `nros doctor --board fvp-aemv8r-smp` runs the FVP resolution
cross-check (it delegates part of its report to `just doctor`, so it
needs `just` on PATH). The `just zephyr run-fvp-ws-entry` /
`run-fvp-board-import` recipes do the equivalent inline via
`scripts/zephyr/resolve-fvp-bin.sh` and skip with a clear hint when the
binary can't be found.

Build: `just zephyr build-fvp-all` runs the FVP build lanes.

```bash
# The workspace C++ RT-tiers Entry — the canonical ASI-consumption
# reference (nano_ros_use_board + run_tiers).
just zephyr build-fvp-ws-entry

# The minimal board-crate IMPORT surface: nano_ros_use_board() and a
# trivial printk app, so the import path can be checked on its own.
just zephyr build-fvp-board-import
```

Each recipe shells `west build -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`
inside the `zephyr-workspace/` directory and produces `zephyr.elf` at:

- `zephyr-workspace/build-fvp-ws-entry/zephyr/zephyr.elf`
- `zephyr-workspace/build-fvp-board-import/zephyr/zephyr.elf`

Run (once the build artifacts and `ARM_FVP_DIR` / `ARMFVP_BIN_PATH` are
in place):

```bash
# Boot the workspace RT-tiers Entry (prints `[ctrl] tick=` / `[telem] tick=`).
just zephyr run-fvp-ws-entry

# Boot the board-import smoke (prints `nros: smoke ok`).
just zephyr run-fvp-board-import
```

Under the hood the recipe:

1. Verifies `west` + the Zephyr workspace + `zephyr.elf` exist;
   skips with a hint otherwise.
2. Resolves the FVP binary directory via
   `scripts/zephyr/resolve-fvp-bin.sh` (priority order:
   `ARMFVP_BIN_PATH` → `ARM_FVP_DIR/models/Linux64_GCC-*/` →
   `dirname $(command -v FVP_BaseR_AEMv8R)`).
3. Exports `ARMFVP_BIN_PATH=<dir>` and shells
   `west build -d <build-dir> -t run`, which drives Zephyr's
   `cmake/emu/armfvp.cmake` target with the canonical
   `boards/arm/fvp_baser_aemv8r/board.cmake` `-C` flags — UART
   0–3 piped to stdout, GICv3, cache state, NUM_CORES from
   `CONFIG_MP_MAX_NUM_CPUS`. No flags are duplicated in the
   `just` recipe.

Exit cleanly with `Ctrl-C`.

## ThreadX

Lanes for [ThreadX (Linux sim / RISC-V64 QEMU)](../getting-started/threadx.md).

`just <flavour> build-fixtures` produces `threadx_cpp_*` and
`riscv64_threadx_cpp_*` binaries alongside the Rust + C ones:

```bash
just threadx_linux build-fixtures     # build all rust + c examples
just threadx_riscv64 build-fixtures
```

`just threadx_riscv64 talker` boots the prebuilt riscv64 talker
fixture in `qemu-system-riscv64` with the virtio-net + Slirp wiring
baked in. For batch testing, `just threadx_linux test` runs every
pubsub / service / action against an in-test zenohd.

## Cyclone DDS

Lanes for the Cyclone DDS backend
([Choosing an RMW Backend](../user-guide/rmw-backends.md)). These are
contributor-only recipes — a bare `cmake` / `cargo` consumer build
needs no `just cyclonedds` pre-step; the consumer build self-provisions
Cyclone from source.

```bash
just setup cyclonedds       # build Cyclone DDS from third-party/dds/cyclonedds (tag 0.10.5)
just cyclonedds build-rmw   # build packages/rmw/cyclonedds/nros-rmw-cyclonedds
just cyclonedds test        # run the CTest harness
```

## Serial transport (QEMU)

Lane for [Serial Transport](../user-guide/serial-transport.md): the
integration test `test_qemu_serial_pubsub_e2e` in
`packages/testing/nros-tests/tests/emulator.rs` automates the full
QEMU serial pub/sub workflow. Run it with:

```bash
source ./activate.sh
just qemu build-fixtures                  # the test consumes a prebuilt
                                          # qemu-arm-baremetal fixture
cargo nextest run -p nros-tests --test emulator test_qemu_serial_pubsub_e2e
```

(A bare `cargo nextest` counts a skipped-precondition test as a FAILURE;
the contributor lane `just test-all` is what rewrites those into skips.
Read the panic text before treating a red here as a regression.)
