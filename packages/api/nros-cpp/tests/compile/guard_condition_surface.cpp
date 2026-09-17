// phase-417 W4.e — the C++ guard-condition surface, INSTANTIATED.
//
// `nros::GuardCondition`'s members are ordinary non-template methods on a
// non-template class, so the header loop in `just check cpp` parses them and
// compiles NONE of them: a method nobody calls is never type-checked, which is
// the "reads as coverage" shape one level down from `required-features`. This
// TU calls each one, so a body that drifted from the FFI it forwards to fails
// here rather than in a consumer's build.
//
// What it pins:
//
//   * creation is on the NODE, one call, callback bound at creation — the
//     shape W4.e settled for all three languages. `Node::create_guard_condition`
//     is the only way to initialise one, and C++ has always had it; this file
//     is what keeps it compiled.
//   * `is_triggered()` / `clear()` — new here. The ledger recorded the
//     three-language disagreement in its own words: C shipped the polling
//     readers, "a polling C++ or Rust user cannot do what a polling C user
//     can". They forward to `nros_cpp_guard_condition_{is_triggered,clear}`,
//     which read the ARENA flag, the one `trigger()` writes.
//   * an UNINITIALISED guard condition answers rather than dispatching into
//     empty storage: `is_triggered()` is false and `clear()` is
//     `NotInitialized`.
// What it does NOT pin, said plainly rather than implied: the absence of
// `set_on_trigger_callback`. rclcpp installs that hook after construction so an
// executor can attach itself to a guard condition somebody else made; ours is
// bound at creation and cannot be swapped (ledger:
// `cpp:GuardCondition::set_on_trigger_callback`, disposition ABSENT). An
// absent member is not a refusal with a diagnostic, so there is nothing here
// for a probe to read — turning it into a REFUSE-LOUD deleted overload is a
// disposition change, and a disposition change belongs in the ledger before it
// belongs in a header.

#include <nros/nros.hpp>

namespace nros_cpp_guard_condition_surface_test {

static unsigned g_fires = 0;

static void on_guard(void* context) {
    if (context != nullptr) *static_cast<unsigned*>(context) = 1;
    ++g_fires;
}

// The creation verb's exact shape, taken as a pointer-to-member so a changed
// parameter list is a compile error naming this line.
using CreateVerb = nros::Result (rclcpp::Node::*)(nros::GuardCondition&, nros_cpp_guard_callback_t,
                                                  void*);
static CreateVerb const kCreate = &rclcpp::Node::create_guard_condition;

// An ours-only capability with no upstream counterpart still has to compile.
static bool (nros::GuardCondition::*const kIsTriggered)() const =
    &nros::GuardCondition::is_triggered;
static nros::Result (nros::GuardCondition::*const kClear)() = &nros::GuardCondition::clear;
static nros::Result (nros::GuardCondition::*const kTrigger)() = &nros::GuardCondition::trigger;

// `trigger` and `clear` return `Result`, which is NROS_NODISCARD (RFC-0018:
// the error channel is the return value, because there are no exceptions), so
// a caller that drops one is warned. `is_triggered` answers a plain bool — the
// pointer-to-member declaration above is what states that, and it is a HARDER
// statement than a `<type_traits>` check would be: this file also builds
// `-nostdinc++` against a minimal libcpp, where `<type_traits>` need not exist.

void instantiate(rclcpp::Node& node);
void instantiate(rclcpp::Node& node) {
    nros::GuardCondition guard;

    // Before creation: the accessors ANSWER; they do not read empty storage.
    if (guard.is_valid()) ++g_fires;
    if (guard.is_triggered()) ++g_fires;
    nros::Result r = guard.clear();
    (void)r.code();

    unsigned ctx = 0;
    nros::Result created = (node.*kCreate)(guard, on_guard, &ctx);
    (void)created.code();

    nros::Result triggered = (guard.*kTrigger)();
    (void)triggered.code();
    if ((guard.*kIsTriggered)()) ++g_fires;
    nros::Result cleared = (guard.*kClear)();
    (void)cleared.code();
}

} // namespace nros_cpp_guard_condition_surface_test
