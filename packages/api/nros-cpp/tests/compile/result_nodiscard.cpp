// phase-427 W8 — the POSITIVE half of the discarded-result assertion.
//
// A result that is looked at must compile clean under `-Werror=unused-result`,
// and so must every `Result` our own headers produce along the way: this TU
// includes the umbrella, so a header that drops a `Result` on the floor fails
// HERE rather than in a user's build. That is not hypothetical — writing this
// probe is what surfaced `rclcpp::shutdown()` discarding `nros::shutdown()`
// and answering `true` unconditionally.
//
// It is compiled BEFORE the expected-failure probe beside it, because an
// expected-failure compile cannot tell "the attribute fired" from "the file is
// not there" — both are a non-zero c++, and the second reads as a pass.

#include <nros/nros.hpp>

static nros::Result value_less() {
    return nros::Result::success();
}

static nros::ResultOf<int> value_carrying() {
    return nros::ResultOf<int>::ok(7);
}

int main() {
    // Checked.
    nros::Result r = value_less();
    if (!r.ok()) return r.raw();

    // Checked, then consumed.
    nros::ResultOf<int> v = value_carrying();
    if (!v.ok()) return v.error_as_result().raw();

    // Deliberately ignored. `(void)` is the sanctioned way to say "I looked at
    // this and chose not to care" — the attribute exists to make the SILENT
    // version of that sentence impossible, not to forbid it being said.
    (void)value_less();

    return v.value();
}
