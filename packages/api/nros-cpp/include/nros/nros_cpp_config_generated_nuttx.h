/* Phase 159 (Path C) — NuttX fallback.
 *
 * #167 — this fallback is a snapshot and MUST be a safe UPPER BOUND: when the
 * per-build `<nros/nros_cpp_config_generated.h>` mirror does not reach a TU that
 * emits `Node::GlobalStorageHolder<>::storage` (ODR-picked across the nros-cpp
 * library + entry), the C++ side falls back to these sizes. The old 79304 was
 * STALE — current codegen needs 80712 (rv-virt) / 79952 (arm-virt), so
 * `nros_cpp_init`'s Rust `open_in` wrote past the buffer and smashed a saved
 * return address (rv-virt boot panic EPC=0x4; arm's smaller overflow survived).
 * Keep this comfortably above the largest per-build value.
 */
#ifndef NROS_CPP_CONFIG_GENERATED_NUTTX_H
#define NROS_CPP_CONFIG_GENERATED_NUTTX_H
#define NROS_CPP_EXECUTOR_STORAGE_SIZE 98304
#define NROS_CPP_ACTION_SERVER_STORAGE_SIZE 80
#define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE 48
#define NROS_EXECUTOR_SIZE 98296
#define NROS_GUARD_CONDITION_SIZE 24
#define NROS_PUBLISHER_SIZE 560
#define NROS_SUBSCRIBER_SIZE 560
#define NROS_SERVICE_CLIENT_SIZE 4632
#define NROS_SERVICE_SERVER_SIZE 528
#define NROS_CPP_RAW_SUBSCRIPTION_OPAQUE_U64S 205
#define NROS_CPP_RAW_SERVICE_SERVER_OPAQUE_U64S 194
#define NROS_CPP_RAW_SERVICE_CLIENT_OPAQUE_U64S 707
#define NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S 786
#define NROS_CPP_RAW_ACTION_CLIENT_OPAQUE_U64S 2193
#endif
