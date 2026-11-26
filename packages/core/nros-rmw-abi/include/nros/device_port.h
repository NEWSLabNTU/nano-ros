#ifndef NROS_DEVICE_PORT_H
#define NROS_DEVICE_PORT_H

#include <stddef.h>
#include <stdint.h>

// This header is the ABI source of truth for device ports: Rust consumes the
// committed bindgen output in nros-rmw-cffi/src/generated.rs. Every field offset is pinned on both
// sides: the C half in nros-rmw-cffi/tests/c_stubs/abi_layout_check.c, the Rust half in the
// device-port `const _` block of nros-rmw-cffi/src/lib.rs. A field moved on one side only fails one
// of them.
//
// The version and the return codes are plain integer literals, not casts:
// bindgen drops a macro whose value is a cast, so `((uint32_t)1)` would leave
// the Rust side with no constant at all.

/**
 * @file device_port.h
 * @brief The device port ABI: workers, sources and completion handles.
 *
 * A board package implements a port for its hardware: a worker (a job goes
 * in, a result comes back, as for an NPU or a codec) or a source (data
 * produced on the device's own clock, as for a camera). nano-ros calls the
 * port's operations and the port signals completion through a handle.
 *
 * ## Rules every port keeps
 *
 *  - `submit` and `take` never block. `take` returns
 *    `NROS_DEVICE_RET_PENDING` while the device is still working.
 *  - `submit` is called only when the device is idle.
 *  - `nros_completion_signal_from_isr` is the only function a port may call
 *    from an interrupt handler. Every other call is thread context.
 *  - A port never sees the executor. It holds only completion handles.
 *  - `user_data` is opaque to the runtime and threaded back into every call.
 *
 * ## Versioning
 *
 *  - `abi_version` must equal `NROS_DEVICE_PORT_ABI_V1`. A table with any
 *    other value is refused at registration with
 *    `NROS_DEVICE_RET_ABI_MISMATCH`.
 *  - Fields are only ever appended. A field removed or reordered is a new
 *    major version.
 */

#ifdef __cplusplus
extern "C" {
#endif

/** The current version of the port operation tables. */
#define NROS_DEVICE_PORT_ABI_V1 1

/**
 * The result of a port operation. An `int32_t` rather than an enum, so it is
 * four bytes on every target.
 */
typedef int32_t nros_device_ret_t;

/** The operation succeeded. */
#define NROS_DEVICE_RET_OK 0
/** `take`: the device has not finished yet; ask again after completion. */
#define NROS_DEVICE_RET_PENDING 1
/** `submit`: the device refused the job (bad parameters, busy, not ready). */
#define NROS_DEVICE_RET_REJECTED (-1)
/** The device reported a fault while running the job. */
#define NROS_DEVICE_RET_FAULT (-2)
/** The job did not complete in the time the port allows. */
#define NROS_DEVICE_RET_TIMEOUT (-3)
/** The operation is not implemented by this port. */
#define NROS_DEVICE_RET_UNSUPPORTED (-4)
/** The table's `abi_version` is not one this runtime speaks. */
#define NROS_DEVICE_RET_ABI_MISMATCH (-5)

/**
 * A completion handle. The runtime hands one to `submit` (a worker) or
 * `start` (a source); the port signals it when the device has a result.
 * It is a value: a port may copy it and store it where its interrupt
 * handler can reach it.
 */
typedef struct nros_completion_s {
    /** The runtime's slot. Opaque to the port. */
    void* _slot;
} nros_completion_t;

/** Signal a completion from thread context. */
void nros_completion_signal(nros_completion_t c);

/**
 * Signal a completion from an interrupt handler. The only call a port may
 * make in interrupt context. Bounded and non-blocking.
 */
void nros_completion_signal_from_isr(nros_completion_t c);

/** A buffer a job reads or a result is written to. */
typedef struct nros_device_buffer_s {
    /** The bytes. */
    void* data;
    /** Their capacity, in bytes. */
    size_t size;
} nros_device_buffer_t;

/**
 * A job, type-erased. `op` points at the job of the device's class (an
 * inference job, a 2D operation); the companion package's per-class headers
 * define those types.
 */
typedef struct nros_device_job_s {
    /** The class-specific operation. */
    const void* op;
    /** The input. */
    nros_device_buffer_t input;
    /** Where the output goes. */
    nros_device_buffer_t output;
} nros_device_job_t;

/**
 * A result, type-erased. The capture time is a pair read at one instant:
 * `captured_at_ns` on the monotonic clock, which a stage's maximum age is
 * measured against, and `captured_stamp_ns` on the wall clock, which a
 * message header takes.
 */
typedef struct nros_device_result_s {
    /** The buffer the result is in. */
    nros_device_buffer_t output;
    /** The bytes of `output` the device wrote. */
    size_t length;
    /** When the input was captured, monotonic, in nanoseconds. */
    int64_t captured_at_ns;
    /** The wall clock at that instant, in nanoseconds since the epoch. */
    int64_t captured_stamp_ns;
    /** The job's outcome: `NROS_DEVICE_RET_OK` or an error. */
    nros_device_ret_t status;
    /** Reserved; zero. */
    uint32_t _reserved;
} nros_device_result_t;

/** A worker port: a job goes in, a result comes back. */
typedef struct nros_worker_port_ops_s {
    /** Must equal `NROS_DEVICE_PORT_ABI_V1`. */
    uint32_t abi_version;
    /** Reserved; zero. */
    uint32_t _reserved;
    /** The port's own context, passed back as the first argument. */
    void* user_data;
    /**
     * Start `job` on the device and return at once. The port signals `done`
     * when the device has finished.
     */
    nros_device_ret_t (*submit)(void* user_data, const nros_device_job_t* job,
                                nros_completion_t done);
    /**
     * Collect the result of the finished job into `out`, or return
     * `NROS_DEVICE_RET_PENDING` if it has not finished.
     */
    nros_device_ret_t (*take)(void* user_data, nros_device_result_t* out);
    /** Abandon the running job. May be NULL when the device cannot. */
    nros_device_ret_t (*cancel)(void* user_data);
} nros_worker_port_ops_t;

/** A source port: data produced on the device's own clock. */
typedef struct nros_source_port_ops_s {
    /** Must equal `NROS_DEVICE_PORT_ABI_V1`. */
    uint32_t abi_version;
    /** Reserved; zero. */
    uint32_t _reserved;
    /** The port's own context, passed back as the first argument. */
    void* user_data;
    /** Start producing. The port signals `ready` each time an item is ready. */
    nros_device_ret_t (*start)(void* user_data, nros_completion_t ready);
    /**
     * Collect the next ready item into `out`, or return
     * `NROS_DEVICE_RET_PENDING` if none is ready.
     */
    nros_device_ret_t (*take)(void* user_data, nros_device_result_t* out);
    /** Stop producing. */
    void (*stop)(void* user_data);
} nros_source_port_ops_t;

#ifdef __cplusplus
}
#endif

#endif /* NROS_DEVICE_PORT_H */
