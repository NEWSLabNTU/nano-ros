# ARM FVP (`FVP_BaseR_AEMv8R`)

Run nano-ros on Arm's `Base_RevC AEMv8-R` Fast Models — the
Cortex-A SMP profile under Zephyr 3.7 (board id
`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`). The FVP is the
canonical local proxy for the safety-island reference platforms
that follow the same `hwv2` Zephyr shape; pair this chapter with
the [Zephyr (west module)](./integration-zephyr.md) starter for
the build half.

> **Phase 217.** The build half (`just zephyr build-fvp-aemv8r{,-cyclonedds}`)
> shipped in Phase 117.13–117.14; the run half — invoking
> `FVP_BaseR_AEMv8R` and piping UART 0–3 to stdout — landed as
> Phase 217.A on 2026-06-03. See
> [`docs/roadmap/phase-217-arm-fvp-local-runtime.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-217-arm-fvp-local-runtime.md).

## When to use

- You need to exercise a Cortex-A SMP Zephyr image on a developer
  laptop without dedicated silicon.
- You're bringing up a new `hwv2` safety-island target — the FVP
  is the closest in-tree reference for board.cmake + SMP boot +
  Cyclone DDS shape.
- You're validating Phase 117 stock-RMW interop locally before
  promoting to a hardware bench.

The FVP is **not** a replacement for QEMU on Cortex-M (`mps2-an385`,
`mps3-an547`); those targets are covered by [FreeRTOS (QEMU)](./freertos.md)
and the Zephyr starter. The FVP is also **not** the same surface
as Corellium's AVH cloud FVP — AVH packages firmware as `.coreimg`
and provisions via a remote API; local FVP loads a raw ELF via
`-a cluster0.cpu*=<elf>`. AVH is out of scope for this chapter.

## Prereqs

- A working Zephyr 3.7 workspace — run `nros setup zephyr` once if
  you haven't (see the [Zephyr starter](./integration-zephyr.md)).
- The Arm `Base_RevC AEMv8R` Fast Models binary — **license-gated**.
  Download from
  [developer.arm.com — Arm Ecosystem FVPs](https://developer.arm.com/downloads/-/arm-ecosystem-fvps)
  after accepting the EULA. nano-ros does not download it (the
  `[gated.arm-fvp]` row in `nros-sdk-index.toml` declares the
  tool but the fetch is your responsibility — same policy as the
  NVIDIA Orin SPE FSP).

After installing, export one of the discovery env vars:

```bash
# Preferred — Zephyr's canonical name; takes highest priority.
export ARMFVP_BIN_PATH=/opt/Arm/FastModels/Base_RevC_AEMv8R/models/Linux64_GCC-9.3

# Alternative — directory layout from the gated installer; the
# resolver scans `models/Linux64_GCC-*/` underneath it.
export ARM_FVP_DIR=/opt/Arm/FastModels/Base_RevC_AEMv8R
```

If neither is set, `FVP_BaseR_AEMv8R` is discovered via `PATH` as
a last-ditch fallback.

### Installer surface (Phase 217.B.1)

After extracting the Arm FVP tarball, run the discovery script:

```bash
ARM_FVP_DIR=/path/to/extracted/fvp \
    scripts/installers/arm-fvp-installer.sh
```

The installer locates `FVP_BaseR_AEMv8R` under `$ARM_FVP_DIR`,
symlinks the containing directory to
`~/.nros/sdks/arm-fvp/current/` (atomic via `ln -sfn`), and
prints the `export ARMFVP_BIN_PATH=…` line for your shell rc.
Run `scripts/installers/arm-fvp-installer.sh --print-env` later
to re-emit the export. It never downloads anything — gated-tool
policy.

### Doctor check (Phase 217.B.2)

`nros doctor --board fvp-aemv8r-smp` cross-checks the
`[gated.arm-fvp]` entry in `nros-sdk-index.toml` and warns (never
hard-fails — gated) when the FVP can't be resolved via
`ARMFVP_BIN_PATH`, `ARM_FVP_DIR`, `PATH`, or the canonical
`~/.nros/sdks/arm-fvp/current/FVP_BaseR_AEMv8R` landing path. The
`just zephyr run-fvp-aemv8r{,-cyclonedds}` recipes do the
equivalent inline via `scripts/zephyr/resolve-fvp-bin.sh` and
skip with a clear hint when the binary can't be found.

## Build

The build half is unchanged from Phase 117.13 / 117.14:

```bash
# Phase 117.13 — Zephyr-only talker.
just zephyr build-fvp-aemv8r

# Phase 117.14 — C++ pub/sub over Cyclone DDS, the wire-compat
# reference for the safety-island slice.
just zephyr build-fvp-aemv8r-cyclonedds
```

Each recipe shells `west build -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`
inside the `zephyr-workspace/` directory and produces
`zephyr.elf` at one of:

- `zephyr-workspace/build-fvp-aemv8r-talker/zephyr/zephyr.elf`
- `zephyr-workspace/build-aemv8r-cyclonedds-talker/zephyr/zephyr.elf`

## Run

Once the build artifacts and `ARM_FVP_DIR` / `ARMFVP_BIN_PATH` are in
place:

```bash
# Boot the Phase 117.13 talker.
just zephyr run-fvp-aemv8r

# Boot the Phase 117.14 cpp/cyclonedds talker.
just zephyr run-fvp-aemv8r-cyclonedds
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

## Expected output

The Zephyr 3.7 boot banner appears on UART0 first, followed by
the talker. The exact line counts depend on `CONFIG_BOOT_BANNER`
and your locator config, but the markers to look for are:

```text
*** Booting Zephyr OS build v3.7.0 ***
[00:00:00.xxx,000] <inf> nros: session up (domain 0)
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
...
```

For the cpp/cyclonedds recipe, the same banner is followed by
Cyclone DDS reader-match logs and `std_msgs/String` publish lines.
Verify ROS 2 interop by running a sibling listener in another
terminal:

```bash
# stock ROS 2 — reads the FVP's Cyclone DDS publisher
ros2 topic echo /chatter std_msgs/msg/String
```

The same `std_msgs/String` payload (`Hello World: N`) + byte-equal
CDR framing must appear on both sides — that's the Phase 117
stock-RMW interop contract.

## Cross-references

- [`docs/roadmap/phase-217-arm-fvp-local-runtime.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-217-arm-fvp-local-runtime.md)
  — the runtime slice. Track A (run recipes) landed; B (installer
  + doctor), C (smoke test), D (Rust example), E (this chapter)
  ongoing.
- [`docs/roadmap/archived/phase-117-cyclonedds-rmw.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-117-cyclonedds-rmw.md)
  — Phase 117.13 (Zephyr FVP build smoke) + 117.14 (Cyclone DDS
  port + cpp talker) are the build smokes the runtime exercises.
- [Environment Variables — `ARM_FVP_DIR` / `ARMFVP_BIN_PATH`](../reference/environment-variables.md)
  — the discovery contract.
- [Zephyr (west module)](./integration-zephyr.md) — the parent
  Zephyr starter; the FVP is a board-target slice of it.
- [`examples/zephyr/cpp/cyclonedds/talker-aemv8r/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/cpp/cyclonedds/talker-aemv8r/README.md)
  — the example walk-through.
