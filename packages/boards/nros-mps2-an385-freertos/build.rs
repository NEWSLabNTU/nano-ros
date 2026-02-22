//! Build script for nros-mps2-an385-freertos
//!
//! Compiles the FreeRTOS kernel, lwIP stack, lwIP FreeRTOS sys_arch,
//! LAN9118 lwIP netif driver, and a small C startup/glue layer into
//! a single static library linked into the final firmware.
//!
//! Required environment variables:
//!   FREERTOS_DIR       — FreeRTOS kernel source root
//!   FREERTOS_PORT      — portable layer, e.g. "GCC/ARM_CM3"
//!   LWIP_DIR           — lwIP source root
//!   FREERTOS_CONFIG_DIR — (optional) directory with FreeRTOSConfig.h + lwipopts.h
//!                         Defaults to this crate's config/ directory.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // --- Linker script ---
    File::create(out_dir.join("mps2_an385.ld"))
        .unwrap()
        .write_all(include_bytes!("config/mps2_an385.ld"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rustc-link-arg=-Tmps2_an385.ld");
    println!("cargo:rustc-link-arg=--nmagic");

    // --- Environment variables ---
    let freertos_dir = env_path("FREERTOS_DIR");
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let lwip_dir = env_path("LWIP_DIR");
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir.clone());

    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    let lan9118_dir = manifest_dir.join("../../drivers/lan9118-lwip");

    // --- Build FreeRTOS kernel ---
    let mut freertos = cc::Build::new();
    configure_arm_cm3(&mut freertos);
    add_freertos_includes(&mut freertos, &freertos_dir, &port_dir, &freertos_config_dir);

    // Kernel core
    for src in &[
        "tasks.c",
        "queue.c",
        "list.c",
        "timers.c",
        "event_groups.c",
        "stream_buffer.c",
    ] {
        freertos.file(freertos_dir.join(src));
    }
    // Portable layer
    freertos.file(port_dir.join("port.c"));
    // Memory manager
    freertos.file(freertos_dir.join("portable/MemMang/heap_4.c"));

    freertos.compile("freertos");

    // --- Build lwIP ---
    let mut lwip = cc::Build::new();
    configure_arm_cm3(&mut lwip);
    add_freertos_includes(&mut lwip, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut lwip, &lwip_dir);

    // Core
    for src in &[
        "src/core/init.c",
        "src/core/def.c",
        "src/core/dns.c",
        "src/core/inet_chksum.c",
        "src/core/ip.c",
        "src/core/mem.c",
        "src/core/memp.c",
        "src/core/netif.c",
        "src/core/pbuf.c",
        "src/core/raw.c",
        "src/core/stats.c",
        "src/core/sys.c",
        "src/core/tcp.c",
        "src/core/tcp_in.c",
        "src/core/tcp_out.c",
        "src/core/timeouts.c",
        "src/core/udp.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // IPv4
    for src in &[
        "src/core/ipv4/etharp.c",
        "src/core/ipv4/icmp.c",
        "src/core/ipv4/ip4.c",
        "src/core/ipv4/ip4_addr.c",
        "src/core/ipv4/ip4_frag.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // API (required for sockets)
    for src in &[
        "src/api/api_lib.c",
        "src/api/api_msg.c",
        "src/api/err.c",
        "src/api/if_api.c",
        "src/api/netbuf.c",
        "src/api/netdb.c",
        "src/api/netifapi.c",
        "src/api/sockets.c",
        "src/api/tcpip.c",
    ] {
        lwip.file(lwip_dir.join(src));
    }
    // Netif
    lwip.file(lwip_dir.join("src/netif/ethernet.c"));
    // FreeRTOS sys_arch
    lwip.file(lwip_dir.join("contrib/ports/freertos/sys_arch.c"));

    lwip.compile("lwip");

    // --- Build LAN9118 lwIP netif driver ---
    let mut lan9118 = cc::Build::new();
    configure_arm_cm3(&mut lan9118);
    add_freertos_includes(&mut lan9118, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut lan9118, &lwip_dir);
    lan9118.include(lan9118_dir.join("include"));
    lan9118.file(lan9118_dir.join("src/lan9118_lwip.c"));
    lan9118.compile("lan9118_lwip");

    // --- Build startup/glue C code ---
    let mut glue = cc::Build::new();
    configure_arm_cm3(&mut glue);
    add_freertos_includes(&mut glue, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut glue, &lwip_dir);
    glue.include(lan9118_dir.join("include"));

    // Generate startup C file
    let startup_c = out_dir.join("startup.c");
    File::create(&startup_c)
        .unwrap()
        .write_all(STARTUP_C.as_bytes())
        .unwrap();
    glue.file(&startup_c);
    glue.compile("startup");

    // --- Link order ---
    println!("cargo:rustc-link-lib=static=startup");
    println!("cargo:rustc-link-lib=static=lan9118_lwip");
    println!("cargo:rustc-link-lib=static=lwip");
    println!("cargo:rustc-link-lib=static=freertos");

    // --- Rerun triggers ---
    println!("cargo:rerun-if-changed=config/FreeRTOSConfig.h");
    println!("cargo:rerun-if-changed=config/lwipopts.h");
    println!("cargo:rerun-if-changed=config/mps2_an385.ld");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CONFIG_DIR");
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(
        env::var(name).unwrap_or_else(|_| panic!("{name} not set — run `just setup-freertos`")),
    )
}

fn configure_arm_cm3(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-mcpu=cortex-m3")
        .flag("-mthumb")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);
}

fn add_freertos_includes(
    build: &mut cc::Build,
    freertos_dir: &Path,
    port_dir: &Path,
    config_dir: &Path,
) {
    build
        .include(config_dir)
        .include(freertos_dir.join("include"))
        .include(port_dir);
}

fn add_lwip_includes(build: &mut cc::Build, lwip_dir: &Path) {
    build
        .include(lwip_dir.join("src/include"))
        .include(lwip_dir.join("contrib/ports/freertos/include"));
}

/// C startup code: vector table, Reset_Handler, default handlers, FreeRTOS
/// hooks, semihosting assert, and the init_hardware() / init_lwip() functions
/// called from Rust.
const STARTUP_C: &str = r#"
#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/init.h"
#include "lwip/tcpip.h"
#include "lwip/netif.h"
#include "lwip/ip4_addr.h"

#include "lan9118_lwip.h"

/* ---- Linker symbols ---- */
extern uint32_t _etext;
extern uint32_t _sdata;
extern uint32_t _edata;
extern uint32_t _sbss;
extern uint32_t _ebss;
extern uint32_t _estack;

/* ---- Forward declarations ---- */
void Reset_Handler(void);
void Default_Handler(void);
void SysTick_Handler(void);
void PendSV_Handler(void);
void SVC_Handler(void);

/* Rust entry point (provided by the example's #[no_mangle] main or entry) */
extern void _start(void);

/* ---- Semihosting ---- */
void semihosting_write0(const char *s) {
    __asm__ volatile("mov r0, #0x04\n"
                     "mov r1, %0\n"
                     "bkpt #0xAB\n"
                     :
                     : "r"(s)
                     : "r0", "r1", "memory");
}

/* ---- FreeRTOS assert ---- */
void freertos_assert_failed(const char *file, int line) {
    (void)file;
    (void)line;
    semihosting_write0("FreeRTOS ASSERT FAILED\n");
    __asm__ volatile("bkpt #0");
    for (;;) {}
}

/* ---- Interrupt vector table ---- */
typedef void (*vector_fn)(void);

__attribute__((section(".isr_vector"), used))
const vector_fn isr_vector[] = {
    (vector_fn)(uintptr_t)&_estack,  /* Initial MSP */
    Reset_Handler,
    Default_Handler,  /* NMI */
    Default_Handler,  /* HardFault */
    Default_Handler,  /* MemManage */
    Default_Handler,  /* BusFault */
    Default_Handler,  /* UsageFault */
    0, 0, 0, 0,      /* Reserved */
    SVC_Handler,
    Default_Handler,  /* DebugMon */
    0,                /* Reserved */
    PendSV_Handler,
    SysTick_Handler,
};

/* ---- Reset handler ---- */
void Reset_Handler(void) {
    /* Copy .data from flash to RAM */
    uint32_t *src = &_etext;
    uint32_t *dst = &_sdata;
    while (dst < &_edata) {
        *dst++ = *src++;
    }
    /* Zero .bss */
    dst = &_sbss;
    while (dst < &_ebss) {
        *dst++ = 0;
    }
    /* Jump to Rust entry */
    _start();
    for (;;) {}
}

/* ---- Default handler (infinite loop) ---- */
void Default_Handler(void) {
    for (;;) {}
}

/* ---- FreeRTOS port handlers (from port.c) ---- */
extern void xPortSysTickHandler(void);
extern void xPortPendSVHandler(void);
extern void vPortSVCHandler(void);

void SysTick_Handler(void) {
    if (xTaskGetSchedulerState() != taskSCHEDULER_NOT_STARTED) {
        xPortSysTickHandler();
    }
}
void PendSV_Handler(void)  { xPortPendSVHandler(); }
void SVC_Handler(void)     { vPortSVCHandler(); }

/* ---- Network globals (accessed from Rust via FFI) ---- */
static struct netif lan9118_netif;
static struct lan9118_config lan9118_cfg;
static volatile int lwip_init_done = 0;

static void tcpip_init_done_cb(void *arg) {
    (void)arg;
    lwip_init_done = 1;
}

/* ---- Public C API called from Rust ---- */

/*
 * Initialise the LAN9118 Ethernet + lwIP stack.
 *
 * Parameters are passed from Rust config:
 *   mac[6], ip[4], netmask[4], gateway[4]
 *
 * Returns 0 on success, -1 on failure.
 */
int nros_freertos_init_network(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    ip4_addr_t ipaddr, mask, gateway;

    IP4_ADDR(&ipaddr,  ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask,    netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    lan9118_cfg.base_addr = LAN9118_BASE_DEFAULT;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

    /* Start lwIP's tcpip_thread */
    tcpip_init(tcpip_init_done_cb, NULL);
    while (!lwip_init_done) {
        /* Busy-wait — scheduler not started yet, this runs in the init task */
        vTaskDelay(1);
    }

    /* Register netif */
    if (netif_add(&lan9118_netif, &ipaddr, &mask, &gateway,
                  &lan9118_cfg, lan9118_lwip_init, tcpip_input) == NULL) {
        return -1;
    }

    netif_set_default(&lan9118_netif);
    netif_set_up(&lan9118_netif);
    netif_set_link_up(&lan9118_netif);

    return 0;
}

/*
 * Poll the LAN9118 RX FIFO for received frames.
 * Call from a FreeRTOS task periodically.
 */
void nros_freertos_poll_network(void) {
    lan9118_lwip_poll(&lan9118_netif);
}

/*
 * Start the FreeRTOS scheduler.  Does not return.
 */
void nros_freertos_start_scheduler(void) {
    vTaskStartScheduler();
    /* Should never reach here */
    for (;;) {}
}

/*
 * Create a FreeRTOS task.
 * Returns 0 on success, -1 on failure.
 */
int nros_freertos_create_task(
    void (*entry)(void *),
    const char *name,
    uint32_t stack_words,
    void *arg,
    uint32_t priority)
{
    BaseType_t ret = xTaskCreate(entry, name, (uint16_t)stack_words, arg,
                                 (UBaseType_t)priority, NULL);
    return (ret == pdPASS) ? 0 : -1;
}
"#;
