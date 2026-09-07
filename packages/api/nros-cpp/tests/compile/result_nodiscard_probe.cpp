// phase-427 W8 — EXPECTED FAILURE. A discarded result must not compile under a
// `-D warnings` lane.
//
// Both halves of the error channel are here, because `NROS_NODISCARD` sits on
// the template AND on its `void` specialization and those are two entities: an
// attribute lost from either one is a silence, and a probe that exercises one
// would report the other as fine.
//
// RFC-0089 W8's acceptance names `rclcpp::init(argc, argv)` as the call to
// discard. Measured, that call CANNOT be the probe: `rclcpp::init` returns
// `void` here (`nros.hpp`), deliberately — a ported API keeps upstream's
// channel, and upstream's `init` returns nothing. There is no result to drop,
// so the TU would compile clean and the gate would be vacuous. What the RFC's
// reasoning is actually about is the widening — `nros::init()` returns a
// `Result` where `rclcpp::init()` returns void, and it is the WIDENED call
// whose discarded value nothing would otherwise point at. That call is the one
// probed below.
//
// The lane greps for two `unused-result` diagnostics rather than trusting the
// exit code: a typo or a missing include path also exits non-zero, and would
// read as "the attribute fired".

#include <nros/nros.hpp>

static nros::Result value_less() {
    return nros::Result::success();
}

static nros::ResultOf<int> value_carrying() {
    return nros::ResultOf<int>::ok(7);
}

void discards_both() {
    value_less();     // nros::Result           == ResultOf<void>
    value_carrying(); // nros::ResultOf<int>
}
