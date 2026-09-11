// nros-cpp: lightweight logging macros
// Freestanding C++ — no STL, opt-in to stdio via NROS_CPP_STD or
// hosted-build detection.

/**
 * @file log.hpp
 * @ingroup grp_misc
 * @brief Phase 123.B.7 — `NROS_INFO` / `NROS_WARN` / `NROS_ERROR` /
 *        `NROS_DEBUG` printf-style log macros.
 *
 * Routes through a single configurable sink. By default, on hosted
 * builds (`__STDC_HOSTED__` or `NROS_CPP_STD` defined) the sink
 * writes to `stderr` with a `[level] file:line — fmt…` prefix.
 * Embedded builds without stdio fall through to a no-op so the
 * macros compile away.
 *
 * Override the sink with `#define NROS_LOG_SINK(level, file, line, fmt, ...)`
 * before including this header (or via `-DNROS_LOG_SINK=…`) to
 * route through `defmt`, semihosting, Zephyr's `LOG_INF`, etc.
 *
 * The macros take a `printf`-style format string + variadic
 * arguments. They evaluate `fmt` and the variadics exactly once.
 */

#ifndef NROS_CPP_LOG_HPP
#define NROS_CPP_LOG_HPP

#ifndef NROS_LOG_SINK
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio>
#define NROS_LOG_SINK(level, file, line, ...)                                                      \
    do {                                                                                           \
        ::std::fprintf(stderr, "[" level "] %s:%d ", (file), (line));                              \
        ::std::fprintf(stderr, __VA_ARGS__);                                                       \
        ::std::fputc('\n', stderr);                                                                \
    } while (0)
#else
#define NROS_LOG_SINK(level, file, line, ...) ((void)(level), (void)(file), (void)(line))
#endif
#endif

/// Print an INFO-level log line.
#define NROS_INFO(...) NROS_LOG_SINK("INFO", __FILE__, __LINE__, __VA_ARGS__)
/// Print a WARN-level log line.
#define NROS_WARN(...) NROS_LOG_SINK("WARN", __FILE__, __LINE__, __VA_ARGS__)
/// Print an ERROR-level log line.
#define NROS_ERROR(...) NROS_LOG_SINK("ERROR", __FILE__, __LINE__, __VA_ARGS__)
/// Print a DEBUG-level log line. Compiled out when `NDEBUG` is set.
#ifdef NDEBUG
#define NROS_DEBUG(...) ((void)0)
#else
#define NROS_DEBUG(...) NROS_LOG_SINK("DEBUG", __FILE__, __LINE__, __VA_ARGS__)
#endif

/* ---- Phase 88.12 — node-/logger-keyed surface ----
 *
 * The macros above are legacy (Phase 123.B.7) — file:line-prefixed
 * stderr printf with no per-logger routing. The macros below carry
 * a Logger handle through to the post-Phase-88 dispatcher
 * (`nros_log_emit_fmt` → per-platform sinks, see
 * `<nros/platform.h>` for the ABI).
 *
 * Obtain the handle from a Node via `node.get_logger()`:
 *
 * ```cpp
 * rclcpp::Node node;
 * NROS_TRY(nros::create_node(node, "my_node"));
 * auto logger = node.get_logger();
 * NROS_LOG_INFO(logger, "started; domain=%u", 42);
 * ```
 *
 * Below-threshold filtering happens runtime-side via the
 * `nros_log::Logger`'s `set_level`; compile-time filtering is via
 * `nros-log/max-level-*` Cargo features (compiled into the nros-c
 * staticlib that ships `nros_log_emit_fmt`). */

/* `<nros/log.h>` already defines the six Phase-88 macros
 * (`NROS_LOG_TRACE` … `NROS_LOG_FATAL`) — include it here so C++
 * call sites pick up the same definitions without re-emitting
 * them (would trigger -Wmacro-redefined on identical-but-redeclared
 * macros). */
#include <nros/log.h>

// ============================================================================
// rclcpp:: — the ROS 2 spelling (RFC-0089 stage 6, step A)
// ============================================================================
//
// These names used to live in `nros/rclcpp_compat.hpp`, a separate source-compat
// shim a ported file had to be force-included with. RFC-0089 §"Naming: replace,
// with alias as the migration step" makes the ROS 2 spelling a FIRST-CLASS name
// declared by the API headers themselves, at which point the shim has nothing
// left to bridge and dissolves by construction. `nros::` is untouched: both
// spellings work, and deprecating one is step B.
//
// This header carries the REFUSAL VOCABULARY as well as the logging surface,
// and the two belong together for a reason RFC-0089 states: a REFUSE-LOUD name
// EMITS NO CODE — it is a diagnostic, and this is nano-ros's diagnostic header.
// It is also the only header the umbrella pulls before everything else while
// including nothing of ours, so `qos.hpp`, `options.hpp`, `service.hpp` and the
// umbrella can all reach one definition of `refuse` without a cycle and without
// a second spelling of "dependent false".

// `<string>` for `rclcpp::get_logger(const std::string&)` and `<sstream>` for
// the `_STREAM` family. Gated: this header is reachable from a `no_std` C++
// build with a minimal libcpp, where neither exists — and from a
// `-ffreestanding` build against a FULL libstdc++, which has both files and
// refuses to be included from either. When they are absent the names below
// simply do not exist, which is what a freestanding target should see; the
// un-gated half (the refusal vocabulary, `Logger`, the printf-style macros)
// still reaches it.
//
// Worth knowing where this sits: `qos.hpp` includes this header and every
// entity header includes `qos.hpp`, so the `_STREAM` family's `<sstream>` is on
// the transitive include path of every freestanding TU in the API.
#include "nros/std_detect.hpp"

namespace rclcpp {

// --- REFUSE-LOUD infrastructure (RFC-0089 stage 3) ---------------------------
//
// RFC-0089's rule:
//
//   An upstream name may be adopted only if its observable contract is the
//   same, or strictly weaker in a documented, non-inverting way. A contract
//   that inverts, or silently drops data or configuration, must fail to
//   COMPILE. Never compile and differ.
//
// A refusal is per-CONCEPT, not per-symbol — the ten inert `NodeOptions`
// accessors share ONE message. The name EXISTS rather than being absent,
// because `no member named 'use_intra_process_comms'` is honest and teaches
// nothing, while a diagnostic that names the constraint AND the nano-ros
// alternative is the migration, delivered at the point of failure.
//
// Why a dependent `static_assert` and not `= delete`: these headers are parsed
// as C++14 (`just check cpp` compiles them and their probes with `-std=c++14`),
// where a deleted function carries NO message — `= delete("reason")` is C++26.
// A `static_assert` inside a template body fires on INSTANTIATION, so the name
// stays declarable and only USING it fails, with the full text attached.
namespace detail {

/// Dependent `false`. `static_assert(refuse<T>::value, …)` in a template body
/// is ill-formed only once that template is instantiated — which is exactly
/// "the name exists, calling it fails".
///
/// ONE definition, reached by every refusal in the C++ API: `qos.hpp`'s
/// `SystemDefaultsQoS`, `options.hpp`'s `NodeOptions` accessors, the
/// `throttle_is_refused` below, and the shared_ptr service/client callback
/// overloads on `rclcpp::Node` in `nros.hpp`.
template <typename T> struct refuse {
    static const bool value = false;
};

} // namespace detail

} // namespace rclcpp

// The diagnostics are macros so one literal backs every site that shares a
// concept (C++14 `static_assert` takes a string LITERAL, not a constexpr
// variable, so a `constexpr const char*` cannot be used here). They live
// together because they are one vocabulary; each is USED in the header that
// declares the name it refuses.
//
// Every one of them contains the marker `REFUSED by nano-ros`, which is what
// `just check cpp`'s expected-failure lane greps for: an expected-failure
// compile cannot tell "the refusal fired" from "the file is not there", so the
// exit code alone proves nothing.

#define NROS_RCLCPP_REFUSE_NODE_OPTIONS                                                            \
    "rclcpp::NodeOptions' option setters and getters are REFUSED by nano-ros "                     \
    "(RFC-0089, phase-417 W3.f). Each one used to store its argument in a private field that "     \
    "NOTHING read and return *this, so the idiomatic chained call compiled and configured "        \
    "nothing -- a silent drop of configuration, which the compile-or-conform rule requires to "    \
    "fail to compile instead. nano-ros resolves parameters and remaps in the LAUNCHER "            \
    "(`nros launch` / play_launch, RFC-0060) and projects them into the process environment "      \
    "before exec; it has no runtime ComponentManager, no intra-process transport, no topic "       \
    "statistics collector and no /rosout topic, so there is nothing for these knobs to switch. "   \
    "Use: node->declare_parameter<T>(name, default) for parameter overrides; the launch file "     \
    "for remaps; and drop the option chain. `rclcpp::NodeOptions{}` itself still constructs, so "  \
    "the `::rclcpp::Node(name, options)` constructor shape a composable node needs keeps "         \
    "compiling."

#define NROS_RCLCPP_REFUSE_INIT_ARGV                                                               \
    "rclcpp::init(argc, argv) was given --ros-args, which nano-ros cannot honour "                 \
    "(RFC-0089, phase-417 W3.b). Proceeding would DISCARD it, so `-r chatter:=/other` would "      \
    "silently become a wrong-topic bug at runtime -- the 'compiles and differs' the rule "         \
    "forbids. Nothing in this process parses --ros-args yet: nros::init_with_launch_auto(argc, "   \
    "argv) discards them too (node.hpp:1025-1027), and honouring them is remap resolution -- "     \
    "RFC-0020 violation class 4 -- so the parser belongs beside nros::resolve_name, not in this "  \
    "header. Today remaps and parameter overrides come from the LAUNCHER, which projects them "    \
    "into the environment before exec. Call the zero-argument rclcpp::init(), or "                 \
    "nros::init_with_launch_auto(0, nullptr, \"my_session\") for the launch-aware entry point."

// phase-428 W5 finding 9. Runtime, like `NROS_RCLCPP_REFUSE_INIT_ARGV` above
// and for the same reason: only the VALUE carries the defect, so the earliest
// point loudness is available is the call. Prose tail only — the verb, the
// name and the code are printed by `rclcpp::detail::abort_failed_create`.
#define NROS_RCLCPP_ABORT_FAILED_CREATE                                                            \
    "Upstream rclcpp THROWS here, so control never continues past a failed create; nano-ros has "  \
    "no exceptions (RFC-0018) and the verb returns a shared_ptr, which left two ways to differ. "  \
    "It used to discard the Result and hand back a NON-NULL pointer to a dead entity: publish "    \
    "went nowhere, and rclcpp::spin(node) returned at once because the node was never "            \
    "initialized, so init -> create -> spin -> shutdown ran to completion and exited 0. "          \
    "Returning "                                                                                   \
    "null instead would be quieter still -- a nullptr dereference with no message, and entirely "  \
    "inert for create_wall_timer, whose result a ported node stores and never dereferences. "      \
    "RFC-0089's rule is that a difference the compiler cannot point at must be made loud by "      \
    "other means, so this aborts. To HANDLE the failure rather than die on it, drop to the "       \
    "underlying nros API, where every create verb RETURNS nros::Result into caller-owned "         \
    "storage: nros::create_node(node, name), node.create_publisher(pub, topic, qos), "             \
    "node.create_subscription(sub, topic, qos, cb), node.create_service<S>(srv, name, cb), "       \
    "node.create_client<S>(cli, name), node.create_timer(t, period_ms, cb, ctx)."

#define NROS_RCLCPP_REFUSE_SYSTEM_DEFAULTS_QOS                                                     \
    "rclcpp::SystemDefaultsQoS is REFUSED by nano-ros (RFC-0089 W3.f, issue 0829). Upstream's "    \
    "rmw_qos_profile_system_default names NO concrete policy: every field is a sentinel meaning "  \
    "'let the RMW decide', and the two reference RMWs resolve the depth sentinel differently "     \
    "(rmw_cyclonedds_cpp -> KEEP_LAST 1, rmw_zenoh_cpp -> 42). nros::QoS has no sentinel, "        \
    "deliberately: the backend is linked at build time, so there is no middleware to defer to. "   \
    "Any value this could return would be a concrete profile wearing the name of an absent one, "  \
    "and it used to return QoS(10) -- which is rmw_qos_profile_DEFAULT, a different upstream "     \
    "profile. Name the policy you want: rclcpp::QoS(10) for the ROS default, "                     \
    "rclcpp::SensorDataQoS(), rclcpp::ServicesQoS(), or nros::QoS().best_effort().keep_last(1)."

#define NROS_RCLCPP_REFUSE_THROTTLE                                                                \
    "RCLCPP_*_THROTTLE is REFUSED by nano-ros (RFC-0089 W3.a, issue 1019). It expanded to the "    \
    "plain RCLCPP_* macro with `clock` and the period left UNEVALUATED, so a 1 Hz throttle "       \
    "logged at loop rate and a side-effecting clock expression was dropped entirely. There is no " \
    "throttle on the C or C++ logging path; nros-log has one Rust-side and re-exporting it is "    \
    "phase-417 W4.d, so a throttle written here would be a second implementation of behaviour "    \
    "Rust already owns (RFC-0019). Rate-limit at the call site, or use the un-throttled "          \
    "RCLCPP_INFO / RCLCPP_WARN / RCLCPP_ERROR."

// phase-417 stage 3 (W3.a). ONE concept: "spin until work arrives, with no
// budget". It backs BOTH halves of `Executor::spin_once`'s loudness, because
// both are the same request written two ways — the no-argument form (upstream's
// default IS -1) and an explicit negative timeout. The first is knowable from
// the SIGNATURE, so it is a `static_assert`; the second only from the VALUE, so
// it is a loud return at the call (RFC-0089 §"Where the refusal fires").
#define NROS_RCLCPP_REFUSE_UNBOUNDED_SPIN                                                          \
    "an UNBOUNDED rclcpp::Executor::spin_once is REFUSED by nano-ros (RFC-0089 stage 3, "          \
    "phase-417 W3.a). Upstream's spin_once(timeout = -1) BLOCKS INDEFINITELY and executes ONE "    \
    "ready item. nano-ros takes a millisecond budget and returns when it expires. Nothing here "   \
    "can block forever: building that out of this call would be a polling loop inside the "        \
    "wrapper, which is RFC-0020 violation class 2 -- the loop belongs Rust-side, where `spin()` "  \
    "already is. Two silent differences used to live here: the no-argument form substituted a "    \
    "10 ms budget nobody chose, and a -1 was clamped to 0, so the call POLLED where upstream "     \
    "blocks. NAME YOUR BUDGET: spin_once(10) sleeps up to 10 ms and then returns; spin_once(0) "   \
    "is upstream's spin_some -- drain what is ready, never wait; spin() blocks until cancel(). "   \
    "ENVELOPE, unchanged by this refusal: our spin_once DRAINS every ready arena entry "           \
    "(RFC-0002 section 3 computes the ready bitmap once) where upstream executes the next one; "   \
    "that is `cpp:Executor::spin_some`'s open question, not this one."

// The RUNTIME form of the same refusal, for `spin_once(-1)`. SHORT because it
// has to be: `nros_log`'s format buffer drops a body that does not fit rather
// than truncating it (see `rclcpp::detail::RUNTIME_REFUSAL_MAX`, which enforces
// the bound). It still names the constraint and the alternative; the long text
// above is what the compiler prints for the no-argument form.
#define NROS_RCLCPP_REFUSE_UNBOUNDED_SPIN_RUNTIME                                                  \
    "spin_once(negative) REFUSED by nano-ros: no unbounded spin (RFC-0089/RFC-0020). Use "         \
    "spin_once(0) to drain, spin_once(ms) to wait, spin() until cancel()."

// phase-417 stage 3 (W3.a). ONE concept, TWO call sites: `Client::wait_for_service`
// and `rclcpp_action::Client::wait_for_action_server` had the identical defect
// (a 5000 ms default standing in for upstream's -1), so they share the message.
#define NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT                                                          \
    "an UNBOUNDED wait_for_service / wait_for_action_server is REFUSED by nano-ros (RFC-0089 "     \
    "stage 3, phase-417 W3.a). Upstream's default timeout is -1, which means WAIT FOREVER. "       \
    "These helpers drive the executor cooperatively while they probe, and RFC-0021 is why that "   \
    "cannot be unbounded: a wait that never returns starves every other entity on a "              \
    "single-threaded transport. The budget is an unsigned millisecond count, so there is no "      \
    "value to port -1 to either. The no-argument form used to substitute 5000 ms -- a budget "     \
    "the caller did not choose, silently, which is exactly what the compile-or-conform rule "      \
    "forbids. NAME YOUR BUDGET: wait_for_service(10000) / wait_for_action_server(10000). To "      \
    "keep doing other work while you wait, poll Client::service_is_ready() from your own spin "    \
    "loop, or call the action form with a short budget inside a loop you control."

#define NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK                                             \
    "the shared_ptr service-callback shape is REFUSED by nano-ros (RFC-0089, phase-417 W2.c). "    \
    "rclcpp's create_service/create_client callback takes std::shared_ptr<Request> and "           \
    "std::shared_ptr<Response> (plus a request header), which needs a per-request heap "           \
    "allocation on the delivery path. nano-ros has no allocator there (RFC-0022) and hands the "   \
    "request and response BY REFERENCE into caller-owned storage instead, so adopting that "       \
    "signature would mean a second delivery path. Change the handler to "                          \
    "void(const S::Request&, S::Response&) for a service, or void(const S::Response&) for a "      \
    "client -- a plain function pointer or a capture-less lambda -- or take the poll-style "       \
    "overload create_service<S>(name, qos) / create_client<S>(name, qos) and drain it from your "  \
    "spin loop."

namespace rclcpp {

// --- Logger surface ----------------------------------------------------------
//
// `rclcpp::Logger` in upstream is a pull-through to the rcl logger. Here it is
// a name-only sentinel; the log macros below dispatch through NROS_*, which
// already carry the file/line. The logger NAME is lost (nros has no per-logger
// dispatch yet). Documented; a follow-up can teach nros::log a tag.

class Logger {
  public:
    explicit Logger(const char* name = "") : name_(name), handle_(nullptr) {}

    /// phase-427 W5 — the name PLUS the opaque `nros_log::Logger` handle the
    /// `NROS_LOG_*` macros dispatch through. `rclcpp::Node::get_logger()` builds
    /// one of these; `rclcpp::get_logger("free")` leaves the handle null,
    /// because a free-standing name has no node behind it.
    Logger(const char* name, const void* handle) : name_(name), handle_(handle) {}

    const char* get_name() const { return name_; }

    /// Implicit conversion to `nros_logger_t` (`const void*`, `<nros/log.h>`).
    ///
    /// This is what let `get_logger()` become ONE accessor with upstream's
    /// return type without breaking the native call sites. Before the merge
    /// there were two: `rclcpp::Node::get_logger() -> const void*` for
    /// `NROS_LOG_INFO(logger, …)`, and the shim's `-> rclcpp::Logger` for
    /// `RCLCPP_INFO(get_logger(), …)`. Two overloads differing only in return
    /// type are ill-formed, so the ported channel won (RFC-0089 clause 2) and
    /// the native one is reached by conversion — including `logger == nullptr`,
    /// which `examples/native/cpp/logging` writes.
    ///
    /// Null when this logger was built from a name alone, or from an
    /// uninitialized node.
    operator const void*() const { return handle_; }

  private:
    const char* name_;
    const void* handle_;
};

/// `rclcpp::get_logger(name)` — issue 1019, phase-417 W3.a.
///
/// RESOLVES the name now. It used to build a name-only sentinel with a null
/// handle, so `rclcpp::get_logger("planner")` selected nothing and every
/// `RCLCPP_*` call through it shared one threshold with every other logger in
/// the image — issue 1019's third defect. `nros_log_get_logger` (phase-417
/// W4.d, `<nros/log.h>`) is the C surface that made this possible; it is
/// TOTAL, so this is too, and a NULL or over-long name answers the catch-all
/// logger rather than a null handle.
inline Logger get_logger(const char* name) {
    return Logger(name, name != nullptr ? nros_log_get_logger(name) : nros_log_default_logger());
}

#ifdef NROS_CPP_HAS_STD_STRING
/// `std::string`-keyed overload. Present only where `<string>` is — a
/// freestanding target has no `std::string` to take.
inline Logger get_logger(const std::string& name) {
    return get_logger(name.c_str());
}
#endif

namespace detail {

/// The handle the `NROS_LOG_*` dispatcher takes, from whatever a `RCLCPP_*`
/// call site was handed — issue 1019.
///
/// Two overloads rather than one, and the fallback is the point.
/// `nros_log_emit_fmt_at` RETURNS EARLY on a null handle, so routing the
/// `RCLCPP_*` family at it without this would have swapped one silent drop for
/// another: a `Logger` built from a name alone, or from an uninitialised node,
/// carries a null handle. Those records go to the catch-all `nros` logger
/// instead, which is where a record with no owner belongs.
///
/// The `const void*` overload exists because `nros_logger_t` IS `const void*`
/// and a native call site may hand one straight in; `Logger` binds the
/// reference overload by identity, so the two never compete.
inline nros_logger_t log_handle(const void* handle) {
    return handle != nullptr ? handle : nros_log_default_logger();
}

inline nros_logger_t log_handle(const Logger& logger) {
    return log_handle(static_cast<const void*>(logger));
}

/// The longest a RUNTIME refusal may be — phase-417 stage 3, MEASURED.
///
/// `nros_log`'s formatting buffer is 256 bytes by default
/// (`nros_log::format_buffer_capacity`, and `buffer-size-128` makes it 128),
/// and `heapless::String::push_str` is ALL-OR-NOTHING: a body that does not fit
/// is not truncated, it is DROPPED, and the console shows the header plus a
/// lone `…`. So a long runtime refusal is not a shortened refusal, it is an
/// INVISIBLE one — measured on this host at 1050 bytes, which printed
/// `[ERROR] nros: [ts] …` and nothing else.
///
/// 160 leaves room for the `[LEVEL] logger: [timestamp] ` prefix inside the
/// same buffer, with margin for the 128-byte build.
///
/// The long form of each message stays where it works: the `static_assert`
/// text, which the compiler prints in full.
const size_t RUNTIME_REFUSAL_MAX = 160;

/// Say a `NROS_RCLCPP_REFUSE_*_RUNTIME` message at runtime — phase-417 stage 3.
///
/// Takes the literal BY REFERENCE so its length is a compile-time constant, and
/// then refuses at compile time to emit one that would vanish. A runtime
/// refusal nobody can read is the defect this whole stage exists to remove, so
/// it must not be possible to add one by accident.
///
/// `nros_log_emit_at`, not the `NROS_LOG_*` printf path: that one renders
/// through its own 256-byte stack buffer as well, and there is no reason to pay
/// two truncation risks for a message that needs no formatting.
///
/// ERROR rather than FATAL: the call REFUSES and returns, so the process is
/// still alive and still able to do the right thing.
template <size_t N>
inline void say_refused(const char (&message)[N], const char* file, uint32_t line) {
    static_assert(N - 1 <= RUNTIME_REFUSAL_MAX,
                  "a RUNTIME refusal must fit nros_log's format buffer. Longer than that it is "
                  "not truncated, it is DROPPED, and the reader sees a lone ellipsis. Keep the "
                  "long text on the static_assert, which the compiler prints in full, and give "
                  "the runtime site a short form that still names the alternative.");
    nros_log_emit_at(log_handle(nullptr), NROS_LOG_SEVERITY_ERROR, message, N - 1, file, line);
}

/// **REFUSED** — the target of every `RCLCPP_*_THROTTLE` macro. Variadic so
/// the macro can forward `logger`, `clock`, the period and the whole format
/// pack, which means the arguments are still parsed and type-checked; only the
/// `static_assert` stops the build, with the migration attached.
template <typename Logger, typename Clock, typename Period, typename... Rest>
void throttle_is_refused(Logger&&, Clock&&, Period&&, Rest&&...) {
    static_assert(refuse<Logger>::value, NROS_RCLCPP_REFUSE_THROTTLE);
}

} // namespace detail

} // namespace rclcpp

// --- Log macros --------------------------------------------------------------
//
// Same call shape as rclcpp.
//
// THE LOGGER IS CARRIED, NOT DISCARDED (issue 1019, phase-417 W3.a). The family
// used to expand to `(void)(logger); NROS_<LEVEL>(__VA_ARGS__)`, and that one
// line was three defects at once:
//
//   1. `NROS_INFO` is the legacy printf family, whose sink is `fprintf(stderr)`
//      on a hosted build and a NO-OP on a freestanding one — so on Zephyr,
//      FreeRTOS, NuttX and ThreadX, the targets nano-ros exists for, a ported
//      node's entire log output was compiled away with no diagnostic. It worked
//      on the host, which is where anyone would test the port.
//   2. the logger was cast to `void`, so per-logger levels applied to none of
//      the family and `rclcpp::get_logger("planner")` selected nothing.
//   3. `RCLCPP_FATAL` lowered to `NROS_ERROR`, because the legacy family has no
//      fatal level at all — a fatal line survived an ERROR-threshold filter
//      upstream would have treated as a different severity.
//
// All three are one fix: route at `NROS_LOG_*` (`<nros/log.h>`), which
// dispatches through `nros_log` and therefore reaches `LOG_ERR`/`printk` on
// exactly the targets where the legacy sink is a no-op, honours the per-logger
// threshold, and has a distinct `NROS_LOG_SEVERITY_FATAL`.
//
// `NROS_INFO` and friends are UNCHANGED and still the right thing for a board's
// own console print: the hosted/freestanding split is a feature there. What was
// wrong was routing `RCLCPP_*` through it — a ported node calling `RCLCPP_INFO`
// is asking for the ROS logger, not for a board console.
//
// One consequence worth naming: `RCLCPP_DEBUG` no longer compiles out under
// `NDEBUG`. It is runtime-filtered by the logger's threshold now, which is what
// upstream does.
//
// `_STREAM` no longer discards its message (issue 1019). It used to expand to
// `RCLCPP_INFO(logger, "%s", "")` with `args` NEVER REFERENCED, which is the
// worst outcome available: the call compiled, the level was right, the file and
// line were right, and the text was gone. It now formats through
// `std::ostringstream` and hands the result to the same sink — a string
// conversion that copies and calls through, which RFC-0089 §"Who implements an
// adopted name" allows in the wrapper. Since the move into this header
// `<sstream>` is GATED rather than unconditional, so the `_STREAM` family is
// declared only where the standard library that backs it exists.
//
// `_THROTTLE` is REFUSE-LOUD. See `NROS_RCLCPP_REFUSE_THROTTLE`.

// NROS_INFO is a do-while(0) block; the comma-operator wrapper around it was
// invalid C++. Use a do-while wrapper so RCLCPP_INFO is a single statement.
//
// The whole family sits behind `#ifndef RCLCPP_INFO` so a translation unit that
// somehow also has real rclcpp keeps rclcpp's own definitions.
#ifndef RCLCPP_INFO
#define RCLCPP_DEBUG(logger, ...)                                                                  \
    do {                                                                                           \
        NROS_LOG_DEBUG(::rclcpp::detail::log_handle(logger), __VA_ARGS__);                         \
    } while (0)
#define RCLCPP_INFO(logger, ...)                                                                   \
    do {                                                                                           \
        NROS_LOG_INFO(::rclcpp::detail::log_handle(logger), __VA_ARGS__);                          \
    } while (0)
#define RCLCPP_WARN(logger, ...)                                                                   \
    do {                                                                                           \
        NROS_LOG_WARN(::rclcpp::detail::log_handle(logger), __VA_ARGS__);                          \
    } while (0)
#define RCLCPP_ERROR(logger, ...)                                                                  \
    do {                                                                                           \
        NROS_LOG_ERROR(::rclcpp::detail::log_handle(logger), __VA_ARGS__);                         \
    } while (0)
#define RCLCPP_FATAL(logger, ...)                                                                  \
    do {                                                                                           \
        NROS_LOG_FATAL(::rclcpp::detail::log_handle(logger), __VA_ARGS__);                         \
    } while (0)

// REFUSED. The arguments are still forwarded so they are parsed and
// type-checked — a refusal should not also hide a typo in the format pack.
#define RCLCPP_DEBUG_THROTTLE(logger, clock, period_ms, ...)                                       \
    ::rclcpp::detail::throttle_is_refused((logger), (clock), (period_ms), __VA_ARGS__)
#define RCLCPP_INFO_THROTTLE(logger, clock, period_ms, ...)                                        \
    ::rclcpp::detail::throttle_is_refused((logger), (clock), (period_ms), __VA_ARGS__)
#define RCLCPP_WARN_THROTTLE(logger, clock, period_ms, ...)                                        \
    ::rclcpp::detail::throttle_is_refused((logger), (clock), (period_ms), __VA_ARGS__)
#define RCLCPP_ERROR_THROTTLE(logger, clock, period_ms, ...)                                       \
    ::rclcpp::detail::throttle_is_refused((logger), (clock), (period_ms), __VA_ARGS__)
#define RCLCPP_FATAL_THROTTLE(logger, clock, period_ms, ...)                                       \
    ::rclcpp::detail::throttle_is_refused((logger), (clock), (period_ms), __VA_ARGS__)
#endif // RCLCPP_INFO

/// Emit a refusal message, with the call site — phase-417 stage 3.
///
/// The RUNTIME half of REFUSE-LOUD, for the cases where only the VALUE carries
/// the defect and a `static_assert` therefore cannot reach it (RFC-0089
/// §"Where the refusal fires"). The argument must be a string LITERAL, which
/// every `NROS_RCLCPP_REFUSE_*_RUNTIME` is, and must fit
/// `rclcpp::detail::RUNTIME_REFUSAL_MAX` — enforced there, not here.
#define NROS_RCLCPP_SAY_REFUSED(msg)                                                               \
    ::rclcpp::detail::say_refused((msg), __FILE__, (uint32_t)__LINE__)

// The stream family, carrying its message. `NROS_RCLCPP_STREAM_` builds the
// text once and forwards it as a single `%s` argument, so a `%` inside the
// user's text can never be read as a conversion.
#if defined(NROS_CPP_HAS_STD_SSTREAM) && !defined(RCLCPP_INFO_STREAM)
#define NROS_RCLCPP_STREAM_(macro, logger, ...)                                                    \
    do {                                                                                           \
        ::std::ostringstream nros_rclcpp_stream_;                                                  \
        nros_rclcpp_stream_ << __VA_ARGS__;                                                        \
        macro(logger, "%s", nros_rclcpp_stream_.str().c_str());                                    \
    } while (0)

#define RCLCPP_DEBUG_STREAM(logger, ...) NROS_RCLCPP_STREAM_(RCLCPP_DEBUG, logger, __VA_ARGS__)
#define RCLCPP_INFO_STREAM(logger, ...) NROS_RCLCPP_STREAM_(RCLCPP_INFO, logger, __VA_ARGS__)
#define RCLCPP_WARN_STREAM(logger, ...) NROS_RCLCPP_STREAM_(RCLCPP_WARN, logger, __VA_ARGS__)
#define RCLCPP_ERROR_STREAM(logger, ...) NROS_RCLCPP_STREAM_(RCLCPP_ERROR, logger, __VA_ARGS__)
#define RCLCPP_FATAL_STREAM(logger, ...) NROS_RCLCPP_STREAM_(RCLCPP_FATAL, logger, __VA_ARGS__)
#endif // NROS_CPP_HAS_STD_SSTREAM && !RCLCPP_INFO_STREAM

#endif // NROS_CPP_LOG_HPP
