// nros-cpp: Inline storage size constants
// Freestanding C++ — no exceptions, no STL required
//
// These constants define the inline opaque storage sizes for core entity
// types. They must match the Rust-side compile-time assertions in nros-cpp.
// Values are in bytes (Rust constants are in u64 units, multiply by 8).

#ifndef NROS_CPP_CONFIG_HPP
#define NROS_CPP_CONFIG_HPP

/// Inline storage for publisher handle (bytes).
#define NROS_CPP_PUBLISHER_STORAGE_SIZE (96 * 8)

/// Inline storage for subscription handle (bytes).
#define NROS_CPP_SUBSCRIPTION_STORAGE_SIZE (224 * 8)

/// Inline storage for service server handle (bytes).
#define NROS_CPP_SERVICE_SERVER_STORAGE_SIZE (224 * 8)

/// Inline storage for service client handle (bytes).
#define NROS_CPP_SERVICE_CLIENT_STORAGE_SIZE (224 * 8)

/// Inline storage for guard condition handle (bytes).
#define NROS_CPP_GUARD_CONDITION_STORAGE_SIZE (4 * 8)

/// Inline storage for action server handle (bytes).
/// CppActionServer contains: Option<ActionServerRawHandle> + 4 PendingGoal slots
/// (each with 1024-byte buffer) + action_name (256 bytes).
#define NROS_CPP_ACTION_SERVER_STORAGE_SIZE (768 * 8)

/// Inline storage for action client handle (bytes).
/// CppActionClient contains: ActionClientCore (3 service clients + 1 subscriber
/// + 3 x 1024-byte buffers) + action_name (256 bytes).
#define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE (768 * 8)

#endif // NROS_CPP_CONFIG_HPP
