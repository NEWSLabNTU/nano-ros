// phase-482 W4 — EXPECTED FAILURE. The old `nros::LifecycleNode` spelling must
// WARN, naming its replacement.
//
// `nros::LifecycleNode` is the one deprecated forwarder phase-482 keeps for a
// release (W6 retired every other one). `just check cpp` compiles this TU with
// `-Werror=deprecated-declarations` and requires it to FAIL with a diagnostic
// naming `rclcpp_lifecycle::LifecycleNode`; a clean compile would mean the
// attribute stopped reaching callers, and the forwarder would vanish next
// release with no one told.

#include <nros/lifecycle.hpp>

// A VARIABLE of the type, not a class deriving from it: GCC 12 warns on the
// former and is silent on a base-clause use (clang warns on both), so a
// derivation-only probe would pass on GCC while proving nothing.
inline void use_legacy() {
    nros::LifecycleNode legacy;
    (void)legacy;
}
