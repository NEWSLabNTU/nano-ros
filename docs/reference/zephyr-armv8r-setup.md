# Zephyr Toolchain Prep — ARMv8-R Targets (Phase 117.10 / 117.11)

`autoware-safety-island` runs on two Zephyr boards that nano-ros
will support in Phase 117.10 / 117.11:

| Board                          | ISA                       | Toolchain          | Runtime |
|--------------------------------|---------------------------|--------------------|---------|
| `fvp_baser_aemv8r_smp`         | AArch64 ARMv8-R, SMP      | `aarch64-zephyr-elf` | ARM Base RevC AEMv8R FVP simulator |
| `s32z270dc2_rtu0_r52@D`        | AArch32 ARMv8-R Cortex-R52 | `arm-zephyr-eabi`    | NXP S32Z270 evaluation board |

This page covers the **manual steps** required before `just zephyr
setup` produces a workspace that can build for these boards. The
out-of-tree FVP simulator and NXP-licensed board files cannot be
auto-fetched by the setup script.

## 1. Install the AArch64 toolchain

The default `just zephyr setup` line in
`scripts/zephyr/setup.sh` installs `x86_64-zephyr-elf` and
`arm-zephyr-eabi`. Phase 117.10 needs `aarch64-zephyr-elf` too.

```bash
# Forwards through `just zephyr setup *ARGS` to scripts/zephyr/setup.sh.
just zephyr setup --phase-117
```

This adds `-t aarch64-zephyr-elf` to the SDK installer's target
list. Running it on a workspace that already has the default SDK
re-runs `setup.sh -t aarch64-zephyr-elf -h -c` and installs only
the missing toolchain (idempotent — `setup.sh` skips already-
installed targets).

To add a different target ad-hoc:

```bash
just zephyr setup --target=mips-zephyr-elf
```

`--target` is repeatable.

## 2. Install the ARM Base RevC AEMv8R FVP

The FVP is a free download from ARM (registration required, EULA-
gated; nano-ros cannot redistribute it).

1. Register at <https://developer.arm.com/downloads/-/arm-ecosystem-fvps>.
2. Download "Armv8-R AEM FVP" (`AEMv8R_base_pkg_*.tgz`).
3. Extract into `~/opt/arm/fvp/` (or any path on `$PATH`):

```bash
mkdir -p ~/opt/arm
tar -xzf ~/Downloads/FVP_Base_AEMv8R_*.tgz -C ~/opt/arm
~/opt/arm/FVP_Base_AEMv8R/license_terms/license_agreement.txt   # accept terms
```

4. Add the FVP `bin/` to `$PATH` (or symlink the binary):

```bash
echo 'export PATH="$HOME/opt/arm/FVP_Base_AEMv8R/models/Linux64_GCC-9.3:$PATH"' >> ~/.bashrc
```

5. Verify:

```bash
FVP_BaseR_AEMv8R --version
# Fast Models [11.x.y]
```

Zephyr's `west build -b fvp_baser_aemv8r_smp` and `west flash`
both invoke the FVP via `FVP_BaseR_AEMv8R` on `$PATH`.

## 3. NXP S32Z270 board files

Cortex-R52 board support is in upstream Zephyr (`boards/arm/
s32z270dc2_r52`). No external download needed for the build —
runtime testing requires either:

- The NXP S32Z270 evaluation board (license-gated, contact NXP).
- ARM's separate Cortex-R FVP (different from AEMv8R; not the same
  binary as Step 2). Slow + unverified for our use case.

Phase 117.11's acceptance is **build-only** for this reason.

## 4. Verify the toolchain

After Steps 1-3 (Step 3 only if you have the board), check:

```bash
just zephyr doctor
$ZEPHYR_SDK_INSTALL_DIR/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc --version
$ZEPHYR_SDK_INSTALL_DIR/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc --version
FVP_BaseR_AEMv8R --version    # from Step 2
```

## 5. Build the boards

Once Phase 117.10 / 117.11 land:

```bash
just zephyr build              # default: native_sim
just zephyr build-fixtures     # all examples + boards Phase 117 supports
```

For ad-hoc builds:

```bash
WORKSPACE="${NROS_ZEPHYR_WORKSPACE:-zephyr-workspace}"
cd "$WORKSPACE"
west build -b fvp_baser_aemv8r_smp \
    nano-ros/examples/zephyr-aemv8r-cyclonedds
west build -b s32z270dc2_rtu0_r52@D \
    nano-ros/examples/zephyr-s32z-cyclonedds
```

## Troubleshooting

- **`zephyr-sdk` directory exists but the new toolchain isn't
  there.** Older nano-ros workspaces installed the SDK before
  `--phase-117` was a flag. The SDK installer is idempotent — re-
  running `just zephyr setup --phase-117 --skip-sdk` won't fix
  it because `--skip-sdk` short-circuits the SDK step. Either
  delete `scripts/zephyr/sdk/zephyr-sdk-<version>/` and re-run
  `just zephyr setup --phase-117`, or invoke the SDK's setup
  script directly:

  ```bash
  cd scripts/zephyr/sdk/zephyr-sdk-0.16.8
  ./setup.sh -t aarch64-zephyr-elf -h -c
  ```

- **`west build -b fvp_baser_aemv8r_smp` fails with "FVP_BaseR_
  AEMv8R: not found"**. The FVP isn't on `$PATH`. See Step 2.

- **Build succeeds but `west flash` hangs.** FVP licence not
  accepted, or your terminal isn't a TTY (FVP needs an interactive
  console for the AArch64 emulation startup logs). Run from a real
  shell, not a CI step that pipes stdout.
