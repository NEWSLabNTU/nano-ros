// phase-427 W8 — EXPECTED FAILURE. The old `Expected<T>` spelling must WARN.
//
// `Expected<T>` is `ResultOf<T>` for one release. A deprecation nobody is told
// about is just an alias, and this is the half that proves the telling — the
// shape is deliberately a deprecated CLASS template rather than the deprecated
// ALIAS template the obvious reading asks for, because `[[deprecated]]` on an
// alias template warns on gcc 12.3 and is silent on clang 14. Measured, both
// standards. See the comment on `Expected` in `nros/result.hpp`.

#include <nros/result.hpp>

static nros::ResultOf<int> value_carrying() {
    return nros::ResultOf<int>::ok(7);
}

int uses_the_old_spelling() {
    nros::Expected<int> e = value_carrying();
    return e.value();
}
