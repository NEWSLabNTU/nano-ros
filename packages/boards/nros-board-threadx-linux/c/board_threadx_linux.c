/*
 * board_threadx_linux.c — board-specific glue for nros ThreadX Linux sim.
 *
 * The shared `tx_application_define` + byte-pool / app-thread plumbing
 * lives in `nros_board_common`'s `threadx_hooks.c`. This file fills in
 * the three weak hooks that file calls into, plus the
 * `nros_threadx_set_config` FFI setter whose signature differs from
 * the RISC-V sibling overlay (Linux carries `interface_name`).
 *
 * Networking goes through nsos-netx (NetX BSD shim over host POSIX
 * sockets) — no NetX Duo TCP/IP stack, no IP instance, no packet pool,
 * no ARP, no veth/TAP driver. Application's `nx_bsd_*` calls are
 * forwarded directly to the host kernel.
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "tx_api.h"

/* ---- Configuration (set from Rust before tx_kernel_enter) ---- *
 * IP/MAC/interface fields are accepted but mostly ignored — NSOS
 * uses the host kernel's networking, so no per-instance IP setup is
 * needed. We still cache IP/MAC for the RNG seed derivation. */
static uint8_t cfg_ip[4]  = {127, 0, 0, 1};
static uint8_t cfg_mac[6] = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00};

/* FFI: called from Rust to set config. Signature kept for
 * compatibility with the Rust glue. The netmask / gateway /
 * interface_name parameters are ignored under NSOS. */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name)
{
    (void)netmask;
    (void)gateway;
    (void)interface_name;
    if (ip  != NULL) { memcpy(cfg_ip,  ip,  4); }
    if (mac != NULL) { memcpy(cfg_mac, mac, 6); }
}

/* ---- Weak-hook impls (overrides the defaults in threadx_hooks.c) ---- */

void nros_board_log(const char *s)
{
    if (s) { fputs(s, stdout); }
}

int nros_board_init_eth(void)
{
    /* NSOS uses the host kernel's BSD sockets — nothing to do at
     * the NetX layer. */
    return 0;
}

void nros_board_compute_rng_seed(uint32_t *out)
{
    if (!out) { return; }
    uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                  | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
    seed = seed * 2654435761u;  /* Knuth multiplicative hash */
    seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
    *out = seed;
}
