#include <stdint.h>
#include <stdio.h>
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
    extern size_t xPortGetFreeHeapSize(void);
    extern size_t xPortGetMinimumEverFreeHeapSize(void);
    char buf[128];
    snprintf(buf, sizeof(buf),
        "MALLOC FAILED: free=%u min_ever_free=%u\n",
        (unsigned)xPortGetFreeHeapSize(),
        (unsigned)xPortGetMinimumEverFreeHeapSize());
    semihosting_write0(buf);
    for (;;) {}
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

/* ---- Semihosting stdio ---- */
/* Newlib's printf calls _write(fd, buf, len). With -nostartfiles, the
 * semihosting file handles for stdin/stdout/stderr aren't opened by crt0.
 * We open them via SYS_OPEN at startup and map fd 0/1/2 to the returned
 * semihosting handles. */
static int sh_stdout_handle = -1;

static int semihosting_open(const char *path, int mode) {
    uint32_t args[3] = { (uint32_t)path, (uint32_t)mode, (uint32_t)__builtin_strlen(path) };
    int result;
    __asm__ volatile("mov r0, #0x01\n"  /* SYS_OPEN */
                     "mov r1, %1\n"
                     "bkpt #0xAB\n"
                     "mov %0, r0\n"
                     : "=r"(result) : "r"(args) : "r0", "r1", "memory");
    return result;
}

/* Called from app_task_entry before app_main to initialise semihosting I/O. */
static void semihosting_stdio_init(void) {
    /* Open ":tt" in write mode (mode=4) for stdout */
    sh_stdout_handle = semihosting_open(":tt", 4);
}

/* Provides printf() output on QEMU via ARM semihosting SYS_WRITE (0x05).
 * This overrides the stub in libnosys (which returns -1). */
int _write(int fd, const char *buf, int count) {
    int sh_fd = sh_stdout_handle;
    if (sh_fd < 0) {
        /* Fallback before init: use SYS_WRITE0 (goes to stderr/debug) */
        char tmp[256];
        int rem = count;
        const char *p = buf;
        while (rem > 0) {
            int chunk = rem < (int)(sizeof(tmp) - 1) ? rem : (int)(sizeof(tmp) - 1);
            for (int i = 0; i < chunk; i++) tmp[i] = p[i];
            tmp[chunk] = '\0';
            semihosting_write0(tmp);
            p += chunk;
            rem -= chunk;
        }
        return count;
    }
    (void)fd;
    uint32_t args[3] = { (uint32_t)sh_fd, (uint32_t)buf, (uint32_t)count };
    uint32_t result;
    __asm__ volatile("mov r0, #0x05\n"
                     "mov r1, %1\n"
                     "bkpt #0xAB\n"
                     "mov %0, r0\n"
                     : "=r"(result) : "r"(args) : "r0", "r1", "memory");
    return count - (int)result;
}

/* ---- nros platform functions ---- */
/* Required by nros-c on no_std platforms. The Rust platform layer provides
 * these for Rust examples; C/C++ examples must provide them in C. */

uint64_t nros_platform_time_ns(void) {
    /* TickType_t is 32-bit on this port. Convert ticks to nanoseconds. */
    return (uint64_t)xTaskGetTickCount() * (1000000000ULL / configTICK_RATE_HZ);
}

void nros_platform_sleep_ns(uint64_t ns) {
    uint32_t ms = (uint32_t)(ns / 1000000ULL);
    if (ms == 0) ms = 1;
    vTaskDelay(pdMS_TO_TICKS(ms));
}

void nros_platform_atomic_store_bool(_Bool *ptr, _Bool value) {
    __atomic_store_n(ptr, value, __ATOMIC_RELEASE);
}

_Bool nros_platform_atomic_load_bool(const _Bool *ptr) {
    return __atomic_load_n(ptr, __ATOMIC_ACQUIRE);
}

/* ---- C/C++ application entry point ---- */
/* Replaces the Rust _start() → run() flow for pure C/C++ examples. */

extern void app_main(void);

/* Configuration — set via -D compile flags from CMakeLists.txt */
#ifndef APP_MAC
#define APP_MAC {0x02, 0x00, 0x00, 0x00, 0x00, 0x00}
#endif
#ifndef APP_IP
#define APP_IP {192, 0, 3, 10}
#endif
#ifndef APP_NETMASK
#define APP_NETMASK {255, 255, 255, 0}
#endif
#ifndef APP_GATEWAY
#define APP_GATEWAY {192, 0, 3, 1}
#endif

#define APP_TASK_STACK   16384
#define APP_TASK_PRIORITY 3
#define POLL_TASK_STACK   256
#define POLL_TASK_PRIORITY 4
#define POLL_INTERVAL_MS  1

static void poll_task_entry(void *arg) {
    (void)arg;
    for (;;) {
        vTaskDelay(pdMS_TO_TICKS(POLL_INTERVAL_MS));
        nros_freertos_poll_network();
    }
}

static void app_task_entry(void *arg) {
    (void)arg;

    const uint8_t mac[] = APP_MAC;
    const uint8_t ip[] = APP_IP;
    const uint8_t netmask[] = APP_NETMASK;
    const uint8_t gw[] = APP_GATEWAY;

    if (nros_freertos_init_network(mac, ip, netmask, gw) != 0) {
        semihosting_write0("Network init failed\n");
        for (;;) {}
    }

    /* Wait for tcpip_thread to run and netif to come up */
    vTaskDelay(pdMS_TO_TICKS(2000));

    semihosting_write0("Network ready\n");

    /* Create poll task */
    nros_freertos_create_task(poll_task_entry, "poll", POLL_TASK_STACK, 0, POLL_TASK_PRIORITY);

    /* Initialise semihosting stdio so printf() routes to QEMU stdout.
     * Disable buffering so output is visible immediately (important for
     * test harnesses that capture stdout from QEMU processes). */
    semihosting_stdio_init();
    setvbuf(stdout, NULL, _IONBF, 0);

    /* Run user application */
    app_main();

    /* Semihosting exit */
    {
        uint32_t exit_args[2] = { 0x20026, 0 }; /* ADP_Stopped_ApplicationExit */
        __asm__ volatile("mov r0, #0x18\nmov r1, %0\nbkpt #0xAB\n" : : "r"(exit_args) : "r0", "r1", "memory");
    }
    for (;;) {}
}

void _start(void) {
    nros_freertos_create_task(app_task_entry, "app", APP_TASK_STACK, 0, APP_TASK_PRIORITY);
    nros_freertos_start_scheduler();
    for (;;) {}
}
