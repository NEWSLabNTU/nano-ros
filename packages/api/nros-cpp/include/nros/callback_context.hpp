// nros-cpp: carrying a one-word callback capture through the arena's context slot
// Freestanding C++ — no exceptions, no STL required

/**
 * @file callback_context.hpp
 * @ingroup grp_support
 * @brief `nros::detail::fn_to_context` / `fn_from_context` — W1's shape for a
 *        capture that is exactly one function pointer.
 */

#ifndef NROS_CPP_CALLBACK_CONTEXT_HPP
#define NROS_CPP_CALLBACK_CONTEXT_HPP

#include <stddef.h>
#include <string.h> // memcpy — `<cstring>` isn't in Zephyr's minimal libcpp

namespace nros {
namespace detail {

/// WHY THIS EXISTS — phase-456 W3.
///
/// An arena registration takes a `void* context` and hands it back to the
/// trampoline. What a dispatch SERVICE or CLIENT needs to carry across that
/// boundary is exactly one thing: the user's typed handler, which the SFINAE
/// guard on `create_service` / `create_client` already restricts to a plain
/// function pointer. One word.
///
/// Before W3 the context was `&out`, the C++ `Service<S>` / `Client<S>`
/// object, and the trampoline read `self->user_fn_` out of it. That made the
/// arena hold the address of a caller-side object, which is the "must NOT be
/// moved after register" hazard `service.hpp`'s move constructor used to warn
/// about — a warning the API only needed because the C++ side kept an object
/// the arena did not.
///
/// This is phase-456 W1's answer (the arena carries the callback's capture)
/// for the case where the capture fits in the slot that already exists: the
/// handler IS the context, so the arena copies it by value like any other
/// capture and nothing of the caller's is referenced after registration. W1's
/// `nros_cpp_subscription_register_capturing` is the general form, for a
/// capture wider than one word; neither service nor client has one.
///
/// WHY `memcpy` AND NOT `reinterpret_cast`, MEASURED.
///
/// A `reinterpret_cast` between a function pointer and an object pointer is
/// conditionally-supported ([expr.reinterpret.cast]/8), so what it does is a
/// property of the implementation rather than of the language. The obvious
/// defence — "a warning would tell us" — does not hold, and the measurement is
/// the reason this is spelled out rather than assumed: on gcc 12.3 the cast is
/// silent under `-Wall -Wextra -Wpedantic -Werror` and is diagnosed only under
/// `-Wconditionally-supported`; on clang only under `-Wc++98-compat-pedantic`.
/// Neither flag is on any lane here, so that form would ship unremarked on a
/// target where it does not hold.
///
/// Copying the object representation is unconditionally defined, and it costs
/// nothing: both forms emit the same single `movq` at `-O2` (measured on
/// x86-64 gcc 12.3). `span.hpp` already reads a value back out of bytes this
/// way. The `static_assert` is what carries the safety the cast could not: a
/// target whose function pointers are wider than `void*` gets a compile error
/// naming this file, rather than a truncated handler.
template <typename Fn> inline void* fn_to_context(Fn fn) {
    static_assert(sizeof(Fn) == sizeof(void*),
                  "nros: a callback carried through the arena's context slot must be exactly one "
                  "word wide -- a wider capture needs the register_capturing entry point");
    void* ctx = nullptr;
    ::memcpy(&ctx, &fn, sizeof(ctx));
    return ctx;
}

/// The inverse of @ref fn_to_context, for a trampoline reading its context
/// back. A NULL context yields a NULL function pointer, which every call site
/// checks before dispatching.
template <typename Fn> inline Fn fn_from_context(void* ctx) {
    static_assert(sizeof(Fn) == sizeof(void*),
                  "nros: a callback carried through the arena's context slot must be exactly one "
                  "word wide -- a wider capture needs the register_capturing entry point");
    Fn fn = nullptr;
    ::memcpy(&fn, &ctx, sizeof(fn));
    return fn;
}

} // namespace detail
} // namespace nros

#endif // NROS_CPP_CALLBACK_CONTEXT_HPP
