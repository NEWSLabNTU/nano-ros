/*
 * Issue 1719 -- the realloc probe every platform C-port smoke runs.
 *
 * A GROWING `nros_platform_realloc` must copy the OLD block's bytes, never the
 * new size's worth. Of two equal blocks the LOWER is grown and the higher is
 * its `guard`, filled with a pattern nothing else writes; a copy of `GROW`
 * bytes out of the lower block reads straight through `guard`, so the pattern
 * turns up in the grown block's tail. The reach is checked first -- a layout
 * that put `guard` out of it would make the probe vacuous, and a vacuous probe
 * reports PASS.
 *
 * Header-only and `static`, so each smoke keeps its own exit discipline: the
 * caller turns a non-NULL return into its own FAIL.
 */
#ifndef NROS_C_PORT_SMOKE_REALLOC_PROBE_H
#define NROS_C_PORT_SMOKE_REALLOC_PROBE_H

#include <nros/platform.h>

#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* Returns NULL on PASS, else what failed. */
static const char *nros_smoke_realloc_probe(void) {
    enum { OLD = 32, GROW = 4096, RUN = 16 };
    uint8_t *x = (uint8_t *) nros_platform_alloc(OLD);
    uint8_t *y = (uint8_t *) nros_platform_alloc(OLD);
    if (x == NULL || y == NULL) {
        return "realloc probe: alloc";
    }
    uint8_t *a = x < y ? x : y;
    uint8_t *guard = x < y ? y : x;
    if (guard + OLD > a + GROW) {
        return "realloc probe: guard block is not inside the grow's reach";
    }
    memset(a, 0x11, OLD);
    memset(guard, 0xA5, OLD);
    uint8_t *grown = (uint8_t *) nros_platform_realloc(a, GROW);
    if (grown == NULL) {
        return "realloc grow returned NULL";
    }
    for (int i = 0; i < OLD; i++) {
        if (grown[i] != 0x11) {
            return "realloc grow lost the old contents";
        }
    }
    int run = 0;
    int worst = 0;
    for (int i = OLD; i < GROW; i++) {
        run = grown[i] == 0xA5 ? run + 1 : 0;
        worst = run > worst ? run : worst;
    }
    printf("  realloc %d -> %d: longest guard-pattern run in the grown tail = %d\n", OLD, GROW,
           worst);
    if (worst >= RUN) {
        return "realloc grow copied the NEIGHBOUR block (read past the old block's end)";
    }
    uint8_t *shrunk = (uint8_t *) nros_platform_realloc(grown, 8);
    if (shrunk == NULL || shrunk[0] != 0x11 || shrunk[7] != 0x11) {
        return "realloc shrink lost the old contents";
    }
    nros_platform_dealloc(shrunk);
    nros_platform_dealloc(guard);
    return NULL;
}

#endif /* NROS_C_PORT_SMOKE_REALLOC_PROBE_H */
