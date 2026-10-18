# `integrations/freertos` — nano-ros as a guest in a FreeRTOS CMake project

Your project already builds FreeRTOS. This shell takes the targets your build
defines and adds nano-ros on top. It does not supply a board, a kernel or a
network stack, and the integration adds no files to the nano-ros tree.

Design of record: [RFC-0072](../../docs/design/0072-rtos-integration-nano-ros-is-a-guest.md)
("nano-ros is a guest"). Work item: [phase-349 W4](../../docs/roadmap/phase-349-rtos-integration-shells.md).

## What you provide

| Target | Kind | Where it comes from |
|---|---|---|
| `freertos_config` | INTERFACE | **you** — its include directories hold `FreeRTOSConfig.h`. Upstream's CMake requires this target too. |
| `freertos_kernel` | library | **upstream** — `add_subdirectory(<FreeRTOS-Kernel>)` with `FREERTOS_PORT` (upstream's enum, e.g. `GCC_ARM_CM3`) and `FREERTOS_HEAP` set. Needs V10.5.0 or later. |
| a TCP/IP stack | library | **you** — usually lwIP plus your `lwipopts.h` and `arch/cc.h`. Pass it as `NETSTACK`. |
| netif drivers | library | **you**, optional — pass them as `NETIFS`. |
| startup, vector table, linker script | — | **you**, as in any FreeRTOS project. Put the `-T` script in `LINK_OPTIONS`. |

## Usage

```cmake
set(FREERTOS_PORT GCC_ARM_CM3)   # upstream's enum
set(FREERTOS_HEAP 4)
add_library(freertos_config INTERFACE)
target_include_directories(freertos_config SYSTEM INTERFACE ${CMAKE_SOURCE_DIR}/config)
add_subdirectory(third_party/FreeRTOS-Kernel)

add_library(lwip STATIC ${MY_LWIP_SOURCES})
target_include_directories(lwip PUBLIC
    ${LWIP_DIR}/src/include ${LWIP_DIR}/contrib/ports/freertos/include ${CMAKE_SOURCE_DIR}/config)
target_link_libraries(lwip PUBLIC freertos_kernel)

include(<nano-ros>/integrations/freertos/NanoRosFreertos.cmake)
nros_freertos_integrate(
    NETSTACK     lwip
    NETIFS       my_eth_netif
    LINK_OPTIONS "-T${CMAKE_SOURCE_DIR}/board.ld" -nostartfiles --specs=nosys.specs)

add_subdirectory(<nano-ros> nano_ros)
```

`nros_freertos_integrate` composes `freertos_platform`, the umbrella target
that nano-ros images link. Because that target exists before
`add_subdirectory(<nano-ros>)` runs, the FreeRTOS platform module runs in
board-less mode and needs no `NROS_BOARD`.

| Argument | Default | Meaning |
|---|---|---|
| `KERNEL <target>` | `freertos_kernel` | the kernel library |
| `CONFIG <target>` | `freertos_config` | the target carrying `FreeRTOSConfig.h` |
| `NETSTACK <target>\|NONE` | `lwip` | the TCP/IP stack. `NONE` means the host's kernel owns the network stack, as in the FreeRTOS POSIX simulator. |
| `NETIFS <targets…>` | — | network-interface drivers |
| `LINK_LIBS <libs…>` | — | more libraries on the umbrella target |
| `LINK_OPTIONS <opts…>` | — | link flags on the umbrella target |

## What fails at configure

These checks follow RFC-0072 §7. Each one names the thing that is missing.

* **A required target is missing**, e.g. `no freertos_kernel target`.
* **A capability nothing provides.** The FreeRTOS platform descriptor
  (`packages/platform/nros-platform-freertos/nros-platform.toml`) declares
  `ip_stack = true`. Without a `NETSTACK` target, or `NETSTACK NONE`, the
  configure fails. The message names the capability, the descriptor that
  requires it, and the provider it was given.
* **An include directory does not exist.** Every directory that a target
  exports is checked, including directories reached through linked targets,
  such as upstream's `freertos_kernel_include`. The usual cause is an
  uninitialised submodule. Without this check it would show up as a missing
  header in the first file that needs one.
* **No `FreeRTOSConfig.h`** in the `CONFIG` target's include directories.

`just check freertos-shell-diagnostics` checks that each of these fires.

## Who uses it

nano-ros's own FreeRTOS boards use the shell too: `mps2-an385-freertos`,
`mps3-an536-freertos`, `s32z270-freertos` and `freertos-posix`. Each board
builds the kernel with upstream's CMake (phase-349 W3) and then calls
`nros_freertos_integrate`. So the shell and the boards share one consumption
path, and every in-tree FreeRTOS image runs these checks.

The S32DS shell (`integrations/s32ds/`) is a different shape. S32DS compiles
the kernel inside its own CDT project, so there is no upstream CMake target to
consume, and that shell declares header-only targets instead.
