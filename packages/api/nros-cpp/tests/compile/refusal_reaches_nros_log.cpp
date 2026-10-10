// Issue 1303 — on a FREESTANDING target, both runtime refusals reach `nros_log`,
// the only sink there is on an RTOS (it lands on `printk` / `LOG_ERR`).
//
// A guard for the ROUTE, not for the message length, which is the half of 1303
// that was actually broken: since issue 1576 the freestanding `NROS_ERROR` already
// reached `nros_log`, so this check passes on the pre-1303 headers too
// (measured). It exists so that a refusal re-routed to a hosted-only `fprintf`,
// or to the `NROS_LOG_SINK_DISCARD` no-op, fails here rather than going silent
// on every RTOS. The length is held by `failed_create_aborts.cpp`, through
// `nros_log_stderr_sink.hpp`.
//
// The header sweep cannot see either: it is `-fsyntax-only`, and a no-op sink
// parses. So this TU is COMPILED to an object (`-c -ffreestanding -nostdinc++`
// against the ThreadX minimal libcpp, like `one_node_type_freestanding.cpp`),
// and the recipe asks the object's symbol table whether it calls
// `nros_log_emit_at`, the entry point every `nros_log` sink hangs off.
//
// The recipe compiles it once per refusal, selecting one with `-DREFUSAL=<n>`,
// so each object holds one refusal and the inline helpers it reaches. That is
// what makes the question per refusal: a single object holding both would
// answer yes if either one still reached `nros_log`.

#include <nros/nros.hpp>

#if REFUSAL == 1
extern "C" void refusal_failed_create(void) {
    ::rclcpp::detail::require_created(::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized),
                                      "create_publisher", "chatter");
}
#elif REFUSAL == 2
extern "C" int refusal_init_argv(int argc, char const* const* argv) {
    return static_cast<int>(::rclcpp::detail::apply_ros_args(nullptr, argc, argv));
}
#else
#error "compile with -DREFUSAL=1 (failed create) or -DREFUSAL=2 (--ros-args)"
#endif
