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
    // Make the linker script discoverable by the final binary.
    // The binary's .cargo/config.toml specifies `-Tmps2_an385.ld` via rustflags.
    println!("cargo:rustc-link-search={}", out_dir.display());

    // --- Environment variables ---
    let freertos_dir = env_path("FREERTOS_DIR");
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CM3".to_string());
    let lwip_dir = env_path("LWIP_DIR");
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir.clone());

    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    let lan9118_dir = manifest_dir.join("../../drivers/lan9118-lwip");

    // --- Trace opt-in (NROS_TRACE=1) ---
    let nros_trace = env::var("NROS_TRACE").unwrap_or_default() == "1";
    println!("cargo:rerun-if-env-changed=NROS_TRACE");

    // --- Build FreeRTOS kernel ---
    let mut freertos = cc::Build::new();
    configure_arm_cm3(&mut freertos);
    add_freertos_includes(&mut freertos, &freertos_dir, &port_dir, &freertos_config_dir);
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");
        freertos.include(tband_dir.join("inc"));
        freertos.include(&trace_config_dir);
        freertos.define("NROS_TRACE", "1");
    }

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
        // Phase 97.1.kconfig.freertos — IGMP for RTPS SPDP multicast.
        "src/core/ipv4/igmp.c",
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

    // --- Tonbandgeraet trace library (opt-in via NROS_TRACE=1) ---
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");

        let mut tband = cc::Build::new();
        configure_arm_cm3(&mut tband);
        add_freertos_includes(&mut tband, &freertos_dir, &port_dir, &freertos_config_dir);
        tband.include(tband_dir.join("inc"));
        tband.include(&trace_config_dir);
        tband.define("NROS_TRACE", "1");
        tband.file(tband_dir.join("src/tband.c"));
        tband.file(tband_dir.join("src/tband_freertos.c"));
        tband.file(tband_dir.join("src/tband_backend.c"));
        tband.compile("tband");
        println!("cargo:rustc-link-lib=static=tband");
        println!("cargo:rustc-cfg=nros_trace");
    }

    // --- Build startup/glue C code ---
    let mut glue = cc::Build::new();
    configure_arm_cm3(&mut glue);
    add_freertos_includes(&mut glue, &freertos_dir, &port_dir, &freertos_config_dir);
    add_lwip_includes(&mut glue, &lwip_dir);
    glue.include(lan9118_dir.join("include"));
    if nros_trace {
        let tband_dir = manifest_dir.join("../../../third-party/tracing/Tonbandgeraet/tband");
        let trace_config_dir = manifest_dir.join("trace");
        glue.include(tband_dir.join("inc"));
        glue.include(&trace_config_dir);
        glue.define("NROS_TRACE", "1");
    }

    // Generate startup C file
    let startup_c = out_dir.join("startup.c");
    File::create(&startup_c)
        .unwrap()
        .write_all(STARTUP_C.as_bytes())
        .unwrap();
    glue.file(&startup_c);

    // Trace dump (always compiled — stubs when NROS_TRACE not defined)
    glue.file(manifest_dir.join("trace/trace_dump.c"));

    glue.compile("startup");

    // --- Link order ---
    println!("cargo:rustc-link-lib=static=startup");
    println!("cargo:rustc-link-lib=static=lan9118_lwip");
    println!("cargo:rustc-link-lib=static=lwip");
    println!("cargo:rustc-link-lib=static=freertos");

    // --- Newlib (libc + nosys stubs for bare-metal) ---
    // zenoh-pico and lwIP use standard C library functions (atoi, strtoul, snprintf, etc.)
    // Use --print-file-name to discover multilib-correct paths (--print-sysroot is empty
    // on some distros).
    let libc_path = gcc_print_file("libc.a");
    let libc_dir = Path::new(&libc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libc_dir.display());
    // GCC's own library (libgcc.a) for ARM intrinsics
    let libgcc_path = gcc_print_file("libgcc.a");
    let libgcc_dir = Path::new(&libgcc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libgcc_dir.display());
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=nosys");
    println!("cargo:rustc-link-lib=static=gcc");

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

fn gcc_print_file(name: &str) -> String {
    let out = std::process::Command::new("arm-none-eabi-gcc")
        .args(["-mcpu=cortex-m3", "-mthumb", &format!("--print-file-name={name}")])
        .output()
        .expect("arm-none-eabi-gcc not found");
    let path = String::from_utf8(out.stdout).unwrap();
    let path = path.trim().to_string();
    // If GCC can't resolve the file it echoes the bare name back
    assert!(
        Path::new(&path).is_absolute(),
        "arm-none-eabi-gcc could not locate {name}"
    );
    path
}

/// C startup code: vector table, Reset_Handler, default handlers, FreeRTOS
/// hooks, semihosting assert, and the init_hardware() / init_lwip() functions
/// called from Rust.
const STARTUP_C: &str = r#"
#include <stdint.h>
#include <string.h>
#include <errno.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/init.h"
#include "lwip/tcpip.h"
#include "lwip/netif.h"
#include "lwip/netifapi.h"
#include "lwip/ip4_addr.h"
#include "lwip/sockets.h"

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

/* FreeRTOS port handlers — installed directly in the vector table.
 * FreeRTOS asserts that these exact function pointers appear in the
 * vector table, so wrapper functions are not allowed. */
extern void xPortPendSVHandler(void);
extern void vPortSVCHandler(void);

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
static void semihosting_write_int(int val) {
    char buf[12];
    char *p = buf + sizeof(buf) - 1;
    *p = '\0';
    if (val < 0) { semihosting_write0("-"); val = -val; }
    if (val == 0) { semihosting_write0("0"); return; }
    while (val > 0) { *--p = '0' + (val % 10); val /= 10; }
    semihosting_write0(p);
}

void freertos_assert_failed(const char *file, int line) {
    semihosting_write0("FreeRTOS ASSERT FAILED: ");
    semihosting_write0(file);
    semihosting_write0(":");
    semihosting_write_int(line);
    semihosting_write0("\n");
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
    vPortSVCHandler,
    Default_Handler,  /* DebugMon */
    0,                /* Reserved */
    xPortPendSVHandler,
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

/* ---- FreeRTOS SysTick handler ---- */
extern void xPortSysTickHandler(void);

void SysTick_Handler(void) {
    if (xTaskGetSchedulerState() != taskSCHEDULER_NOT_STARTED) {
        xPortSysTickHandler();
    }
}

/* ---- FreeRTOS malloc failed hook ---- */
void vApplicationMallocFailedHook(void) {
    /* Loop forever — debugger can inspect xPortGetFreeHeapSize() */
    for (;;) { __asm__ volatile("wfi"); }
}

/* ---- FreeRTOS idle hook: WFI for QEMU ---- */
/* On real hardware, WFI saves power. In QEMU, it yields CPU time back to
 * the main event loop so that the TAP network FD can be serviced. Without
 * this, the idle task busy-loops and QEMU never processes incoming network
 * frames from the host (ARP replies, TCP SYN-ACKs, etc.). */
void vApplicationIdleHook(void) {
    __asm__ volatile("wfi");
}

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

    /* Seed the C stdlib RNG with a value unique to this node.
     * Without this, rand() starts from seed 1 on every boot, causing
     * all QEMU instances to generate identical zenoh-pico session IDs
     * (16 bytes from LWIP_RAND → rand()). zenohd rejects duplicate
     * session IDs, so the second QEMU's z_open() always fails.
     *
     * Use IP octets directly — each node has a unique IP. Multiply to
     * spread bits and avoid XOR cancellation between MAC and IP. */
    {
        uint32_t seed = ((uint32_t)ip[0] << 24) | ((uint32_t)ip[1] << 16)
                      | ((uint32_t)ip[2] << 8)  | (uint32_t)ip[3];
        seed = seed * 2654435761u;  /* Knuth multiplicative hash */
        seed ^= ((uint32_t)mac[4] << 8) | (uint32_t)mac[5];
        if (seed == 0) seed = 1;
        srand(seed);
    }

    IP4_ADDR(&ipaddr,  ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask,    netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    lan9118_cfg.base_addr = LAN9118_BASE_DEFAULT;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

    /* Initialize per-thread lwIP semaphore for the app task.
     * Required when LWIP_NETCONN_SEM_PER_THREAD=1 — each task that calls
     * lwIP socket/netifapi functions must have its own semaphore.
     * Must be called before any lwIP API (including netifapi_netif_add). */
    lwip_socket_thread_init();

    /* Start lwIP's tcpip_thread (scheduler must be running) */
    tcpip_init(tcpip_init_done_cb, NULL);
    while (!lwip_init_done) {
        vTaskDelay(1);
    }

    /* Register netif via netifapi (thread-safe: executes in tcpip_thread).
     * Note: netif_add() does NOT set netif_default, even with LWIP_SINGLE_NETIF.
     * We must call netif_set_default() explicitly. */
    if (netifapi_netif_add(&lan9118_netif, &ipaddr, &mask, &gateway,
                           &lan9118_cfg, lan9118_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }

    netifapi_netif_set_default(&lan9118_netif);
    netifapi_netif_set_up(&lan9118_netif);
    netifapi_netif_set_link_up(&lan9118_netif);

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

/*
 * Low-level network diagnostic: send a raw ARP request via LAN9118
 * and check if packets flow through the TX/RX path.
 */
void nros_freertos_diag_network(void) {
    uint32_t base = LAN9118_BASE_DEFAULT;

    /* Read MAC_CR via indirect MAC CSR */
    {
        /* Wait for MAC not busy */
        while (*(volatile uint32_t *)(uintptr_t)(base + 0xA4) & (1u << 31)) {}
        /* Issue read of MAC_CR (index 1) */
        *(volatile uint32_t *)(uintptr_t)(base + 0xA4) = (1u << 31) | (1u << 30) | 1;
        while (*(volatile uint32_t *)(uintptr_t)(base + 0xA4) & (1u << 31)) {}
        uint32_t mac_cr = *(volatile uint32_t *)(uintptr_t)(base + 0xA8);
        semihosting_write0("  [diag] MAC_CR: 0x");
        {
            static const char hex[] = "0123456789abcdef";
            char buf[9];
            for (int i = 7; i >= 0; i--) {
                buf[7 - i] = hex[(mac_cr >> (i * 4)) & 0xF];
            }
            buf[8] = '\0';
            semihosting_write0(buf);
        }
        semihosting_write0(" (RXEN=");
        semihosting_write0((mac_cr & (1u << 2)) ? "YES" : "NO");
        semihosting_write0(", TXEN=");
        semihosting_write0((mac_cr & (1u << 3)) ? "YES" : "NO");
        semihosting_write0(")\n");
    }

    /* Check TX FIFO free space */
    uint32_t tx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x80);
    uint32_t tx_free = tx_inf & 0xFFFF;
    semihosting_write0("  [diag] TX FIFO free: ");
    semihosting_write_int((int)tx_free);
    semihosting_write0(" bytes\n");

    /* Check RX FIFO status */
    uint32_t rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    uint32_t rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX status entries: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Check INT_STS for TX/RX activity */
    uint32_t int_sts = *(volatile uint32_t *)(uintptr_t)(base + 0x58);
    semihosting_write0("  [diag] INT_STS: 0x");
    /* Print hex */
    {
        static const char hex[] = "0123456789abcdef";
        char buf[9];
        for (int i = 7; i >= 0; i--) {
            buf[7 - i] = hex[(int_sts >> (i * 4)) & 0xF];
        }
        buf[8] = '\0';
        semihosting_write0(buf);
    }
    semihosting_write0("\n");

    /* Send a raw ARP request to test TX path */
    /* ARP request: who has 192.0.3.1? tell 192.0.3.10 */
    /* Read the MAC that QEMU assigned to the NIC (from the netif) */
    uint8_t *our_mac = lan9118_netif.hwaddr;
    semihosting_write0("  [diag] netif MAC: ");
    {
        static const char hex[] = "0123456789abcdef";
        for (int i = 0; i < 6; i++) {
            char buf[4];
            buf[0] = hex[our_mac[i] >> 4];
            buf[1] = hex[our_mac[i] & 0xF];
            buf[2] = (i < 5) ? ':' : '\n';
            buf[3] = '\0';
            semihosting_write0(buf);
        }
    }

    uint8_t arp_frame[42] = {
        /* Ethernet header */
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,  /* Destination: broadcast */
        our_mac[0], our_mac[1], our_mac[2],
        our_mac[3], our_mac[4], our_mac[5],  /* Source: our MAC */
        0x08, 0x06,                            /* EtherType: ARP */
        /* ARP payload */
        0x00, 0x01,                            /* Hardware type: Ethernet */
        0x08, 0x00,                            /* Protocol type: IPv4 */
        0x06,                                  /* Hardware size: 6 */
        0x04,                                  /* Protocol size: 4 */
        0x00, 0x01,                            /* Opcode: request */
        our_mac[0], our_mac[1], our_mac[2],
        our_mac[3], our_mac[4], our_mac[5],  /* Sender MAC */
        0xC0, 0x00, 0x03, 0x0A,              /* Sender IP: 192.0.3.10 */
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  /* Target MAC: unknown */
        0xC0, 0x00, 0x03, 0x01               /* Target IP: 192.0.3.1 */
    };

    /* Write TX Command A */
    uint32_t cmd_a = (1u << 13) | (1u << 12) | 42;  /* FIRST_SEG | LAST_SEG | len */
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_a;
    /* Write TX Command B */
    uint32_t cmd_b = (42u << 16) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_b;
    /* Write frame data (DWORD-aligned) */
    uint32_t nwords = (42 + 3) / 4;
    for (uint32_t i = 0; i < nwords; i++) {
        uint32_t word = 0;
        for (int b = 0; b < 4 && (i * 4 + b) < 42; b++) {
            word |= (uint32_t)arp_frame[i * 4 + b] << (b * 8);
        }
        *(volatile uint32_t *)(uintptr_t)(base + 0x20) = word;
    }
    semihosting_write0("  [diag] ARP request sent (42 bytes) for 192.0.3.1\n");

    /* Check RX immediately after TX (synchronous delivery check) */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX immediately after TX: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Also send ARP for 10.0.2.2 (QEMU user-mode gateway) */
    arp_frame[38] = 10; arp_frame[39] = 0; arp_frame[40] = 2; arp_frame[41] = 2;
    cmd_a = (1u << 13) | (1u << 12) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_a;
    cmd_b = (42u << 16) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_b;
    for (uint32_t i = 0; i < nwords; i++) {
        uint32_t word = 0;
        for (int b = 0; b < 4 && (i * 4 + b) < 42; b++) {
            word |= (uint32_t)arp_frame[i * 4 + b] << (b * 8);
        }
        *(volatile uint32_t *)(uintptr_t)(base + 0x20) = word;
    }
    semihosting_write0("  [diag] ARP request sent (42 bytes) for 10.0.2.2\n");

    /* Check RX immediately after second TX (synchronous delivery check) */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX immediately after 2nd TX: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Now wait for async delivery (WFI triggers QEMU main loop) */
    /* Use busy-wait with WFI instead of vTaskDelay to avoid poll task consuming frames */
    for (int i = 0; i < 200; i++) {
        __asm__ volatile("wfi");
        rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
        rx_used = (rx_inf >> 16) & 0xFF;
        if (rx_used > 0) {
            semihosting_write0("  [diag] RX arrived after ");
            semihosting_write_int(i);
            semihosting_write0(" WFI iterations: ");
            semihosting_write_int((int)rx_used);
            semihosting_write0(" entries\n");
            break;
        }
    }

    /* Final check */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX final: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0(" entries\n");

    /* TX status check */
    int_sts = *(volatile uint32_t *)(uintptr_t)(base + 0x58);
    semihosting_write0("  [diag] INT_STS final: 0x");
    {
        static const char hex[] = "0123456789abcdef";
        char buf[9];
        for (int i = 7; i >= 0; i--) {
            buf[7 - i] = hex[(int_sts >> (i * 4)) & 0xF];
        }
        buf[8] = '\0';
        semihosting_write0(buf);
    }
    semihosting_write0("\n");
}

/*
 * Test TCP connectivity to a given IPv4 address and port.
 * Used for diagnostics during network bring-up.
 * Returns 0 on success, -1 on failure.
 */
/*
 * Test TCP connectivity to a given IPv4 address and port.
 * Returns 0 on success, or the positive errno value on failure.
 */
int nros_freertos_test_tcp_connect(const uint8_t ip[4], uint16_t port) {
    struct sockaddr_in addr;
    int sock;

    sock = lwip_socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0) {
        return errno ? errno : 1;
    }

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = lwip_htons(port);
    addr.sin_addr.s_addr = lwip_htonl(
        ((uint32_t)ip[0] << 24) |
        ((uint32_t)ip[1] << 16) |
        ((uint32_t)ip[2] << 8)  |
        (uint32_t)ip[3]);

    /* Set a 10-second connect/receive timeout. */
    struct timeval tv;
    tv.tv_sec = 10;
    tv.tv_usec = 0;
    lwip_setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));

    int ret = lwip_connect(sock, (struct sockaddr *)&addr, sizeof(addr));
    if (ret < 0) {
        int err = errno;
        lwip_close(sock);
        return err ? err : 1;
    }
    lwip_close(sock);
    return 0;
}

/*
 * Query lwIP netif state for diagnostics.
 * Returns a bitmask:
 *   bit 0: netif_default is set
 *   bit 1: netif is UP
 *   bit 2: link is UP
 *   bit 3: has IP address (non-zero)
 */
int nros_freertos_get_netif_state(void) {
    int flags = 0;
    if (netif_default != NULL) {
        flags |= 1;
        if (netif_default->flags & NETIF_FLAG_UP) flags |= 2;
        if (netif_default->flags & NETIF_FLAG_LINK_UP) flags |= 4;
        if (netif_default->ip_addr.addr != 0) flags |= 8;
    }
    return flags;
}
"#;
