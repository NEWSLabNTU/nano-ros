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
#define NROS_SESSION_SIZE 528
#define NROS_LIFECYCLE_CTX_SIZE 64
#define NROS_ACTION_SERVER_INTERNAL_SIZE 96
#define SESSION_OPAQUE_U64S 66
#define PUBLISHER_OPAQUE_U64S 70
#define EXECUTOR_OPAQUE_U64S 9912
#define GUARD_HANDLE_OPAQUE_U64S 3
#define NROS_LIFECYCLE_CTX_OPAQUE_U64S 8
#undef SUBSCRIPTION_OPAQUE_U64S
#define SUBSCRIPTION_OPAQUE_U64S 205
#undef SERVICE_SERVER_OPAQUE_U64S
#define SERVICE_SERVER_OPAQUE_U64S 194
#undef SERVICE_CLIENT_OPAQUE_U64S
#define SERVICE_CLIENT_OPAQUE_U64S 707
#undef ACTION_SERVER_OPAQUE_U64S
#define ACTION_SERVER_OPAQUE_U64S 786
#undef ACTION_CLIENT_OPAQUE_U64S
#define ACTION_CLIENT_OPAQUE_U64S 2193
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
