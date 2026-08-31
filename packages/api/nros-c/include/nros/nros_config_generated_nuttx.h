/* Phase 159 (Path C) — NuttX fallback. */
#ifndef NROS_CONFIG_GENERATED_NUTTX_H
#define NROS_CONFIG_GENERATED_NUTTX_H
#include <stdint.h>
/* #167 — safe upper bound (was 79296, stale): current codegen needs ~80704 on
 * rv-virt; too-small here overflows the executor storage buffer. Keep above the
 * largest per-build value; the per-build header supersedes this when mirrored. */
#define NROS_EXECUTOR_STORAGE_SIZE 98304
#define NROS_EXECUTOR_SIZE 98296
#define NROS_GUARD_CONDITION_SIZE 24
#define NROS_PUBLISHER_SIZE 560
#define NROS_SUBSCRIBER_SIZE 560
#define NROS_SERVICE_CLIENT_SIZE 4632
#define NROS_SERVICE_SERVER_SIZE 528
/* issue 0954 — was 528 / 66 u64s. `_z_session_t` grew 8 bytes when it gained
 * `_mutex_transport` + `_reconnecting` (issues 0899 / 0924), and this file must
 * be an UPPER BOUND over every per-build value: freshly built headers now read
 * 536, so 66 * 8 = 528 left the opaque array eight bytes short of the struct it
 * stores. Same shape as #167 and #464 above — a per-build size moved and this
 * hand-maintained twin did not. */
#define NROS_SESSION_SIZE 536
#define NROS_LIFECYCLE_CTX_SIZE 64
#define NROS_ACTION_SERVER_INTERNAL_SIZE 96
#define SESSION_OPAQUE_U64S 67 /* 67 * 8 = 536, issue 0954 */
#define PUBLISHER_OPAQUE_U64S 70
/* #464 — was 9912, i.e. 79296 bytes: the value #167 REPLACED in the two macros
 * above and missed here, even though this is the one that sizes the array
 * (`uint64_t _opaque[EXECUTOR_OPAQUE_U64S]` in nros_generated.h). The comment
 * above states rv-virt needs ~80704, so 9912 was already too small by this
 * file's own requirement, and it contradicted NROS_EXECUTOR_STORAGE_SIZE by
 * 19008 bytes. `98304 / 8` keeps the three in agreement — asserted below. */
#define EXECUTOR_OPAQUE_U64S 12288
#define GUARD_HANDLE_OPAQUE_U64S 3
#define NROS_LIFECYCLE_CTX_OPAQUE_U64S 8
#undef SUBSCRIPTION_OPAQUE_U64S
#define SUBSCRIPTION_OPAQUE_U64S 205
#undef SERVICE_SERVER_OPAQUE_U64S
#define SERVICE_SERVER_OPAQUE_U64S 194
#undef SERVICE_CLIENT_OPAQUE_U64S
#define SERVICE_CLIENT_OPAQUE_U64S 707
#undef ACTION_SERVER_OPAQUE_U64S
/* #464 postscript — was 786, below a real per-build value (a host probe of the
 * same type measures 799). This file must be an upper bound over ALL per-build
 * values; raised with margin, matching the C++ twin. NOT verified against a
 * 32-bit NuttX build, where the true size is likely smaller — raised because
 * 786 demonstrably failed the contract for one configuration, not because 816
 * was measured anywhere. */
#define ACTION_SERVER_OPAQUE_U64S 816
#undef ACTION_CLIENT_OPAQUE_U64S
#define ACTION_CLIENT_OPAQUE_U64S 2193
/* Issue 0464 — this file is a hand-maintained SNAPSHOT, so the pairs below can
 * drift against each other, and one of them did: #167 bumped
 * NROS_EXECUTOR_{STORAGE_,}SIZE and left EXECUTOR_OPAQUE_U64S at the stale
 * 9912, leaving the array 19008 bytes shorter than the size the same file
 * declared for what goes in it. Nothing noticed, because a snapshot cannot
 * observe the type it describes.
 *
 * These assertions cost nothing and make the file self-checking: every
 * `_opaque` width must cover the size this header itself states. They do not
 * make the snapshot CURRENT — only the per-build header can, and it supersedes
 * this one whenever the build system supplies it — but they do make an
 * internally contradictory snapshot a compile error at the including TU.
 *
 * C11 / C++11 for `_Static_assert`; older toolchains simply skip the check. */
#if (defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L)
#define NROS__NUTTX_FALLBACK_ASSERT(cond, msg) _Static_assert(cond, msg)
#elif defined(__cplusplus) && __cplusplus >= 201103L
#define NROS__NUTTX_FALLBACK_ASSERT(cond, msg) static_assert(cond, msg)
#else
#define NROS__NUTTX_FALLBACK_ASSERT(cond, msg)
#endif

NROS__NUTTX_FALLBACK_ASSERT(EXECUTOR_OPAQUE_U64S * 8 >= NROS_EXECUTOR_SIZE,
                            "EXECUTOR_OPAQUE_U64S is smaller than NROS_EXECUTOR_SIZE");
NROS__NUTTX_FALLBACK_ASSERT(EXECUTOR_OPAQUE_U64S * 8 >= NROS_EXECUTOR_STORAGE_SIZE,
                            "EXECUTOR_OPAQUE_U64S is smaller than NROS_EXECUTOR_STORAGE_SIZE");
NROS__NUTTX_FALLBACK_ASSERT(SESSION_OPAQUE_U64S * 8 >= NROS_SESSION_SIZE,
                            "SESSION_OPAQUE_U64S is smaller than NROS_SESSION_SIZE");
NROS__NUTTX_FALLBACK_ASSERT(PUBLISHER_OPAQUE_U64S * 8 >= NROS_PUBLISHER_SIZE,
                            "PUBLISHER_OPAQUE_U64S is smaller than NROS_PUBLISHER_SIZE");
NROS__NUTTX_FALLBACK_ASSERT(GUARD_HANDLE_OPAQUE_U64S * 8 >= NROS_GUARD_CONDITION_SIZE,
                            "GUARD_HANDLE_OPAQUE_U64S is smaller than NROS_GUARD_CONDITION_SIZE");
NROS__NUTTX_FALLBACK_ASSERT(
    NROS_LIFECYCLE_CTX_OPAQUE_U64S * 8 >= NROS_LIFECYCLE_CTX_SIZE,
    "NROS_LIFECYCLE_CTX_OPAQUE_U64S is smaller than NROS_LIFECYCLE_CTX_SIZE");

#ifdef __cplusplus
extern "C" {
#endif
typedef struct ActionServerRawHandle {
    uint64_t _opaque[6];
} ActionServerRawHandle;
#ifdef __cplusplus
}
#endif
#endif
