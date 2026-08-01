# `integrations/s32ds` — NXP S32 Design Studio integration shell

**Status: skeleton, partially validated.** The project probe is tested against
the MR-CANHUBK3 reference project; the full build and link have **not** been
exercised on hardware. Design of record: [RFC-0064](../../docs/design/0064-board-support-organization.md).

## What this is

S32DS projects own their build: the CDT project already contains the FreeRTOS
kernel, RTD drivers, startup code, vector table, linker scripts and pin/clock
configuration. So nano-ros integrates the way it does with ESP-IDF — as a
library, not as a board.

**This board contributes zero files to the nano-ros tree.** No board crate, no
`nros-board.toml`, no `cmake/board/` module, no `board-support.toml` row, no
tier entry, no SDK-index entry.

| Who | Builds |
|---|---|
| S32DS | FreeRTOS kernel, RTD drivers, startup, vector table, clock/pin config, final `.elf` |
| this shell | `nros-platform-freertos` shim, the RMW, generated node + message code, app entry |
| **you** | a TCP/IP stack — see [Networking](#networking) |

## Quick start

```sh
cmake -S <nano-ros>/integrations/s32ds -B build-s32ds \
      -DNXP_S32DS_PROJECT=~/MR_CANHUBK3_IEEE1722 \
      -DNROS_WORKSPACE_DIR=~/my_ws \
      -DNROS_BRINGUP=safety_island_bringup \
      -DLWIP_DIR=<lwip source>
cmake --build build-s32ds

cp build-s32ds/nros-libs.mk                       ~/MR_CANHUBK3_IEEE1722/
cp <nano-ros>/integrations/s32ds/makefile.defs    ~/MR_CANHUBK3_IEEE1722/
make -C ~/MR_CANHUBK3_IEEE1722/Debug_FLASH all
```

Host tools come from the ordinary index verbs — there is deliberately no
`nros setup --board <name>`, because a board name in the SDK index is only an
alias for a package list (RFC-0064):

```sh
nros setup --tool arm-none-eabi-gcc --rmw zenoh
nros setup --source lwip
```

## How it hooks into the CDT build

CDT's generated `Debug_FLASH/makefile` is marked "do not edit", but it already
provides three extension points, none of which exist in a stock project:

```make
-include ../makefile.init
-include objects.mk            # USER_OBJS :=   LIBS := -lc -lm -lgcc
-include ../makefile.defs      # <- we land here, before any rule
...
<proj>.elf: $(OBJS) <ld> $(USER_OBJS)
	arm-none-eabi-gcc -o "<proj>.elf" "@<proj>.args"  $(USER_OBJS)
-include ../makefile.targets
```

`makefile.defs` appends nano-ros's static libraries to `USER_OBJS`, which the
link rule already passes to gcc. Regenerating the project from S32DS does not
overwrite it.

## ABI flags are probed, not retyped

Objects built here must share the project's exact ABI or the link fails — or
silently produces a broken image. For MR-CANHUBK3:

```
-mcpu=cortex-m7 -mthumb -mlittle-endian -mfloat-abi=hard -mfpu=fpv5-sp-d16
```

Note `fpv5-**sp**-d16` — single precision, not the `fpv5-d16` one would guess.

`cmake/S32dsProject.cmake` reads these out of CDT's own per-translation-unit
`.args` response files and generates the toolchain file from them, so it cannot
drift from the project. It also recovers the FreeRTOS source dir and port
(`GCC/ARM_CM7/r0p1`), the flash linker script, and the portable include set.

Windows-generated projects carry absolute `C:/…` include paths and a
`--sysroot=C:/…`; these are dropped. In the reference project every one is a
duplicate of a project-relative path that already resolves. Anything genuinely
reachable only through the S32DS install must be passed via
`-DNROS_S32DS_EXTRA_INCLUDES=…`.

**Prerequisite:** the project must have been built once (in S32DS, or `make -C
Debug_FLASH`) so CDT has emitted the `.args` files.

## Networking

The MR-CANHUBK3 FreeRTOS deliverable ships **no TCP/IP stack** — `src/enet.c`
drives the GMAC directly for raw IEEE-1722/AVTP frames. The zenoh and Cyclone
backends both want sockets. Three options, increasing in nano-ros involvement:

1. **lwIP + a netif driver** over the vendor MAC. `nros setup --source lwip`,
   then write `<chip>_lwip.c`. Reference:
   `packages/drivers/net/lan9118-lwip/src/lan9118_lwip.c` (~507 LOC).
2. **Implement the socket subset** of the `nros_platform_*` ABI directly over
   the vendor MAC (`packages/platform/nros-platform-api`).
3. **`LinkFeatures{custom}`** plus a transport vtable — smallest surface, but
   you give up TCP/UDP locators.

None of these is a nano-ros change; the shell fails loudly with this list if
`LWIP_DIR` is unset.

## Known gaps

- **Not validated end to end.** Probe: yes. Build and link: not yet.
- **Windows-absolute linker-script prerequisite.** The reference project's
  `.elf` rule depends on `C:/Users/…/linker_flash_s32k344.ld`, so `make` fails
  with "no rule to make target" on Linux unless the project is regenerated
  locally. A future `makefile.init` may patch this; for now, regenerate.
- **Cargo staticlib staging.** `nros-libs.mk` includes a
  `nros-cargo-libs.mk` that nothing generates yet — the `nros-c` / `nros-cpp` /
  per-package FFI archives must currently be appended by hand. NuttX solves the
  same problem with `scripts/nuttx/stage-external-apps.sh`; the equivalent is
  not written.
- **Cyclone DDS is unexercised on FreeRTOS C/C++** (every FreeRTOS fixture in
  `examples/fixtures.toml` is `rmw = "zenoh"`), so the shell defaults to zenoh.
