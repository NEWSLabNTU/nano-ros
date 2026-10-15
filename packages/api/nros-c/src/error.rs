//! Error types and return codes for the C API.

use core::ffi::c_int;

/// Return type for nros C API functions.
///
/// NOT value-compatible with `rcl_ret_t` — only `OK` (0) agrees.
///
/// CORRECTED 2026-09-04 (phase-417 W5.b). This said "Compatible with rcl_ret_t
/// for familiarity", which is false for five of six shared codes: ours are
/// NEGATIVE where rcl's are positive.
///
/// | code | ours | rcl |
/// | --- | ---: | ---: |
/// | OK | 0 | 0 |
/// | ERROR | -1 | 1 |
/// | TIMEOUT | -2 | 2 |
/// | UNSUPPORTED | -16 | 3 |
/// | INVALID_ARGUMENT | -3 | 11 |
/// | NOT_INIT | -7 | 101 |
///
/// A ported `if (ret == RCL_RET_TIMEOUT)` compiled against the old comment and
/// never matched — the "compiles and differs" shape RFC-0089 forbids, arriving
/// through a doc comment rather than a signature.
///
/// `<rclc/detail/rcl_compat.h>` maps rcl's constant SPELLINGS onto these values. The
/// values are deliberately NOT renumbered: doing so would silently flip the
/// meaning of every stored return code across the C, C++ and Rust FFI seams.
pub type rcl_ret_t = c_int;

/// Success
pub const RCL_RET_OK: rcl_ret_t = 0;

/// Generic error
pub const RCL_RET_ERROR: rcl_ret_t = -1;

/// Timeout occurred
pub const RCL_RET_TIMEOUT: rcl_ret_t = -2;

/// Invalid argument passed
pub const RCL_RET_INVALID_ARGUMENT: rcl_ret_t = -3;

/// Resource not found
pub const RCLC_RET_NOT_FOUND: rcl_ret_t = -4;

/// Resource already exists
pub const RCLC_RET_ALREADY_EXISTS: rcl_ret_t = -5;

/// Resource limit reached (e.g., max handles)
pub const RCLC_RET_FULL: rcl_ret_t = -6;

/// Not initialized
pub const RCL_RET_NOT_INIT: rcl_ret_t = -7;

/// Bad sequence (e.g., wrong order of operations)
pub const RCLC_RET_BAD_SEQUENCE: rcl_ret_t = -8;

/// phase-379 W4 — the entity's node reference no longer names a live binding.
///
/// Returned when an entity is used or finalised after `rcl_node_fini` retired
/// the slot it was created on. Before W4 the entity held a raw
/// `*const rcl_node_t` that nothing dereferenced, so this case SUCCEEDED
/// silently; the identity makes it detectable. Distinct from
/// `RCL_RET_NOT_INIT` (never initialised) and `RCLC_RET_BAD_SEQUENCE`
/// (initialised, wrong order) because the remedy differs: the node outlived by
/// this entity has to be finalised LAST.
pub const RCLC_RET_STALE_NODE: rcl_ret_t = -17;

/// Service call failed
pub const RCLC_RET_SERVICE_FAILED: rcl_ret_t = -9;

/// Publish failed
pub const RCLC_RET_PUBLISH_FAILED: rcl_ret_t = -10;

/// Subscription failed
pub const RCLC_RET_SUBSCRIPTION_FAILED: rcl_ret_t = -11;

/// Operation not allowed (e.g., goal not in correct state)
pub const RCLC_RET_NOT_ALLOWED: rcl_ret_t = -12;

/// Request was rejected (e.g., goal rejected by server)
pub const RCLC_RET_REJECTED: rcl_ret_t = -13;

/// Operation not yet ready (e.g., async response still pending).
/// Caller should spin the executor and try again.
pub const RCLC_RET_TRY_AGAIN: rcl_ret_t = -14;

/// Reentrant call detected — a blocking helper (`rclc_client_call`,
/// `rclc_action_send_goal`, `rclc_action_get_result`) was called from
/// inside a dispatch callback. These functions internally call
/// `rclc_executor_spin_some`, which is not reentrant.
pub const RCLC_RET_REENTRANT: rcl_ret_t = -15;

/// The active backend does not implement this operation. Phase 108
/// status-event setters return this until backend wiring lands per
/// phase (109+).
pub const RCL_RET_UNSUPPORTED: rcl_ret_t = -16;
