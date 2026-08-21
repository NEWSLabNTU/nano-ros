# Adding a FreeRTOS Board

nano-ros ships one FreeRTOS witness — MPS2-AN385 under QEMU
(`nros-board-mps2-an385-freertos`, tier 1). This page is the answer to the
only question that matters if your hardware is not that board: **what do I
actually have to write?**

The answer is measured, not estimated. Everything below is the complete file
set for a hypothetical second board (an NXP S32K344, Cortex-M7), and the line
counts are `wc -l` of exactly these listings.

| File | Lines | What it is |
|---|---:|---|
| `config/lwipopts.h` | 1 | `#include` the shared defaults |
| `config/arch/cc.h` | 1 | `#include` the shared defaults |
| `config/FreeRTOSConfig.h` | 3 | CPU clock + NVIC priority bits, then the shared file |
| `config/s32k344.ld` | 7 | memory map + `_estack`, then the shared section layout |
| `c/board_s32k344.c` | 64 | vector table, `Reset_Handler`, netif registration |
| **per-board delta** | **76** | |
| `src/lib.rs` | 57 | the board ZST + four trait impls |
| `build.rs` | 45 | driver + board C + `NROS_APP_CONFIG` + linker scripts |
| `Cargo.toml` | 27 | dependencies + the `rmw-zenoh` feature |
| **total** | **205** | |

Read that table honestly: the part that is genuinely **about your board** is
76 lines. The other 129 is Cargo and trait scaffolding that is identical for
every Cortex-M FreeRTOS board and is the next thing worth templating — see
"What is still boilerplate" at the bottom.

For comparison, `nros-board-mps2-an385-freertos` before phase-337 W5 was
~1600 lines plus a 727-line `startup.c`.

## What you do NOT write

The FreeRTOS family crate (`nros-board-freertos`) owns all of this, and your
board gets it by depending on the crate:

- the FreeRTOS kernel, lwIP and `nros-platform-freertos` compile
- `FreeRTOSConfig.h`, `lwipopts.h`, `arch/cc.h` — shared defaults, in
  `packages/boards/nros-board-freertos/config/`
- the Cortex-M section layout, `nros-freertos-cortex-m.ld`
- the FreeRTOS hooks (assert / idle / malloc-failed / stack-overflow /
  SysTick) and the semihosting helpers — `c/freertos_hooks.c`
- lwIP bring-up, the task wrappers and the FFI surface — `c/network_glue.c`
- the C/C++ application boot path — `c/freertos_c_entry.c`
- the multi-tier entry — `c/freertos_run_tiers.c`
- the entry driver (`run_entry` / `run_bare` / `run_tiers_entry`) and `Config`

`configure_cflags` resolves your target's compiler flags from the `[arch.*]`
profiles in `config/freertos-lwip/nros-platform.toml`. Cortex-M3 and Cortex-M7
are declared there; a new arch is a profile block, not a build-script branch.

## The files

### `config/lwipopts.h`

```c
#include "../../nros-board-freertos/config/lwipopts.h"
```

### `config/arch/cc.h`

```c
#include "../../../nros-board-freertos/config/arch/cc.h"
```

The include is relative because `FREERTOS_CONFIG_DIR` is a single directory,
not a search path. If your board lives outside this repository and cannot
spell that path, take ladder rung 3 (see [The customization
ladder](../concepts/board-integration.md)) and own the file — that is what
rung 3 is for, and nothing else in nano-ros changes.

> **Out-of-tree variant.** The worked example's `build.rs` reaches
> sibling crates with `../../` walks
> (`manifest.parent().join("nros-board-freertos/config")`, drivers,
> `nros-c` includes) — those only resolve inside `packages/boards/`. An
> out-of-tree board crate resolves the nano-ros root ONCE from an env
> var instead (`NROS_REPO_DIR`, the same variable `nros sync` uses) and
> derives every include from it:
> `let root = PathBuf::from(env::var("NROS_REPO_DIR")?);`
> `hal.include(root.join("packages/api/nros-c/include"));` — same
> files, absolute anchor.

### `config/FreeRTOSConfig.h`

Two numbers. Everything else in the 111-line shared file is generic.

```c
#define NROS_BOARD_CPU_CLOCK_HZ 160000000 /* S32K344 CORE_CLK */
#define NROS_BOARD_PRIO_BITS    4         /* Cortex-M7 __NVIC_PRIO_BITS */
#include "../../nros-board-freertos/config/FreeRTOSConfig.h"
```

### `config/s32k344.ld`

Three numbers. `INCLUDE` resolves against the linker's `-L` search path, which
`build.rs` populates with `OUT_DIR`.

```
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x00400000, LENGTH = 4M
    RAM   (rwx) : ORIGIN = 0x20400000, LENGTH = 320K
}
_estack = ORIGIN(RAM) + LENGTH(RAM);
INCLUDE nros-freertos-cortex-m.ld
```

### `c/board_s32k344.c`

The vector table, the reset handler, and the strong overrides for
`network_glue.c`'s weak `nros_board_register_netif` / `nros_board_poll_netif`
hooks. `main` is the firmware entry — the Rust `main` on a Rust image, or
`freertos_c_entry.c::main` on a C/C++ image. One symbol, one board file.

```c
#include <stdint.h>
#include <string.h>
#include "FreeRTOS.h"
#include "lwip/netifapi.h"
#include "gmac_lwip.h"   /* your MAC driver's lwIP netif init + poll */

extern uint32_t _etext, _sdata, _edata, _sbss, _ebss, _estack;
extern void xPortPendSVHandler(void);
extern void vPortSVCHandler(void);
void SysTick_Handler(void);      /* nros-board-freertos/c/freertos_hooks.c */
extern int main(void);           /* Rust entry, or freertos_c_entry.c::main */
void Reset_Handler(void);
void Default_Handler(void);

static struct netif s_netif;
static struct gmac_config s_cfg;

/* Strong overrides for network_glue.c's weak hooks. */
int nros_board_register_netif(const uint8_t mac[6], const uint8_t ip[4],
                              const uint8_t netmask[4], const uint8_t gw[4]) {
    ip4_addr_t a, m, g;
    IP4_ADDR(&a, ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&m, netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&g, gw[0], gw[1], gw[2], gw[3]);
    s_cfg.base_addr = GMAC_BASE_DEFAULT;
    memcpy(s_cfg.mac_addr, mac, 6);
    if (netifapi_netif_add(&s_netif, &a, &m, &g, &s_cfg, gmac_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }
    netifapi_netif_set_default(&s_netif);
    netifapi_netif_set_up(&s_netif);
    netifapi_netif_set_link_up(&s_netif);
    return 0;
}

void nros_board_poll_netif(void) { gmac_lwip_poll(&s_netif); }

typedef void (*vector_fn)(void);
__attribute__((section(".isr_vector"), used))
const vector_fn isr_vector[] = {
    (vector_fn)(uintptr_t)&_estack,
    Reset_Handler,
    Default_Handler,  /* NMI */
    Default_Handler,  /* HardFault */
    Default_Handler,  /* MemManage */
    Default_Handler,  /* BusFault */
    Default_Handler,  /* UsageFault */
    0, 0, 0, 0,
    vPortSVCHandler,
    Default_Handler,  /* DebugMon */
    0,
    xPortPendSVHandler,
    SysTick_Handler,
};

void Reset_Handler(void) {
    uint32_t *src = &_etext, *dst = &_sdata;
    while (dst < &_edata) *dst++ = *src++;
    for (dst = &_sbss; dst < &_ebss;) *dst++ = 0;
    (void)main();
    for (;;) {}
}

void Default_Handler(void) { for (;;) {} }
```

If the board has **no** Ethernet, omit both hook overrides: the weak defaults
in `network_glue.c` return "no netif", and the board reaches the router over a
serial locator instead.

### `build.rs`

```rust
use std::{env, fs, path::PathBuf};

use nros_board_common::freertos_build::{
    add_freertos_includes, add_lwip_includes, app_stack_bytes_from_build_env, configure_cflags,
    emit_app_config_tu,
};
use nros_board_common::{BaseConfig, FreertosScheduling};

fn main() {
    if nros_board_common::host_probe::skip_cross_build("nros-board-s32k344", &["thumb"]) {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let shared = manifest.parent().unwrap().join("nros-board-freertos/config");

    fs::copy(manifest.join("config/s32k344.ld"), out.join("s32k344.ld")).unwrap();
    fs::copy(
        shared.join("nros-freertos-cortex-m.ld"),
        out.join("nros-freertos-cortex-m.ld"),
    )
    .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    let freertos = nros_build_paths::freertos_dir();
    let port = freertos.join("portable/GCC/ARM_CM7/r0p1");
    let lwip = nros_build_paths::lwip_dir();
    let cfg = manifest.join("config");

    let mut b = cc::Build::new();
    configure_cflags(&mut b);
    add_freertos_includes(&mut b, &freertos, &port, &cfg);
    add_lwip_includes(&mut b, &lwip);
    b.include(manifest.join("../../drivers/net/gmac-lwip/include"));
    b.include(manifest.join("../../api/nros-c/include"));
    b.file(manifest.join("c/board_s32k344.c"));
    b.file(manifest.join("../../drivers/net/gmac-lwip/src/gmac_lwip.c"));
    let sched = FreertosScheduling {
        app_stack_bytes: app_stack_bytes_from_build_env(),
        ..FreertosScheduling::default()
    };
    b.file(emit_app_config_tu(&out, &BaseConfig::default(), &sched));
    b.compile("startup");
    println!("cargo:rustc-link-lib=static=startup");
}
```

### `src/lib.rs`

```rust
#![no_std]

extern crate nros_platform as _;
#[cfg(target_os = "none")]
use panic_semihosting as _;
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

pub use nros_board_freertos::{BaseConfig, Config};

/// Per-board marker for trait dispatch into the `nros_board_freertos::run_*`
/// family driver.
pub struct S32k344;

impl nros_platform::BoardInit for S32k344 {
    fn init_hardware() {}
}

impl nros_platform::BoardPrint for S32k344 {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        if let Ok(mut out) = cortex_m_semihosting::hio::hstdout() {
            let _ = writeln!(out, "{}", args);
        }
    }
}

impl nros_platform::BoardExit for S32k344 {
    fn exit_success() -> ! {
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
        loop {}
    }
    fn exit_failure() -> ! {
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
        loop {}
    }
}

impl nros_platform::BoardEntry for S32k344 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        nros_board_freertos::run_entry::<S32k344, F, E>(Config::default(), None, setup)
    }

    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let mut config = Config::default();
        config.base.apply_overlay(deploy);
        nros_board_freertos::run_entry::<S32k344, F, E>(config, deploy.boot_config, setup)
    }
}
```

### `Cargo.toml`

```toml
[package]
name = "nros-board-s32k344"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"

[dependencies]
nros-board-freertos = { path = "../nros-board-freertos" }
nros-platform = { path = "../../platform/nros-platform", default-features = false, features = [
    "platform-freertos",
    "global-allocator",
] }
zpico-sys = { path = "../../rmw/zenoh/zpico-sys", default-features = false, features = [
    "freertos",
    "platform-aliases",
], optional = true }
cortex-m-semihosting = "0.5"
panic-semihosting = "0.6"

[build-dependencies]
nros-board-common = { path = "../nros-board-common", features = ["build-helpers"] }
nros-build-paths = { path = "../../tooling/nros-build-paths" }
cc = "1"

[features]
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:zpico-sys", "nros-board-freertos/rmw-zenoh"]
```

## The one thing that is not a line count

The MAC driver. `board_s32k344.c` above assumes a `gmac_lwip.c` exists — an
lwIP netif driver for your chip's Ethernet MAC. nano-ros's reference is
`packages/drivers/net/lan9118-lwip/src/lan9118_lwip.c` at ~507 lines. If your
vendor SDK already ships an lwIP netif (most do), you wire it up in the four
lines of `nros_board_register_netif` and write none of it. If it does not,
that driver is the real cost of the port, and it is a cost nano-ros cannot
remove — it is between your silicon and lwIP.

If your vendor SDK owns the whole build (S32DS, MCUXpresso, ESP-IDF,
PlatformIO), you likely want an **integration shell** instead of a board
crate: see `integrations/s32ds/` and RFC-0064. The shell composes
`freertos_platform` from artefacts the vendor build already produced and
nano-ros runs board-less — no board crate at all.

## What is still boilerplate

Measured above: 129 of the 205 lines are not about the board.

- `src/lib.rs` (57): `BoardPrint` / `BoardExit` are pure Cortex-M semihosting
  and `BoardEntry` is a two-line delegation to the family driver. Every
  Cortex-M FreeRTOS board writes the same file with a different type name. A
  declarative macro in `nros-board-freertos` would take this to one line.
- `Cargo.toml` (27) and `build.rs` (45): the dependency set and the build
  sequence are fixed; only the driver paths, the FreeRTOS port directory and
  the linker-script name vary.

Phase-337 W5 templated the C and config half (299 lines of headers and 135
lines of linker script became 12 and 7). The Rust half is the same exercise,
not yet done. Until it is, quote 205 lines for a second FreeRTOS board, not
80.
