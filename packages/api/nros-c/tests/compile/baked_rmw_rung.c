/* SPDX-License-Identifier: Apache-2.0 */
/*
 * issue 1531 — the C surface consumes the BAKED RMW rung.
 *
 * `NROS_ENTRY_RMW` is on every entry target's compile line whatever the
 * language, and until this probe existed only the C++ headers read it: a C image
 * held its own answer in its own preprocessor and passed `NULL`. This TU pins
 * the three properties that make `<rclc/detail/baked_rmw.h>` a fix rather than a
 * rewrite of twenty call sites.
 *
 * It is COMPILE-only on purpose. Whether the selector reaches the backend is a
 * runtime fact, measured on a two-backend image (see the issue); what a compile
 * can pin is that the forward happens, that it does not happen without a bake,
 * and that the macro does not damage the function it is named after.
 */

#include <rclc/rclc.h>

/* (1) The macro must not eat the FUNCTION. A function-like macro expands only
 * when followed by `(`, so the address of the real symbol is still available —
 * which is what any C consumer holding a table of init functions depends on. If
 * `<rclc/detail/baked_rmw.h>` were ever written as an object-like macro this stops
 * compiling, which is the point. */
typedef rcl_ret_t (*nros_support_init_fn)(rclc_support_t*, int, const char* const*,
                                          const rcl_allocator_t*);
static nros_support_init_fn taken_address = &rclc_support_init;
typedef rcl_ret_t (*nros_support_init_locator_fn)(rclc_support_t*, const char*, uint8_t);
static nros_support_init_locator_fn taken_address_locator = &rclc_support_init_with_locator;

/* (2) The nameless spellings must still COMPILE at a call site, with the bake
 * present or absent. Both arities, because `<rclc/detail/baked_rmw.h>` wraps both and a
 * wrapper with the wrong parameter count is a compile error only where it is
 * actually called. */
rcl_ret_t nros_probe_baked_rmw_rung(rclc_support_t* support);

rcl_ret_t nros_probe_baked_rmw_rung(rclc_support_t* support) {
    rcl_ret_t rc = rclc_support_init(support, 0, NULL, NULL);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rcl_ret_t rc_locator = rclc_support_init_with_locator(support, NULL, 0);
    if (rc_locator != RCL_RET_OK) {
        return rc_locator;
    }
    /* Not reached in any run — this TU is never linked. It exists so the
     * four-argument wrapper is type-checked too. */
    return rclc_support_init_named(support, NULL, 0, "probe");
}

/* (3) `rclc_support_init_rmw` is deliberately NOT wrapped: a caller that names a
 * backend has already answered the question the bake answers. Calling it with a
 * selector must stay legal and must keep its five-argument shape. */
rcl_ret_t nros_probe_explicit_selector(rclc_support_t* support);

rcl_ret_t nros_probe_explicit_selector(rclc_support_t* support) {
    return rclc_support_init_rmw(support, NULL, 0, NULL, "cyclonedds");
}

/* Silence `-Wunused-variable` for the address-taking check without making the
 * variable's existence conditional: the assertion IS that the address can be
 * taken, so the symbol has to stay. */
const void* nros_probe_taken_address(void);

const void* nros_probe_taken_address(void) {
    return (const void*)taken_address;
}

const void* nros_probe_taken_address_locator(void);

const void* nros_probe_taken_address_locator(void) {
    return (const void*)taken_address_locator;
}
