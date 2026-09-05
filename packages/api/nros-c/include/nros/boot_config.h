/**
 * @file boot_config.h
 * @ingroup grp_node
 * @brief Phase 266 (RFC-0045) — C mirror of `nros_platform_api::BakedBootConfig`.
 *
 * Defines `struct nros_baked_boot_config`, the fixed-layout blob emitted into the
 * `.nros_boot_config` linker section by the entry macro / cmake.  A post-link tool
 * can locate and patch it in place (RFC-0045 Option A).  The Rust-side source of
 * truth is `nros_platform_api::BakedBootConfig` (`repr(C)`); keep this header
 * BYTE-IDENTICAL to that struct — the static asserts below and a Rust offset test
 * in `nros-platform-api` guard against layout drift.
 *
 * This header is self-contained (`<stdint.h>` + `<stddef.h>` only) and compiles
 * as both C11 and C++ (embedded C entries are often compiled as C++ TUs).
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_BOOT_CONFIG_H
#define NROS_BOOT_CONFIG_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Magic word stored in `nros_baked_boot_config::magic`.  ASCII "NRBC". */
#define NROS_BOOT_CONFIG_MAGIC 0x4E524243u

/** Layout version.  The resolver rejects a struct whose version != this.
 *
 * 2 since issue 1050 defect (3) appended `rmw`.  The bump is what the field is
 * for: a reader compiled against version 1 rejects a version-2 blob rather than
 * reading 32 bytes of `rmw` as whatever its own layout put after `namespace_`. */
#define NROS_BOOT_CONFIG_VERSION 2u

/** `set_flags` bit — `node_name` field is valid. */
#define NROS_BOOT_SET_NODE_NAME (1u << 0)

/** `set_flags` bit — `locator` field is valid. */
#define NROS_BOOT_SET_LOCATOR (1u << 1)

/** `set_flags` bit — `domain_id` field is valid. */
#define NROS_BOOT_SET_DOMAIN (1u << 2)

/** `set_flags` bit — `namespace_` field is valid. */
#define NROS_BOOT_SET_NAMESPACE (1u << 3)

/** `set_flags` bit — `rmw` field is valid (issue 1050; layout version 2). */
#define NROS_BOOT_SET_RMW (1u << 4)

/**
 * Build-time-baked boot configuration blob.
 *
 * Mirrors `nros_platform_api::BakedBootConfig` (`#[repr(C)]`).  Fixed size
 * (268 bytes), pointer-free, no padding.  A post-link tool scans for
 * `NROS_BOOT_CONFIG_MAGIC` to locate this struct in a firmware image and can
 * patch the fields in place (RFC-0045).
 *
 * Layout (offsets):
 *   magic      @  0  (4 bytes)
 *   version    @  4  (2 bytes)
 *   set_flags  @  6  (2 bytes)
 *   domain_id  @  8  (4 bytes)
 *   node_name  @ 12  (64 bytes)
 *   locator    @ 76  (96 bytes)
 *   namespace_ @172  (64 bytes)
 *   rmw        @236  (32 bytes)
 *   total size: 268 bytes
 *
 * @note The field is named `namespace_` (trailing underscore) because
 *       `namespace` is a reserved keyword in C++.
 */
struct nros_baked_boot_config {
    uint32_t magic;      /**< 0x4E524243 = "NRBC"; tool scans for this. */
    uint16_t version;    /**< Layout version (currently 1). */
    uint16_t set_flags;  /**< Bitmask: which fields are set (NROS_BOOT_SET_*). */
    uint32_t domain_id;  /**< ROS 2 domain ID (valid when NROS_BOOT_SET_DOMAIN). */
    char node_name[64];  /**< NUL-padded UTF-8 node name (NROS_BOOT_SET_NODE_NAME). */
    char locator[96];    /**< NUL-padded UTF-8 middleware locator (NROS_BOOT_SET_LOCATOR). */
    char namespace_[64]; /**< NUL-padded UTF-8 node namespace (NROS_BOOT_SET_NAMESPACE). */
    /** NUL-padded UTF-8 RMW backend selector, e.g. "uorb" (NROS_BOOT_SET_RMW).
     *
     * 32 bytes because that is the registry's own `BACKEND_NAME_MAX`; a longer
     * name could never resolve, so accepting one here would only move the
     * failure later.  Appended in layout version 2 (issue 1050 defect (3)). */
    char rmw[32];
};

/* ── Layout guards ────────────────────────────────────────────────────────────
 * Fail the compile if this mirror drifts from the Rust source of truth.
 * A matching Rust test in nros-platform-api asserts size + offsets on the
 * Rust side, so a change to BakedBootConfig forces both sides to be updated. */
#if defined(__cplusplus) && __cplusplus >= 201103L
static_assert(sizeof(struct nros_baked_boot_config) == 268,
              "nros_baked_boot_config size must be 268");
static_assert(offsetof(struct nros_baked_boot_config, magic) == 0, "magic offset must be 0");
static_assert(offsetof(struct nros_baked_boot_config, version) == 4, "version offset must be 4");
static_assert(offsetof(struct nros_baked_boot_config, set_flags) == 6,
              "set_flags offset must be 6");
static_assert(offsetof(struct nros_baked_boot_config, domain_id) == 8,
              "domain_id offset must be 8");
static_assert(offsetof(struct nros_baked_boot_config, node_name) == 12,
              "node_name offset must be 12");
static_assert(offsetof(struct nros_baked_boot_config, locator) == 76, "locator offset must be 76");
static_assert(offsetof(struct nros_baked_boot_config, namespace_) == 172,
              "namespace_ offset must be 172");
static_assert(offsetof(struct nros_baked_boot_config, rmw) == 236, "rmw offset must be 236");
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(struct nros_baked_boot_config) == 268,
               "nros_baked_boot_config size must be 268");
_Static_assert(offsetof(struct nros_baked_boot_config, magic) == 0, "magic offset must be 0");
_Static_assert(offsetof(struct nros_baked_boot_config, version) == 4, "version offset must be 4");
_Static_assert(offsetof(struct nros_baked_boot_config, set_flags) == 6,
               "set_flags offset must be 6");
_Static_assert(offsetof(struct nros_baked_boot_config, domain_id) == 8,
               "domain_id offset must be 8");
_Static_assert(offsetof(struct nros_baked_boot_config, node_name) == 12,
               "node_name offset must be 12");
_Static_assert(offsetof(struct nros_baked_boot_config, locator) == 76, "locator offset must be 76");
_Static_assert(offsetof(struct nros_baked_boot_config, namespace_) == 172,
               "namespace_ offset must be 172");
_Static_assert(offsetof(struct nros_baked_boot_config, rmw) == 236, "rmw offset must be 236");
#endif /* static_assert */

/**
 * Return a pointer to the baked node name, or NULL.
 *
 * Returns NULL when:
 *   - @p c is NULL,
 *   - the magic word or version does not match,
 *   - or the `NROS_BOOT_SET_NODE_NAME` bit is not set in `set_flags`.
 *
 * The returned pointer is valid for the lifetime of @p c.  The buffer is
 * NUL-padded; treat it as at most 64 bytes (no guaranteed NUL when the
 * name fills the entire 64-byte buffer).
 */
static inline const char* nros_boot_config_node_name(const struct nros_baked_boot_config* c) {
    if (c == NULL) return NULL;
    if (c->magic != NROS_BOOT_CONFIG_MAGIC || c->version != NROS_BOOT_CONFIG_VERSION) return NULL;
    if ((c->set_flags & NROS_BOOT_SET_NODE_NAME) == 0) return NULL;
    return c->node_name;
}

/**
 * Return a pointer to the baked node namespace, or NULL.
 *
 * issue 0794 — the blob has always carried four fields and C exposed one
 * accessor, so even a correctly-baked namespace could not be read from C.
 * Same contract as nros_boot_config_node_name(): NULL when @p c is NULL, the
 * magic or version does not match, or the `NROS_BOOT_SET_NAMESPACE` bit is
 * clear.  NUL-padded; treat it as at most 64 bytes.
 */
static inline const char* nros_boot_config_namespace(const struct nros_baked_boot_config* c) {
    if (c == NULL) return NULL;
    if (c->magic != NROS_BOOT_CONFIG_MAGIC || c->version != NROS_BOOT_CONFIG_VERSION) return NULL;
    if ((c->set_flags & NROS_BOOT_SET_NAMESPACE) == 0) return NULL;
    return c->namespace_;
}

/**
 * Return a pointer to the baked middleware locator, or NULL.
 *
 * NUL-padded; treat it as at most 96 bytes.  NOTE the entry emitter does not
 * yet SET this field (issue 0794): the accessor is correct and will report
 * NULL until a producer exists, which is the honest answer rather than a
 * zero-length string that reads as "configured empty".
 */
static inline const char* nros_boot_config_locator(const struct nros_baked_boot_config* c) {
    if (c == NULL) return NULL;
    if (c->magic != NROS_BOOT_CONFIG_MAGIC || c->version != NROS_BOOT_CONFIG_VERSION) return NULL;
    if ((c->set_flags & NROS_BOOT_SET_LOCATOR) == 0) return NULL;
    return c->locator;
}

/**
 * Read the baked ROS domain id into @p out.
 *
 * Returns `true` when the value was written, `false` when @p c or @p out is
 * NULL, the magic or version does not match, or `NROS_BOOT_SET_DOMAIN` is
 * clear.  An out-parameter rather than a sentinel return because 0 is a VALID
 * domain — the same reason `NROS_DOMAIN_ID_EXPLICIT_ZERO` exists on the init
 * path (issues 0206/0227).
 *
 * As with the locator, no producer sets this field yet (issue 0794).
 */
static inline bool nros_boot_config_domain_id(const struct nros_baked_boot_config* c,
                                              uint32_t* out) {
    if (c == NULL || out == NULL) return false;
    if (c->magic != NROS_BOOT_CONFIG_MAGIC || c->version != NROS_BOOT_CONFIG_VERSION) return false;
    if ((c->set_flags & NROS_BOOT_SET_DOMAIN) == 0) return false;
    *out = c->domain_id;
    return true;
}

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NROS_BOOT_CONFIG_H */
