// nros_arduino.h — Arduino IDE glue for the nros C API.
//
// Wraps WiFi + zenoh-locator setup so the Arduino-specific code is
// confined to ~70 lines. The rest of the C API
// (`nros_init`, `nros_node_create`, etc.) lives in the precompiled
// `libnanoros.a` shipped under `src/<arch>/`.
//
// Mirrors the micro-ROS Arduino API shape on purpose: users
// migrating from `set_microros_wifi_transports()` should find the
// nros equivalent immediately recognisable. The key difference is
// the absence of a host-side agent — nros sketches connect directly
// to `zenohd`.

#ifndef NANO_ROS_ARDUINO_H
#define NANO_ROS_ARDUINO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The bundled `arduino/nros/src/nros/` headers ship the
// per-build size macros + the public nros-c API
// (`nros_support_*` / `nros_node_*` / `nros_publisher_*` /
// `nros_subscription_*` / `nros_client_*` / `nros_executor_*`).
// Sketches DO NOT call those names directly — they call the
// `nros_*_create` / `nros_publish` / `nros_spin_once` wrappers
// declared below, which mirror the micro-ROS Arduino API shape
// while delegating to the real nros-c executor.
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/subscription.h>
#include <nros/client.h>
#include <nros/executor.h>

// micro-ROS users call the support context `rcl_context_t`. nros
// calls it `nros_support_t`. Alias the Arduino-shape name to the
// real type so sketches keep the micro-ROS-style `nros_context_t`
// spelling.
typedef nros_support_t nros_context_t;

// ────────────────────────────────────────────────────────────────────
// Error-handling macros (mirror micro-ROS's RCCHECK / RCSOFTCHECK)
// ────────────────────────────────────────────────────────────────────

#define NRCHECK(fn)                                                              \
    {                                                                            \
        int _nros_rc = (fn);                                                     \
        if (_nros_rc != 0) {                                                     \
            Serial.printf("[nros] Error %d at %s:%d\n", _nros_rc, __FILE__,      \
                          __LINE__);                                             \
            while (1) {                                                          \
                delay(1000);                                                     \
            }                                                                    \
        }                                                                        \
    }

#define NRSOFTCHECK(fn)                                                          \
    {                                                                            \
        int _nros_rc = (fn);                                                     \
        if (_nros_rc != 0) {                                                     \
            Serial.printf("[nros] Warning %d at %s:%d\n", _nros_rc, __FILE__,    \
                          __LINE__);                                             \
        }                                                                        \
    }

// ────────────────────────────────────────────────────────────────────
// Transport setup
// ────────────────────────────────────────────────────────────────────

/// Connect to a WiFi network and configure the zenoh locator for the
/// next `nros_init()` call. Blocks until `WiFi.status() ==
/// WL_CONNECTED`. The locator string is in zenoh's standard form
/// (`tcp/<host>:<port>`, `udp/<host>:<port>`, …).
void set_nanoros_wifi_transports(const char* ssid, const char* pass,
                                  const char* zenoh_locator);

/// Future: configure Serial as a custom zenoh-pico transport.
void set_nanoros_serial_transports(void);

/// Lightweight reachability check against the configured zenoh
/// router. Returns `true` if the router answers within
/// `timeout_ms`, `false` otherwise. Useful for `Reconnection.ino`.
bool nanoros_ping(uint32_t timeout_ms);

// ────────────────────────────────────────────────────────────────────
// micro-ROS-shaped wrappers (Phase 23.4.x)
//
// Arduino sketches mirror the micro-ROS API: `nros_init(&ctx)` /
// `nros_node_create(&node, &ctx, "name")` / `nros_spin_once(&ctx,
// timeout_ms)`. Under the hood we forward to the rcl-shaped real
// API (`nros_support_init` / `nros_node_init` /
// `nros_executor_spin_some`) plus a hidden global
// `nros_executor_t` so sketches don't have to construct one.
// ────────────────────────────────────────────────────────────────────

#define NANO_ROS_DEFAULT_DOMAIN_ID  0u
#define NANO_ROS_DEFAULT_MAX_HANDLES 16u
#define NANO_ROS_DEFAULT_NAMESPACE   "/"

/// Initialize the nros support context using the locator stashed
/// by `set_nanoros_wifi_transports()` and bring up the hidden
/// global executor. `nros_spin_once(&ctx, …)` spins that
/// executor. Domain id defaults to 0; override with
/// `nros_init_with_domain` if needed.
int nros_init(nros_context_t* ctx);
int nros_init_with_domain(nros_context_t* ctx, uint8_t domain_id);

/// Finalize the global executor and the support context.
int nros_fini(nros_context_t* ctx);

/// Construct a node bound to `ctx` under the default namespace
/// `"/"`. Wrap a stable zero-init step around `nros_node_init`.
int nros_node_create(nros_node_t* node, nros_context_t* ctx,
                      const char* name);
int nros_node_create_in(nros_node_t* node, nros_context_t* ctx,
                         const char* name, const char* namespace_);
int nros_node_destroy(nros_node_t* node);

/// Publisher creation / typed-publish forwarder. Sketches normally
/// call the codegen-emitted `<package>_msg_<type>_publish` helper
/// directly — `nros_publish` is the raw escape hatch.
int nros_publisher_create(nros_publisher_t* pub,
                           const nros_node_t* node,
                           const char* topic_name,
                           const nros_message_type_t* type_info);
int nros_publisher_destroy(nros_publisher_t* pub);
int nros_publish(const nros_publisher_t* pub,
                  const void* data, size_t len);

/// Subscription creation. `cb` matches the nros-c callback
/// signature `(const uint8_t*, size_t, void*)`; sketches may
/// declare `const void*` and let C convert.
int nros_subscription_create(nros_subscription_t* sub,
                              const nros_node_t* node,
                              const char* topic_name,
                              const nros_message_type_t* type_info,
                              nros_subscription_callback_t cb,
                              void* user_ctx);
int nros_subscription_destroy(nros_subscription_t* sub);

/// Service client. The wrapper registers the client with the
/// hidden global executor so `nros_client_call` works without an
/// explicit `nros_executor_add_client` call. Default response
/// buffer is supplied by the caller via the same pointer that
/// micro-ROS's `rcl_send_request` would write to.
int nros_client_create(nros_client_t* client,
                        const nros_node_t* node,
                        const char* service_name,
                        const nros_service_type_t* type_info);
int nros_client_destroy(nros_client_t* client);
// Use the real `nros_client_call(client, request, request_len,
// response, response_capacity, &response_len)` from
// `<nros/client.h>` directly — the Arduino library does not add a
// shim because nros-c's signature is already the natural shape.

/// Spin the hidden global executor for up to `timeout_ms`.
int nros_spin_once(nros_context_t* ctx, uint32_t timeout_ms);

#ifdef __cplusplus
}
#endif

#endif  // NANO_ROS_ARDUINO_H
