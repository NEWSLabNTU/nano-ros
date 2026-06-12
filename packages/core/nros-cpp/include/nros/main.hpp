// Phase 219.E / 235.A — `<nros/main.hpp>` Entry-pkg header.
//
// The cmake fn `nano_ros_entry(LAUNCH "<bringup>:<file>.launch.xml")`
// drives the per-Entry-pkg codegen via `nros codegen entry --lang cpp`,
// then appends the generated TU to the executable target's sources.
// The generated TU has the canonical body — `int main()` + the
// `nros::board::NativeBoard::run(lambda)` boot stub + the per-Node
// register-call sequence.
//
// This header provides two ingredients the generated TU needs:
//
//   1. `NROS_MAIN(<Board>, "<bringup>:<file>.launch.xml")` — empty-
//      expansion macro the user's own TU may carry as a doc/IDE hint
//      (parallels Rust's `nros::main!(launch = "…")`). It expands to
//      a sentinel symbol the cmake fn can detect with
//      `target_compile_definitions` to avoid double-emit when the
//      user wrote it. The actual code generation happens via the CLI;
//      the macro itself is declarative.
//
//   2. `nros::board::<Board>::run(register_fn)` — the Board adapter shim
//      the generated TU calls. Owns the
//      `nros::init() → register_fn(context) → spin → nros::shutdown()`
//      lifecycle so the generated TU stays one declarative lambda.
//
// Two Board adapters ship (Phase 235.B):
//   * `nros::board::NativeBoard` — host/POSIX; runtime domain + locator.
//   * `nros::board::ZephyrBoard` — embedded Zephyr; compile-time domain
//     id, network-wait hook, cooperative spin. Selected through the
//     Phase 215 `nano_ros_use_board(<name>)` import (board.cmake feeds
//     the default RMW + `west` runner). cf. RFC-0032 §8a.
//
// Both adapters share the SAME `detail::EntryNodeRuntime` ops + arena —
// only the boot lifecycle differs (init / network-wait / cooperative
// yield). The op set is factored into `detail::entry_register` +
// `detail::entry_node_context_ops` so neither board duplicates it.

#ifndef NROS_CPP_MAIN_HPP
#define NROS_CPP_MAIN_HPP

#include "nros/nros.hpp"
#include "nros/node_pkg.hpp"

#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdlib> // getenv — Phase 235.A bounded-spin ($NROS_ENTRY_SPIN_MS)
#ifdef NROS_NUTTX_ENTRY_DEBUG
#include <cstdio> // Phase 238 NuttxBoard boot diagnostics (opt-in)
#endif
#endif

// Phase 235.B — the embedded (Zephyr) Board adapter is cooperatively
// scheduled, so the shared Entry spin loop yields each tick (`k_yield()`)
// to let the network stack + peer threads run. Pull the kernel header
// only on Zephyr; the native path keeps the loop dependency-free.
#ifdef __ZEPHYR__
#include <zephyr/kernel.h>
#endif

// Phase 235.A — fixed-capacity arena dimensions for the native
// NodeContext runtime. The runtime is `no_std` / heap-free (mirrors the
// nros-cpp inline-storage discipline), so it pre-sizes its node + entity
// tables at compile time. Override before including this header if an
// Entry pkg declares more than the defaults.
#ifndef NROS_ENTRY_MAX_NODES
#define NROS_ENTRY_MAX_NODES 8
#endif
#ifndef NROS_ENTRY_MAX_ENTITIES
#define NROS_ENTRY_MAX_ENTITIES 24
#endif

namespace nros {
namespace board {
namespace detail {

// ----- bounded-string + parse helpers (no STL) ----------------------

inline void entry_copy_str(char* dst, const char* src, size_t cap) {
    size_t i = 0;
    if (src != nullptr) {
        for (; src[i] != '\0' && i + 1 < cap; ++i) {
            dst[i] = src[i];
        }
    }
    dst[i] = '\0';
}

inline bool entry_str_eq(const char* a, const char* b) {
    if (a == nullptr || b == nullptr) return false;
    while (*a != '\0' && *b != '\0') {
        if (*a != *b) return false;
        ++a;
        ++b;
    }
    return *a == *b;
}

inline bool entry_str_contains(const char* hay, const char* needle) {
    if (hay == nullptr || needle == nullptr) return false;
    for (const char* h = hay; *h != '\0'; ++h) {
        const char* a = h;
        const char* b = needle;
        while (*a != '\0' && *b != '\0' && *a == *b) {
            ++a;
            ++b;
        }
        if (*b == '\0') return true;
    }
    return false;
}

inline uint32_t entry_parse_u32(const char* s) {
    uint32_t v = 0;
    if (s == nullptr) return 0;
    for (; *s >= '0' && *s <= '9'; ++s) {
        v = v * 10 + static_cast<uint32_t>(*s - '0');
    }
    return v;
}

inline nros_cpp_qos_t entry_default_ffi_qos() {
    ::nros::QoS q = ::nros::QoS::default_profile();
    nros_cpp_qos_t f;
    f.reliability = static_cast<nros_cpp_qos_reliability_t>(q.reliability_raw());
    f.durability = static_cast<nros_cpp_qos_durability_t>(q.durability_raw());
    f.history = static_cast<nros_cpp_qos_history_t>(q.history_raw());
    f.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(q.liveliness_raw());
    f.depth = q.depth();
    f.deadline_ms = q.deadline_ms();
    f.lifespan_ms = q.lifespan_ms();
    f.liveliness_lease_ms = q.liveliness_lease_ms();
    f.avoid_ros_namespace_conventions = q.avoid_ros_namespace_conventions() ? 1 : 0;
    return f;
}

// Phase 235.B — per-tick cooperative yield in the shared Entry spin loop.
// Native relies on `spin_once`'s blocking I/O wait for pacing (no-op
// here); Zephyr is cooperatively scheduled, so each tick must `k_yield()`
// to release the CPU to the network stack + peer threads. Keeping this in
// one helper is what lets `NativeBoard` and `ZephyrBoard` share the exact
// same `EntryNodeRuntime::spin()` body.
inline void entry_tick_yield() {
#ifdef __ZEPHYR__
    k_yield();
#endif
}

/// Phase 235.A / 235.B — the **real**, lifecycle-agnostic Entry
/// NodeContext runtime (shared by every Board adapter).
///
/// Originally `NativeNodeRuntime` (235.A); renamed `EntryNodeRuntime` in
/// 235.B once the embedded `ZephyrBoard` started sharing the exact same
/// `create_node` / `create_entity` / `record_callback_effect` ops + poll
/// loop. **Only the boot lifecycle differs** between boards
/// (init / network-wait / yield); the runtime ops are identical, so they
/// live here once and both `NativeBoard` and `ZephyrBoard` install them.
/// A `NativeNodeRuntime` alias is kept below for source-compat.
///
/// Replaces the Phase 219.B recording no-op op set. Maps each recorded
/// register call onto a live `nros-cpp` construction:
///
///   * `create_node`            → `nros::create_node` into an arena slot.
///   * `create_entity`          → the matching raw `nros_cpp_*_create`
///                                FFI on the owning node (the op boundary
///                                is type-erased — entities arrive as
///                                descriptor *strings* — so the runtime
///                                cannot use the typed `create_publisher<M>`
///                                templates and goes through the raw FFI
///                                with the descriptor's `type_name` /
///                                `type_hash`).
///   * `record_callback_effect` → wire the effect into the poll loop:
///                                a `Reads` effect drains its subscription
///                                each spin tick; a timer-driven
///                                `Publishes` effect fires its publisher
///                                on the timer period.
///
/// Storage is a fixed-capacity arena owned at process scope (see
/// `detail::EntryRuntimeHolder`), so every entity outlives the
/// `register_fn` lambda and the whole spin loop (235.A.2). No heap, no
/// STL — same inline-storage discipline as `nros::Publisher<M>` et al.
///
/// **Synthesized publish body (v1).** The declarative Node-pkg
/// `register_node()` only *describes* a `Publishes` callback by name
/// (`"on_tick"`); it carries no executable body (RFC-0032 §8a "Open:
/// callback bodies"). To make a timer-driven publisher emit observable
/// data the runtime synthesizes a monotonic `std_msgs/Int32` counter on
/// each tick (matching the canonical Talker's "the timer fires
/// `on_tick`, which publishes a counter" intent). Publishers of other
/// types are created live but not auto-driven until the callback-body
/// binding lands.
class EntryNodeRuntime {
  public:
    static constexpr size_t ID_CAP = ::nros::DECLARED_NODE_SYNTHETIC_ID_MAX;
    static constexpr size_t TYPE_CAP = 128;
    static constexpr size_t HASH_CAP = 80;
    static constexpr size_t TOPIC_CAP = 256;
    static constexpr size_t ENTITY_STORAGE =
        (NROS_PUBLISHER_SIZE > NROS_SUBSCRIBER_SIZE) ? NROS_PUBLISHER_SIZE : NROS_SUBSCRIBER_SIZE;

    void reset() {
        node_count_ = 0;
        entity_count_ = 0;
        for (size_t i = 0; i < NROS_ENTRY_MAX_NODES; ++i) {
            nodes_[i].used = false;
        }
        for (size_t i = 0; i < NROS_ENTRY_MAX_ENTITIES; ++i) {
            entities_[i].used = false;
        }
    }

    // ---- NodeContextOps trampolines (signatures match the typedefs) ----

    static int32_t op_create_node(void* user, const char* stable_id,
                                  const ::nros::NodeOptions* opts, ::nros::DeclaredNode* /*out*/) {
        return static_cast<EntryNodeRuntime*>(user)->do_create_node(stable_id, opts);
    }

    static int32_t op_create_entity(void* user, const void* descriptor) {
        return static_cast<EntryNodeRuntime*>(user)->do_create_entity(
            static_cast<const ::nros::detail::NodeEntityDescriptor*>(descriptor));
    }

    static int32_t op_record_callback_effect(void* user, const char* callback_id,
                                             ::nros::CallbackEffectKind kind,
                                             const char* entity_id) {
        return static_cast<EntryNodeRuntime*>(user)->do_record_effect(callback_id, kind, entity_id);
    }

    /// Drive the constructed topology until `nros::ok()` flips false, or
    /// for `$NROS_ENTRY_SPIN_MS` milliseconds when set (the bounded
    /// external-observer test path — mirrors the Rust entry harness's
    /// `NROS_ENTRY_SPIN_MS`). Each tick: `spin_once`, fire any due
    /// timer-driven publishers, then drain every `Reads` subscription.
    ::nros::Result spin() {
        uint32_t bound_ms = 0;
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
        const char* env = ::std::getenv("NROS_ENTRY_SPIN_MS");
        if (env != nullptr && env[0] != '\0') {
            bound_ms = entry_parse_u32(env);
        }
#endif
        const uint64_t start_ns = nros_cpp_time_ns();
        // Seed each publishing entity's first fire one period out.
        for (size_t i = 0; i < entity_count_; ++i) {
            Entity& e = entities_[i];
            if (e.used && e.publish_period_ms > 0) {
                e.next_fire_ns = start_ns + static_cast<uint64_t>(e.publish_period_ms) * 1000000ull;
            }
        }

        ::nros::Result last = ::nros::Result::success();
        uint8_t drain[512];
        for (;;) {
            if (bound_ms == 0 && !::nros::ok()) break;

            last = ::nros::spin_once(10);
            if (!last.ok()) return last;

            const uint64_t now = nros_cpp_time_ns();
            for (size_t i = 0; i < entity_count_; ++i) {
                Entity& e = entities_[i];
                if (!e.used) continue;

                if (e.kind == ::nros::NodeEntityKind::Publisher && e.publish_period_ms > 0 &&
                    now >= e.next_fire_ns) {
                    fire_publisher(e);
                    const uint64_t period_ns =
                        static_cast<uint64_t>(e.publish_period_ms) * 1000000ull;
                    e.next_fire_ns += period_ns;
                    if (e.next_fire_ns <= now) {
                        e.next_fire_ns = now + period_ns;
                    }
                } else if (e.kind == ::nros::NodeEntityKind::Subscription && e.reads) {
                    // Drain queued samples — this is the poll-loop wiring
                    // of the subscription's `Reads` callback effect.
                    for (int n = 0; n < 16; ++n) {
                        size_t len = 0;
                        nros_cpp_ret_t r = nros_cpp_subscription_try_recv_raw(e.storage, drain,
                                                                              sizeof(drain), &len);
                        if (r != 0 || len == 0) break;
                        ++e.recv_count;
                    }
                }
            }

            if (bound_ms != 0) {
                const uint64_t elapsed_ms = (nros_cpp_time_ns() - start_ns) / 1000000ull;
                if (elapsed_ms >= bound_ms) break;
                if (!::nros::ok()) break;
            }

            // Phase 235.B — cooperative tick. No-op on native (where
            // `spin_once` blocks on I/O for pacing); `k_yield()` on Zephyr
            // so the cooperatively-scheduled network stack + peer threads
            // get the CPU between ticks. This is the ONLY platform-divergent
            // line in the shared runtime — the lifecycle hooks (init /
            // network-wait) live in the per-Board adapters below.
            entry_tick_yield();
        }
        return last;
    }

    /// Diagnostics — number of samples the runtime drained from the
    /// subscription whose stable id is `entity_id` (0 if none / unknown).
    uint32_t received_count(const char* entity_id) const {
        for (size_t i = 0; i < entity_count_; ++i) {
            if (entities_[i].used && entry_str_eq(entities_[i].id, entity_id)) {
                return entities_[i].recv_count;
            }
        }
        return 0;
    }

  private:
    struct NodeSlot {
        char id[ID_CAP];
        ::nros::Node node;
        bool used = false;
    };

    struct Entity {
        bool used = false;
        ::nros::NodeEntityKind kind = ::nros::NodeEntityKind::Publisher;
        char id[ID_CAP];
        char node_id[ID_CAP];
        char callback_id[ID_CAP];
        char type_name[TYPE_CAP];
        char type_hash[HASH_CAP];
        char topic[TOPIC_CAP];
        uint32_t period_ms = 0;         // Timer: declared period
        int32_t publish_period_ms = -1; // Publisher: bound timer period, -1 = none
        bool reads = false;             // Subscription: drained in the poll loop
        uint64_t next_fire_ns = 0;      // Publisher: next synthesized publish
        int32_t counter = 0;            // Publisher: synthesized Int32 value
        uint32_t recv_count = 0;        // Subscription: drained-sample count
        alignas(8) uint8_t storage[ENTITY_STORAGE];
    };

    NodeSlot* find_node(const char* id) {
        for (size_t i = 0; i < node_count_; ++i) {
            if (nodes_[i].used && entry_str_eq(nodes_[i].id, id)) return &nodes_[i];
        }
        return nullptr;
    }

    Entity* find_entity(const char* id) {
        for (size_t i = 0; i < entity_count_; ++i) {
            if (entities_[i].used && entry_str_eq(entities_[i].id, id)) return &entities_[i];
        }
        return nullptr;
    }

    Entity* find_timer_for_callback(const char* callback_id) {
        for (size_t i = 0; i < entity_count_; ++i) {
            Entity& e = entities_[i];
            if (e.used && e.kind == ::nros::NodeEntityKind::Timer &&
                entry_str_eq(e.callback_id, callback_id)) {
                return &e;
            }
        }
        return nullptr;
    }

    int32_t do_create_node(const char* stable_id, const ::nros::NodeOptions* opts) {
        if (stable_id == nullptr) return static_cast<int32_t>(::nros::ErrorCode::InvalidArgument);
        if (node_count_ >= NROS_ENTRY_MAX_NODES) {
            return static_cast<int32_t>(::nros::ErrorCode::Full);
        }
        NodeSlot& slot = nodes_[node_count_];
        entry_copy_str(slot.id, stable_id, ID_CAP);
        const char* name = (opts != nullptr && opts->name != nullptr) ? opts->name : stable_id;
        const char* ns = (opts != nullptr) ? opts->namespace_ : nullptr;
        ::nros::Result r = ::nros::create_node(slot.node, name, ns);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        slot.used = true;
        ++node_count_;
        return 0;
    }

    int32_t do_create_entity(const ::nros::detail::NodeEntityDescriptor* d) {
        if (d == nullptr || d->stable_id == nullptr || d->node_id == nullptr) {
            return static_cast<int32_t>(::nros::ErrorCode::InvalidArgument);
        }
        NodeSlot* node = find_node(d->node_id);
        if (node == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);
        if (entity_count_ >= NROS_ENTRY_MAX_ENTITIES) {
            return static_cast<int32_t>(::nros::ErrorCode::Full);
        }
        Entity& e = entities_[entity_count_];
        entry_copy_str(e.id, d->stable_id, ID_CAP);
        entry_copy_str(e.node_id, d->node_id, ID_CAP);
        entry_copy_str(e.callback_id, d->callback_id, ID_CAP);
        entry_copy_str(e.type_name, d->type_name, TYPE_CAP);
        entry_copy_str(e.type_hash, d->type_hash, HASH_CAP);
        entry_copy_str(e.topic, d->source_name, TOPIC_CAP);
        e.kind = d->kind;
        e.reads = false;
        e.publish_period_ms = -1;
        e.counter = 0;
        e.recv_count = 0;

        nros_cpp_qos_t qos = entry_default_ffi_qos();
        nros_cpp_ret_t ret = 0;
        switch (d->kind) {
        case ::nros::NodeEntityKind::Publisher:
            ret = nros_cpp_publisher_create(node->node.ffi_handle(), e.topic, e.type_name,
                                            e.type_hash, qos, e.storage);
            break;
        case ::nros::NodeEntityKind::Subscription:
            ret = nros_cpp_subscription_create(node->node.ffi_handle(), e.topic, e.type_name,
                                               e.type_hash, qos, e.storage);
            break;
        case ::nros::NodeEntityKind::Timer:
            // `source_name` carries the period-ms literal ("1000").
            e.period_ms = entry_parse_u32(e.topic);
            break;
        default:
            // Services / clients / actions / parameters are not yet
            // constructed by the native runtime; recording them keeps the
            // register sequence intact (no hard error) so a mixed Entry
            // pkg still boots its pub/sub topology.
            break;
        }
        if (ret != 0) return static_cast<int32_t>(ret);
        e.used = true;
        ++entity_count_;
        return 0;
    }

    int32_t do_record_effect(const char* callback_id, ::nros::CallbackEffectKind kind,
                             const char* entity_id) {
        Entity* e = find_entity(entity_id);
        if (e == nullptr) return static_cast<int32_t>(::nros::ErrorCode::InvalidArgument);
        switch (kind) {
        case ::nros::CallbackEffectKind::Publishes: {
            // Bind the publisher to the timer that drives `callback_id`.
            Entity* timer = find_timer_for_callback(callback_id);
            uint32_t period = (timer != nullptr && timer->period_ms > 0) ? timer->period_ms : 0;
            e->publish_period_ms = (period > 0) ? static_cast<int32_t>(period) : -1;
            break;
        }
        case ::nros::CallbackEffectKind::Reads:
            e->reads = true;
            break;
        case ::nros::CallbackEffectKind::Writes:
        default:
            break;
        }
        return 0;
    }

    void fire_publisher(Entity& e) {
        // Synthesized v1 body — see the class doc. Only std_msgs/Int32 has
        // a known trivial CDR encoding the type-erased runtime can emit.
        if (!entry_str_contains(e.type_name, "Int32")) return;
        uint8_t buf[8];
        buf[0] = 0x00; // CDR encapsulation header — CDR_LE (matches nros-c cdr.rs)
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = 0x00;
        const int32_t v = e.counter++;
        buf[4] = static_cast<uint8_t>(v & 0xff);
        buf[5] = static_cast<uint8_t>((v >> 8) & 0xff);
        buf[6] = static_cast<uint8_t>((v >> 16) & 0xff);
        buf[7] = static_cast<uint8_t>((v >> 24) & 0xff);
        nros_cpp_publish_raw(e.storage, buf, sizeof(buf));
    }

    NodeSlot nodes_[NROS_ENTRY_MAX_NODES];
    Entity entities_[NROS_ENTRY_MAX_ENTITIES];
    size_t node_count_ = 0;
    size_t entity_count_ = 0;
};

/// Source-compat alias for the Phase 235.A name. The runtime is shared
/// by every Board adapter now (235.B), so the lifecycle-agnostic
/// `EntryNodeRuntime` is the canonical spelling.
using NativeNodeRuntime = EntryNodeRuntime;

/// The single real `NodeContextOps` table — identical for every Board
/// adapter (native + embedded). Replaces the Phase 219.B recording no-op
/// set. Function-local `static const` of a constant-initializable POD, so
/// no guarded-init runs (safe on NuttX/Zephyr — see `Node`'s storage note).
inline const ::nros::NodeContextOps& entry_node_context_ops() {
    static const ::nros::NodeContextOps ops = {
        /* create_node              */ &EntryNodeRuntime::op_create_node,
        /* create_entity            */ &EntryNodeRuntime::op_create_entity,
        /* record_callback_effect   */ &EntryNodeRuntime::op_record_callback_effect,
    };
    return ops;
}

/// Shared boot step (235.B): reset the runtime, install the real ops, and
/// run the generated register sequence once. Factored out of
/// `NativeBoard::run` so the embedded `ZephyrBoard` reuses the *exact*
/// create_node/create_entity/callback machinery — only the surrounding
/// lifecycle (init / network-wait / spin / shutdown) is per-board.
///
/// Returns `register_fn`'s code (0 on success); the caller owns
/// `nros::shutdown()` on a non-zero result.
template <typename Lambda> int32_t entry_register(EntryNodeRuntime& runtime, Lambda&& register_fn) {
    runtime.reset();
    ::nros::NodeContext context(&runtime, &entry_node_context_ops());
    return register_fn(&context);
}

/// Process-scope, COMDAT-folded arena storage, shared by every Board
/// adapter (one binary links exactly one board, so one arena). Template-
/// static-member (vs a function-local static) keeps it out of the
/// guarded-init path the `Node::GlobalStorageHolder` comment flags on
/// NuttX, and yields a single `.bss` allocation across every including TU.
template <int = 0> struct EntryRuntimeHolder {
    static EntryNodeRuntime runtime;
};
template <int N> EntryNodeRuntime EntryRuntimeHolder<N>::runtime;

} // namespace detail

// Phase 235.B — weak network-readiness hook for embedded Board adapters.
//
// Default: no-op. The canonical in-tree Zephyr path auto-brings-up
// networking at boot (`CONFIG_NET_CONFIG_AUTO_INIT` — static IP / DHCP),
// so `ZephyrBoard::run` needs no explicit wait. A board crate or Entry app
// that must block until the link / DHCP lease is ready (e.g. ASI's
// `configure_network()` prologue) provides a STRONG definition of this
// symbol, which the linker prefers over the weak default. Mirrors the
// weak-default discipline already used for `nros_app_register_backends`.
extern "C" {
#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak)) void nros_board_network_wait(void) {}
#else
void nros_board_network_wait(void);
#endif
}

/// Phase 219.B / 235.A — Native (POSIX) board adapter for Entry-pkg
/// generated TUs.
///
/// Owns the init/spin/shutdown ritual; the generated TU supplies the
/// per-Node register sequence as a `NodeRegisterFn`-shaped lambda.
///
/// Phase 235.A replaced the recording no-op NodeContext with the real
/// `detail::EntryNodeRuntime` — `run()` now constructs live
/// publishers/subscriptions and drives them in a poll loop, so a
/// generated C++ `main()` boots a working pub/sub topology on native.
/// Phase 235.B factored the ops + arena into `detail::` so the embedded
/// `ZephyrBoard` shares them verbatim (only the lifecycle differs).
class NativeBoard {
  public:
    /// Run the Entry-pkg lifecycle. `register_fn` is invoked once,
    /// before the spin loop, with a NodeContext backed by the live
    /// `EntryNodeRuntime`.
    ///
    /// Returns the first non-zero code from `register_fn` or the spin
    /// loop. `0` on graceful shutdown.
    template <typename Lambda> static int32_t run(Lambda&& register_fn) {
        // Native init resolves locator + domain from $NROS_LOCATOR /
        // $ROS_DOMAIN_ID at runtime (the host exception to the embedded
        // compile-time domain-id rule — see CLAUDE.md / `ZephyrBoard`).
        nros::Result r = nros::init();
        if (!r.ok()) {
            return static_cast<int32_t>(r.raw());
        }

        detail::EntryNodeRuntime& runtime = detail::EntryRuntimeHolder<>::runtime;
        int32_t rc = detail::entry_register(runtime, register_fn);
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }

        nros::Result spin_r = runtime.spin();
        nros::shutdown();
        return static_cast<int32_t>(spin_r.raw());
    }
};

/// Phase 235.B — embedded (Zephyr) board adapter, the `Board::run()`
/// sibling to `NativeBoard` (RFC-0032 §8a).
///
/// **Board granularity decision (RFC-0032 §8a open item).** ONE
/// metadata-driven `ZephyrBoard`, not per-board types (`FvpAemv8rBoard`,
/// …). Everything that varies per board — the Zephyr `BOARD` id, the DTS
/// overlay, the default RMW, the `west` runner — is already supplied by
/// the Phase 215 `nano_ros_use_board(<name>)` cmake import + Kconfig at
/// *build* time, so the C++ adapter has nothing board-specific left to
/// specialize at the source level. A per-board C++ type would only
/// duplicate this lifecycle with no behavioural difference. (RFC-0032
/// already leaned this way: "single + metadata-driven".)
///
/// Lifecycle (mirrors ASI `actuation_module/src/main.cpp`):
///   `nros::init(domain) → network-wait → register_fn → spin → shutdown`.
///
/// The runtime ops + arena are the **same** `detail::EntryNodeRuntime`
/// machinery `NativeBoard` uses — only the lifecycle differs:
///   * domain id is **compile-time** (Kconfig `CONFIG_NROS_*_DOMAIN_ID`),
///     never a runtime env (CLAUDE.md embedded domain-id rule);
///   * a `nros_board_network_wait()` hook runs before init so a board /
///     app can block for DHCP / link-up (default no-op — Zephyr
///     auto-brings-up networking);
///   * the spin loop yields cooperatively each tick (`entry_tick_yield`
///     → `k_yield()`).
class ZephyrBoard {
  public:
    /// Compile-time domain id (CLAUDE.md embedded rule — NOT a runtime
    /// env). Cyclone keys off `CONFIG_NROS_CYCLONE_DOMAIN_ID` when present
    /// (matches ASI), else the generic `CONFIG_NROS_DOMAIN_ID`. Override
    /// by defining `NROS_ENTRY_DOMAIN_ID` before including this header.
#ifndef NROS_ENTRY_DOMAIN_ID
#if defined(NROS_RMW_CYCLONEDDS) && defined(CONFIG_NROS_CYCLONE_DOMAIN_ID)
#define NROS_ENTRY_DOMAIN_ID CONFIG_NROS_CYCLONE_DOMAIN_ID
#elif defined(CONFIG_NROS_DOMAIN_ID)
#define NROS_ENTRY_DOMAIN_ID CONFIG_NROS_DOMAIN_ID
#else
#define NROS_ENTRY_DOMAIN_ID 0
#endif
#endif

    /// Run the Entry-pkg lifecycle on a Zephyr board. Same shape +
    /// contract as `NativeBoard::run`, with the embedded lifecycle.
    template <typename Lambda> static int32_t run(Lambda&& register_fn) {
        // Block for network readiness BEFORE init so the RMW backend has a
        // routable interface to bind (weak no-op by default).
        nros_board_network_wait();

        // Compile-time domain id + default locator ("" → backend discovery
        // default, as the in-tree FVP Cyclone example uses).
        nros::Result r = nros::init("", static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID));
        if (!r.ok()) {
            return static_cast<int32_t>(r.raw());
        }

        detail::EntryNodeRuntime& runtime = detail::EntryRuntimeHolder<>::runtime;
        int32_t rc = detail::entry_register(runtime, register_fn);
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }

        nros::Result spin_r = runtime.spin();
        nros::shutdown();
        return static_cast<int32_t>(spin_r.raw());
    }
};

/// Phase 238 — embedded NuttX board adapter, sibling to `ZephyrBoard`.
///
/// NuttX brings up `eth0` (virtio-net) during kernel boot **before** the
/// app entry runs (see `nros-board-nuttx-qemu-arm::entry_212n` —
/// "NuttX brings up eth0 during kernel boot before main"), so — like the
/// Zephyr `CONFIG_NET_CONFIG_AUTO_INIT` path — no explicit network wait is
/// needed; the weak `nros_board_network_wait()` default no-op is correct.
///
/// The runtime ops + arena are the **same** `detail::EntryNodeRuntime`
/// machinery `NativeBoard` / `ZephyrBoard` use — only the lifecycle differs:
///   * domain id is **compile-time** (`NROS_ENTRY_DOMAIN_ID`, fed from the
///     example's `nano_ros_deploy(DOMAIN_ID …)` → `CONFIG_NROS_DOMAIN_ID`),
///     never a runtime env (CLAUDE.md embedded domain-id rule);
///   * the cooperative `entry_tick_yield()` is a no-op on NuttX (the
///     preemptive scheduler + `spin_once`'s `z_sleep_ms` pacing release the
///     CPU to the zenoh-pico read/lease tasks — see CLAUDE.md
///     `zpico_spin_once` note).
///
/// The bootable ELF *is* the NuttX kernel: the generated entry TU is
/// compiled as `APP_MAIN_CPP` and linked into the kernel by the cargo
/// `nros-nuttx-ffi` build (`nuttx_ffi_build.rs`), driven from the carrier
/// cmake (`nano_ros_node_register` NuttX branch → `nros_platform_link_app`).
// Phase 238 — compile-time default connect locator for the locator-less
// `NuttxBoard::run(lambda)` overload. The carrier normally bakes the real
// locator into the generated entry TU and calls the 2-arg overload; this
// default only applies if a hand-written entry uses the 1-arg form.
#ifndef NROS_ENTRY_LOCATOR
#define NROS_ENTRY_LOCATOR ""
#endif

class NuttxBoard {
  public:
    /// Run the Entry-pkg lifecycle on a NuttX board with an explicit
    /// connect `locator`. The bootable-ELF carrier
    /// (`nano_ros_node_register` NuttX branch) bakes the locator into the
    /// generated entry TU (`configure_file` of
    /// `cmake/templates/nuttx_entry_main.cpp.in`) because — unlike Zephyr's
    /// `CONFIG_NET_CONFIG_AUTO_INIT` peer discovery — the QEMU slirp guest
    /// must dial the host zenoh router explicitly (`tcp/10.0.2.2:<port>`),
    /// mirroring the Rust `*_entry` pkg's `[…entry] locator = …` bake.
    /// `locator == ""` falls back to backend discovery.
    template <typename Lambda> static int32_t run(const char* locator, Lambda&& register_fn) {
        // Network is up at kernel boot; the weak hook stays a no-op unless a
        // board/app provides a strong override (mirrors ZephyrBoard).
        nros_board_network_wait();

#ifdef NROS_NUTTX_ENTRY_DEBUG
        ::std::printf("[nuttx-cpp] run: locator=%s domain=%d\n", locator,
                      (int)NROS_ENTRY_DOMAIN_ID);
#endif
        // Compile-time domain id. `NROS_ENTRY_DOMAIN_ID` resolves from
        // `CONFIG_NROS_DOMAIN_ID` (else 0) — same macro ZephyrBoard uses.
        nros::Result r =
            nros::init(locator, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID));
#ifdef NROS_NUTTX_ENTRY_DEBUG
        ::std::printf("[nuttx-cpp] init -> %d\n", (int)r.raw());
#endif
        if (!r.ok()) {
            return static_cast<int32_t>(r.raw());
        }

        detail::EntryNodeRuntime& runtime = detail::EntryRuntimeHolder<>::runtime;
        int32_t rc = detail::entry_register(runtime, register_fn);
#ifdef NROS_NUTTX_ENTRY_DEBUG
        ::std::printf("[nuttx-cpp] register -> %d; spinning\n", (int)rc);
#endif
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }

        nros::Result spin_r = runtime.spin();
#ifdef NROS_NUTTX_ENTRY_DEBUG
        ::std::printf("[nuttx-cpp] spin exit -> %d\n", (int)spin_r.raw());
#endif
        nros::shutdown();
        return static_cast<int32_t>(spin_r.raw());
    }

    /// Locator-less overload — uses the compile-time `NROS_ENTRY_LOCATOR`
    /// (default `""`, i.e. backend discovery).
    template <typename Lambda> static int32_t run(Lambda&& register_fn) {
        return run(NROS_ENTRY_LOCATOR, static_cast<Lambda&&>(register_fn));
    }
};

} // namespace board
} // namespace nros

// Phase 219.E — `NROS_MAIN(<Board>, "<launch_spec>")` declarative
// marker. Expands to a sentinel TU-local symbol so the cmake fn can
// detect macro presence (via `target_compile_definitions` or the
// compiled-out-but-still-elf-visible `__nros_entry_macro_present`
// symbol). The Phase 219.D cmake fn body owns the actual codegen —
// the macro is doc/IDE shape only.
//
// Usage:
//
//   #include <nros/main.hpp>
//   NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:system.launch.xml")
//
// Putting it in a user TU is OPTIONAL. The generated TU (emitted by
// `nano_ros_entry(LAUNCH …)`) carries the canonical `int main()` body
// regardless; the macro is purely a hint for tooling and IDEs that
// parse the source.
#define NROS_MAIN(BoardType, LaunchSpec)                                                           \
    extern "C" const unsigned char __nros_entry_macro_present = 1;                                 \
    static_assert(sizeof(LaunchSpec) > 1, "NROS_MAIN: launch spec must be a non-empty literal")

#endif // NROS_CPP_MAIN_HPP
