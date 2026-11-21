# ARM FVP (`FVP_BaseR_AEMv8R`)

<!-- type: how-to · level: 2 -->

> **For maintainers.** The Arm FVP is maintainer-run: its board row is
> `cadence = "manual"` and no probe track replays this page. So the nano-ros
> Book does not teach it (RFC-0115 D4, RFC-0124 D8); its generated board page
> stays in the Book's reference. What follows is how to run it, for whoever
> maintains it or gives it a replaying track.

Run nano-ros on Arm's `Base_RevC AEMv8-R` Fast Models — the
Cortex-A SMP profile under Zephyr 3.7 (board id
`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`). The FVP is the
canonical local proxy for the safety-island reference platforms
that follow the same `hwv2` Zephyr shape; pair this chapter with
the [Zephyr (west module)](../../../book/src/guide/platforms/zephyr.md) starter for
the build half.

> **Status.** The run half — invoking `FVP_BaseR_AEMv8R` and piping
> UART 0–3 to stdout — is landed.
>
> **The two standalone talker lanes were retired**:
> their runners had been deleted, so they built images nothing booted. What
> remains is the part with consumers — the board-crate import surface and the
> workspace Entry. FVP support is intended to grow back from those. See
> [the archived FVP runtime roadmap](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-217-arm-fvp-local-runtime.md).
>
> **Verified where?** The run half is maintainer-run: a maintainer boots the
> image by hand (the recipe is in the contributor lanes linked below), and no
> installed probe track replays this chapter. `fvp-aemv8r-smp`'s status
> and the date of its last boot are on
> [its support evidence](../../reference/board-support-levels.html#fvp); a probe track follows
> an automated run, not the other way round. Read the output below as what a
> maintainer last saw, not as a gate.

## When to use

- You need to exercise a Cortex-A SMP Zephyr image on a developer
  laptop without dedicated silicon.
- You're bringing up a new `hwv2` safety-island target — the FVP
  is the closest in-tree reference for board.cmake + SMP boot +
  Cyclone DDS shape.
- You're validating stock-Cyclone-DDS RMW interop locally before
  promoting to a hardware bench.

The FVP is **not** a replacement for QEMU on Cortex-M (`mps2-an385`,
`mps3-an547`); those targets are covered by [FreeRTOS (QEMU)](../../../book/src/guide/platforms/freertos.md)
and the Zephyr starter. The FVP is also **not** the same surface
as Corellium's AVH cloud FVP — AVH packages firmware as `.coreimg`
and provisions via a remote API; local FVP loads a raw ELF via
`-a cluster0.cpu*=<elf>`. AVH is out of scope for this chapter.

## Prereqs

- A working Zephyr 3.7 workspace — run `nros setup zephyr` once if
  you haven't (see the [Zephyr starter](../../../book/src/guide/platforms/zephyr.md)).
- The Arm `Base_RevC AEMv8R` Fast Models binary, `FVP_BaseR_AEMv8R`.
  **It is user-provided:** nano-ros does not download, install or distribute
  it, and `nros setup` has no tool for it. **x86_64 Linux only** -- Arm
  publishes no other host build of this model. On another architecture you
  can still build the image; you cannot run it here.

### Install the FVP

1. Download the AEMv8-R FVP (`FVP_Base_AEMv8R`, Linux x86_64) from Arm's
   [Arm Ecosystem FVPs page](https://developer.arm.com/downloads/-/arm-ecosystem-fvps).
2. Unpack it wherever you keep tools, for example:

   ```bash
   mkdir -p ~/opt/fvp-aemv8r
   tar -xzf FVP_Base_AEMv8R_*_Linux_x86.tar.gz -C ~/opt/fvp-aemv8r --strip-components=1
   ```

3. Install its runtime dependency. The model's libraries link
   `libatomic.so.1` (Debian/Ubuntu `libatomic1`, Fedora `libatomic`, Arch
   `gcc-libs`); `nros setup --system` names it for your package manager.
4. Point nano-ros at it, in your shell rc:

   ```bash
   # Preferred -- Zephyr's canonical name: the directory holding the binary.
   export ARMFVP_BIN_PATH=~/opt/fvp-aemv8r/bin

   # Alternative -- the install root; the resolver looks at the root and
   # under `models/Linux64_GCC-*/` and `bin/`.
   export ARM_FVP_DIR=~/opt/fvp-aemv8r
   ```

   If neither is set, `FVP_BaseR_AEMv8R` is found on `PATH`. The order is
   `ARMFVP_BIN_PATH`, then `ARM_FVP_DIR`, then `PATH`; a setting that is set
   but does not hold the binary is reported, not skipped.
5. Check it: `FVP_BaseR_AEMv8R --version` prints `Fast Models [...]`.

`nros setup board fvp-aemv8r-smp` (and `nros setup fvp-aemv8r-smp`) does not
install the model either: it says the model is user-provided and reports
whether it is found, and through which setting. It succeeds either way, so a
bootstrap that runs it on a host without the model keeps working.

### Configure-cache trap

Zephyr's `cmake/emu/armfvp.cmake` looks the model up once, at **configure**,
and caches the answer. A build directory configured before the FVP was
installed, or before `ARMFVP_BIN_PATH` was set, caches `ARMFVP-NOTFOUND`, and
`west build -t run` then fails with
`/bin/sh: 1: ARMFVP-NOTFOUND: not found` even though the model is now there.
Re-configure that build directory: delete it, or rerun the configure
(`nros build demo_bringup:fvp -- --pristine`, or `west build -p always ...`).

### Doctor check

`nros doctor` reports whether the model is found, and through which setting
(`ARMFVP_BIN_PATH`, `ARM_FVP_DIR` or `PATH`). A miss is a warning naming the
install steps above, never a failure: a contributor who is not working on the
FVP should not be blocked by its absence. `nros doctor --board fvp-aemv8r-smp`
reports the same line.

> Changing this in the tree: see [Internals → per-platform lanes](platform-lanes.md#arm-fvp).

## Build

The FVP is a Zephyr board, `fvp-aemv8r-smp`, so an image names it in its
bringup's `system.toml` and `nros build` hands it to `west` like any Zephyr
image ([Zephyr native_sim](../../../book/src/guide/platforms/zephyr-native-sim.md#drive-west-yourself)
has the workspace flow). The worked example is
[`examples/workspaces/realtime-cpp`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/realtime-cpp/src/demo_bringup)'s
`[image.fvp]`, a Cyclone DDS image:

```toml
[image.fvp]
board = "fvp-aemv8r-smp"
rmw = "cyclonedds"
```

The build generates the messages from your ROS 2 install, so the first one
runs in a shell that has sourced it; it records the prefix, so later builds
need no sourced shell:

```bash
nros setup zephyr --rmw cyclonedds     # the Zephyr workspace + SDK
# the FVP itself: user-provided, see "Install the FVP" above
source /opt/ros/humble/setup.bash
nros build demo_bringup:fvp
```

The model must be resolvable when the build **configures** (see the
[configure-cache trap](#configure-cache-trap)). `nros build` says so in the
note it prints after the build.

## Run on the FVP

`nros build` ends by printing Zephyr's run target for the image's build
directory; it invokes `FVP_BaseR_AEMv8R` with the board's cluster and UART
settings and pipes the UARTs to your terminal:

```bash
west build -d build/zephyr-cyclonedds/demo_bringup__fvp_west -t run
```

> **Not replayed.** No probe track builds or boots the FVP image yet, so this line is unmeasured on the installed track ([tracking issue](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

The model is user-provided (see [Install the FVP](#install-the-fvp)); what
keeps it off a CI lane is cost and its x86_64-only host build. It runs until you stop it (Ctrl-C).

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
CDR framing must appear on both sides — that's the stock-RMW
interop contract (byte-compatible with `rmw_cyclonedds_cpp`).

## Cross-references

- [Environment Variables — `ARM_FVP_DIR` / `ARMFVP_BIN_PATH`](../../reference/environment-variables.html)
  — the discovery contract.
- [Zephyr (west module)](../../../book/src/guide/platforms/zephyr.md) — the parent
  Zephyr starter; the FVP is a board-target slice of it.
- [`examples/workspaces/realtime-cpp/src/demo_bringup/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/realtime-cpp/src/demo_bringup)
  — the workspace image (`[image.fvp]`) this chapter builds and boots. Its
  west application is generated by `nros build`; its Kconfig lives in
  `boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp/`.
